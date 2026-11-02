//! Props: meshes, materials and textures a game creates while it runs (a table, cards, chips,
//! characters), placed by per-instance transforms and drawn with the cooked world in every
//! pass: shadow cascades, the water reflection and the main view, with the same stylized
//! shading, tiled lamps and Look rules (a prop material's name is matched like a world
//! material's).
//!
//! Each frame the live, visible instances are batched by (material, mesh); their transforms go
//! to the frame slot's instance buffer in batch order and each batch is one instanced draw that
//! reads `props[first + instance_index]` (see `Prop` in common.wgsl). Meshes keep a CPU copy of
//! their triangles for [`Renderer::pick`].
//!
//! Handles are generational: using one after its object was destroyed is an error, never a
//! different object.
use super::View;
use super::math::{self, M4, V3};
use super::renderer::Renderer;
use super::scene::MaterialData;
use crate::gpu::pool::Pool;
use crate::gpu::{
    BindGroupId, BufferDesc, BufferId, BufferUsage, Device, Format, Memory, Resource, Result,
    TextureDesc, TextureId, TextureUsage, f32_bytes, u32_bytes,
};
use crate::world::Shading;

macro_rules! handle {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(u32);
        impl $name {
            /// The raw id, for hosts that pass handles across a language boundary.
            pub const fn raw(self) -> u32 {
                self.0
            }
            pub const fn from_raw(raw: u32) -> Self {
                Self(raw)
            }
        }
    };
}
handle!(
    /// A prop mesh.
    PropMesh
);
handle!(
    /// A prop material.
    PropMaterial
);
handle!(
    /// A prop texture.
    PropTexture
);
handle!(
    /// A placed prop: one mesh with one material.
    PropInstance
);

/// Vertices in model space and triangles (counter-clockwise front faces, like world meshes).
#[derive(Clone, Copy, Debug)]
pub struct MeshData<'a> {
    pub positions: &'a [V3],
    /// One per position; empty for flat shading from the face normals.
    pub normals: &'a [V3],
    /// One per position; empty for zeros.
    pub uvs: &'a [[f32; 2]],
    pub indices: &'a [u32],
}

/// How a prop's colour combines with what is behind it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BlendMode {
    /// Covers what is behind it and writes depth.
    #[default]
    Opaque,
    /// Mixed by its alpha; sorted, and writes no depth.
    Alpha,
    /// Added, scaled by its alpha (glows, sparks); sorted, and writes no depth.
    Add,
}

/// How a prop material looks. Colours are linear.
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialDesc {
    /// Look detail rules match this name.
    pub name: String,
    pub base_color: [f32; 4],
    pub emissive: V3,
    /// The emission follows the time of day's lamp level, like the world's lamps.
    pub lamp: bool,
    pub roughness: f32,
    pub metallic: f32,
    pub shading: Shading,
    /// Alpha-tested at this cutoff (`None`: opaque).
    pub alpha_cutoff: Option<f32>,
    pub base_texture: Option<PropTexture>,
    pub normal_texture: Option<PropTexture>,
    pub emissive_texture: Option<PropTexture>,
    pub normal_scale: f32,
    /// Only opaque scene materials cast shadows.
    pub casts_shadow: bool,
    pub blend: BlendMode,
    /// Drawn after grading, over the finished image, in exactly its colour and texture (unlit):
    /// labels, badges, rings. Overlays are never reflected and cast no shadows.
    pub overlay: bool,
    /// Hidden behind nearer scene surfaces (`false`: always drawn on top).
    pub depth_test: bool,
    /// Draw order among blended and overlay props: lower first; ties draw far to near.
    pub order: i32,
}

impl Default for MaterialDesc {
    fn default() -> Self {
        Self {
            name: String::new(),
            base_color: [1.0; 4],
            emissive: [0.0; 3],
            lamp: false,
            roughness: 0.8,
            metallic: 0.0,
            shading: Shading::Standard,
            alpha_cutoff: None,
            base_texture: None,
            normal_texture: None,
            emissive_texture: None,
            normal_scale: 1.0,
            casts_shadow: true,
            blend: BlendMode::Opaque,
            overlay: false,
            depth_test: true,
            order: 0,
        }
    }
}

impl MaterialDesc {
    /// A description from JSON, for hosts across a language boundary. Every field is optional
    /// and unknown fields are errors:
    ///
    /// ```json
    /// {"name": "felt", "color": [0.1, 0.3, 0.2, 1], "emissive": [0, 0, 0], "lamp": false,
    ///  "roughness": 0.8, "metallic": 0, "shading": "standard",
    ///  "alphaCutoff": 0.5, "baseTexture": 3, "normalTexture": 4, "emissiveTexture": 5,
    ///  "normalScale": 1, "castsShadow": true, "blend": "opaque", "overlay": false,
    ///  "depthTest": true, "order": 0}
    /// ```
    ///
    /// Colours are linear; `shading` is `standard`, `foliage`, `water` or `unlit`; `blend` is
    /// `opaque`, `alpha` or `add`; textures are raw [`PropTexture`] handles.
    pub fn from_json(text: &[u8]) -> Result<Self> {
        use crate::assets::json::{self, Value};
        const KNOWN: [&str; 17] = [
            "name",
            "color",
            "emissive",
            "lamp",
            "roughness",
            "metallic",
            "shading",
            "alphaCutoff",
            "baseTexture",
            "normalTexture",
            "emissiveTexture",
            "normalScale",
            "castsShadow",
            "blend",
            "overlay",
            "depthTest",
            "order",
        ];
        let v = json::parse(text).map_err(|e| format!("material: {e}"))?;
        let Value::Object(map) = &v else {
            return Err("material: expected a JSON object".into());
        };
        if let Some(unknown) = map.keys().find(|k| !KNOWN.contains(&k.as_str())) {
            return Err(format!("material: unknown field {unknown:?}"));
        }
        let field = |key: &str, e: String| format!("material {key}: {e}");
        let num = |key: &str| -> Result<Option<f32>> {
            v.get(key)
                .map(|x| x.float().map_err(|e| field(key, e)))
                .transpose()
        };
        let floats = |key: &str, n: usize| -> Result<Option<Vec<f32>>> {
            let Some(x) = v.get(key) else {
                return Ok(None);
            };
            let a = x.array().map_err(|e| field(key, e))?;
            if a.len() != n {
                return Err(field(key, format!("expected {n} numbers")));
            }
            a.iter()
                .map(|x| x.float().map_err(|e| field(key, e)))
                .collect::<Result<Vec<f32>>>()
                .map(Some)
        };
        let flag = |key: &str| -> Result<Option<bool>> {
            match v.get(key) {
                None => Ok(None),
                Some(Value::Bool(b)) => Ok(Some(*b)),
                Some(_) => Err(field(key, "expected true or false".into())),
            }
        };
        let texture = |key: &str| -> Result<Option<PropTexture>> {
            match v.get(key) {
                None | Some(Value::Null) => Ok(None),
                Some(x) => x
                    .unsigned()
                    .ok()
                    .and_then(|n| u32::try_from(n).ok())
                    .filter(|&n| n != 0)
                    .map(|n| Some(PropTexture(n)))
                    .ok_or_else(|| field(key, "expected a texture handle".into())),
            }
        };
        let mut d = MaterialDesc::default();
        if let Some(name) = v.get("name") {
            d.name = name.string().map_err(|e| field("name", e))?.into();
        }
        if let Some(c) = floats("color", 4)? {
            d.base_color = [c[0], c[1], c[2], c[3]];
        }
        if let Some(e) = floats("emissive", 3)? {
            d.emissive = [e[0], e[1], e[2]];
        }
        if let Some(b) = flag("lamp")? {
            d.lamp = b;
        }
        if let Some(x) = num("roughness")? {
            d.roughness = x.clamp(0.0, 1.0);
        }
        if let Some(x) = num("metallic")? {
            d.metallic = x.clamp(0.0, 1.0);
        }
        if let Some(s) = v.get("shading") {
            d.shading = match s.string().map_err(|e| field("shading", e))? {
                "standard" => Shading::Standard,
                "foliage" => Shading::Foliage,
                "water" => Shading::Water,
                "unlit" => Shading::Unlit,
                other => return Err(field("shading", format!("unknown {other:?}"))),
            };
        }
        d.alpha_cutoff = num("alphaCutoff")?;
        d.base_texture = texture("baseTexture")?;
        d.normal_texture = texture("normalTexture")?;
        d.emissive_texture = texture("emissiveTexture")?;
        if let Some(x) = num("normalScale")? {
            d.normal_scale = x;
        }
        if let Some(b) = flag("castsShadow")? {
            d.casts_shadow = b;
        }
        if let Some(b) = v.get("blend") {
            d.blend = match b.string().map_err(|e| field("blend", e))? {
                "opaque" => BlendMode::Opaque,
                "alpha" => BlendMode::Alpha,
                "add" => BlendMode::Add,
                other => return Err(field("blend", format!("unknown {other:?}"))),
            };
        }
        if let Some(b) = flag("overlay")? {
            d.overlay = b;
        }
        if let Some(b) = flag("depthTest")? {
            d.depth_test = b;
        }
        if let Some(o) = num("order")? {
            if o.fract() != 0.0 || o.abs() > 1.0e6 {
                return Err(field("order", "expected a whole number".into()));
            }
            d.order = o as i32;
        }
        Ok(d)
    }
}

/// Bytes per instance in the instance buffer (`Prop` in common.wgsl: two matrices, a tint).
const PROP_BYTES: u64 = 144;
const MIN_CAPACITY: u32 = 64;

pub(super) struct MeshEntry {
    vertices: BufferId,
    indices: BufferId,
    index_count: u32,
    positions: Vec<V3>,
    uvs: Vec<[f32; 2]>,
    triangles: Vec<u32>,
    min: V3,
    max: V3,
}

pub(super) struct MaterialEntry {
    desc: MaterialDesc,
    data: MaterialData,
    /// `@group(1)` of the scene pipelines, or of the overlay pipelines for an overlay.
    group: BindGroupId,
    shadow_group: Option<BindGroupId>,
    detail: [f32; 16],
}

impl MaterialEntry {
    fn layer(&self) -> Layer {
        if self.desc.overlay {
            Layer::Overlay
        } else if self.desc.blend == BlendMode::Opaque {
            Layer::Opaque
        } else {
            Layer::Blended
        }
    }
}

/// Where a prop is drawn: in the scene (opaque, batched, or blended, sorted) or over the image.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Layer {
    Opaque,
    Blended,
    Overlay,
}

struct InstanceEntry {
    mesh: PropMesh,
    material: PropMaterial,
    model: M4,
    normal: M4,
    tint: [f32; 4],
    visible: bool,
    /// Pick layers (bits); 0 is never picked.
    pick: u32,
}

/// One instanced draw of this frame: instances `first..first + count` of the buffer.
#[derive(Clone, Copy, Debug)]
pub(super) struct Batch {
    mesh: u32,
    material: u32,
    first: u32,
    count: u32,
    layer: Layer,
}

/// A frame slot's instance buffer and the bind groups that read it.
pub(super) struct PropSlot {
    buffer: BufferId,
    /// Main view and reflection (`@group(0)` of the prop scene pipeline).
    scene: [BindGroupId; 2],
    shadow: BindGroupId,
    /// `@group(0)` of the overlay pipelines.
    overlay: BindGroupId,
}

#[derive(Default)]
pub(super) struct Props {
    meshes: Pool<MeshEntry>,
    materials: Pool<MaterialEntry>,
    textures: Pool<TextureId>,
    instances: Pool<InstanceEntry>,
    slots: Vec<Option<PropSlot>>,
    capacity: u32,
    batches: Vec<Batch>,
    /// World bounds of this frame's visible instances.
    bounds: Option<(V3, V3)>,
}

fn normal_matrix(m: &M4) -> M4 {
    // The inverse transpose keeps normals perpendicular under non-uniform scale.
    let inv = math::invert(m).unwrap_or(math::IDENTITY);
    let mut t = [0.0f32; 16];
    for c in 0..4 {
        for r in 0..4 {
            t[c * 4 + r] = inv[r * 4 + c];
        }
    }
    t
}

fn bounds(points: &[V3]) -> (V3, V3) {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for p in points {
        for k in 0..3 {
            min[k] = min[k].min(p[k]);
            max[k] = max[k].max(p[k]);
        }
    }
    (min, max)
}

/// Interleaved position, normal, uv (the world vertex layout).
fn interleave(positions: &[V3], normals: &[V3], uvs: &[[f32; 2]], indices: &[u32]) -> Vec<f32> {
    let flat;
    let normals = if normals.is_empty() {
        let mut n = vec![[0.0f32; 3]; positions.len()];
        for t in indices.chunks_exact(3) {
            let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
            let face = math::cross(
                math::sub(positions[b], positions[a]),
                math::sub(positions[c], positions[a]),
            );
            for v in [a, b, c] {
                n[v] = math::add(n[v], face);
            }
        }
        flat = n.into_iter().map(math::normalize).collect::<Vec<_>>();
        &flat[..]
    } else {
        normals
    };
    let mut out = Vec::with_capacity(positions.len() * 8);
    for (i, p) in positions.iter().enumerate() {
        out.extend_from_slice(p);
        out.extend_from_slice(&normals[i]);
        out.extend_from_slice(uvs.get(i).unwrap_or(&[0.0, 0.0]));
    }
    out
}

/// Ray–triangle distance (Möller–Trumbore), both faces.
fn ray_triangle(o: V3, d: V3, a: V3, b: V3, c: V3) -> Option<f32> {
    let e1 = math::sub(b, a);
    let e2 = math::sub(c, a);
    let p = math::cross(d, e2);
    let det = math::dot(e1, p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let s = math::sub(o, a);
    let u = math::dot(s, p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = math::cross(s, e1);
    let v = math::dot(d, q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = math::dot(e2, q) * inv;
    (t > 0.0).then_some(t)
}

/// Ray–box entry distance (slab test), or `None` when the ray misses.
fn ray_box(o: V3, d: V3, min: V3, max: V3) -> Option<f32> {
    let (mut near, mut far) = (0.0f32, f32::INFINITY);
    for k in 0..3 {
        if d[k].abs() < 1e-12 {
            if o[k] < min[k] || o[k] > max[k] {
                return None;
            }
            continue;
        }
        let (a, b) = ((min[k] - o[k]) / d[k], (max[k] - o[k]) / d[k]);
        near = near.max(a.min(b));
        far = far.min(a.max(b));
        if near > far {
            return None;
        }
    }
    Some(near)
}

/// A pick: the nearest prop under a screen point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pick {
    pub instance: PropInstance,
    /// Distance from the eye along the ray, in metres.
    pub distance: f32,
    pub point: V3,
}

impl Renderer {
    fn prop_texture_id(&self, t: Option<PropTexture>, fallback: TextureId) -> Result<TextureId> {
        match t {
            None => Ok(fallback),
            Some(t) => self
                .props
                .textures
                .get(t.0)
                .copied()
                .ok_or_else(|| format!("{t:?} was destroyed")),
        }
    }

    /// An RGBA8 texture (sRGB colour or linear data such as normal maps) with a full mip chain.
    pub fn create_prop_texture(
        &mut self,
        device: &mut Device<'_>,
        (width, height): (u32, u32),
        rgba: &[u8],
        srgb: bool,
    ) -> Result<PropTexture> {
        let mips = 32 - width.max(height).leading_zeros();
        let t = device.create_texture(&TextureDesc {
            width,
            height,
            layers: 1,
            mips,
            format: if srgb {
                Format::Rgba8Srgb
            } else {
                Format::Rgba8Unorm
            },
            usage: TextureUsage::SAMPLED
                | TextureUsage::COPY_DST
                | TextureUsage::COPY_SRC
                | TextureUsage::COLOR_TARGET,
        })?;
        let written = device
            .write_texture(t, 0, 0, rgba)
            .and_then(|()| device.generate_mips(t));
        if let Err(e) = written {
            device.destroy_texture(t);
            return Err(e);
        }
        Ok(PropTexture(self.props.textures.insert(t)?))
    }

    /// Replaces a prop texture's pixels (same size) and its mips.
    pub fn update_prop_texture(
        &mut self,
        device: &mut Device<'_>,
        texture: PropTexture,
        rgba: &[u8],
    ) -> Result<()> {
        let t = self.prop_texture_id(Some(texture), self.dummy)?;
        device.write_texture(t, 0, 0, rgba)?;
        device.generate_mips(t)
    }

    /// Destroys a texture; materials that use it must be updated or destroyed first.
    pub fn destroy_prop_texture(&mut self, device: &mut Device<'_>, texture: PropTexture) {
        if let Some(t) = self.props.textures.remove(texture.0) {
            device.destroy_texture(t);
        }
    }

    pub fn create_prop_mesh(
        &mut self,
        device: &mut Device<'_>,
        mesh: &MeshData<'_>,
    ) -> Result<PropMesh> {
        let n = mesh.positions.len();
        if n == 0 || mesh.indices.is_empty() || mesh.indices.len() % 3 != 0 {
            return Err("prop mesh: needs vertices and whole triangles".into());
        }
        if (!mesh.normals.is_empty() && mesh.normals.len() != n)
            || (!mesh.uvs.is_empty() && mesh.uvs.len() != n)
        {
            return Err("prop mesh: normals and uvs need one entry per position".into());
        }
        if let Some(&i) = mesh.indices.iter().find(|&&i| i as usize >= n) {
            return Err(format!("prop mesh: index {i} is outside {n} vertices"));
        }
        let data = interleave(mesh.positions, mesh.normals, mesh.uvs, mesh.indices);
        let vertices = device.create_buffer(&BufferDesc {
            size: data.len() as u64 * 4,
            usage: BufferUsage::VERTEX | BufferUsage::COPY_DST,
            memory: Memory::Device,
        })?;
        device.write_buffer(vertices, 0, &f32_bytes(&data))?;
        let indices = device.create_buffer(&BufferDesc {
            size: mesh.indices.len() as u64 * 4,
            usage: BufferUsage::INDEX | BufferUsage::COPY_DST,
            memory: Memory::Device,
        })?;
        device.write_buffer(indices, 0, &u32_bytes(mesh.indices))?;
        let (min, max) = bounds(mesh.positions);
        let entry = MeshEntry {
            vertices,
            indices,
            index_count: mesh.indices.len() as u32,
            positions: mesh.positions.to_vec(),
            uvs: if mesh.uvs.is_empty() {
                vec![[0.0; 2]; n]
            } else {
                mesh.uvs.to_vec()
            },
            triangles: mesh.indices.to_vec(),
            min,
            max,
        };
        Ok(PropMesh(self.props.meshes.insert(entry)?))
    }

    /// New vertex positions and normals for a mesh (a skinned or morphed character each frame);
    /// the triangles and uvs stay.
    pub fn update_prop_mesh(
        &mut self,
        device: &mut Device<'_>,
        mesh: PropMesh,
        positions: &[V3],
        normals: &[V3],
    ) -> Result<()> {
        let m = self
            .props
            .meshes
            .get_mut(mesh.0)
            .ok_or_else(|| format!("{mesh:?} was destroyed"))?;
        if positions.len() != m.positions.len()
            || (!normals.is_empty() && normals.len() != positions.len())
        {
            return Err("update_prop_mesh: the vertex count must stay the same".into());
        }
        let data = interleave(positions, normals, &m.uvs, &m.triangles);
        device.write_buffer(m.vertices, 0, &f32_bytes(&data))?;
        m.positions.copy_from_slice(positions);
        (m.min, m.max) = bounds(positions);
        Ok(())
    }

    /// Destroys a mesh and every instance of it.
    pub fn destroy_prop_mesh(&mut self, device: &mut Device<'_>, mesh: PropMesh) {
        if let Some(m) = self.props.meshes.remove(mesh.0) {
            let users: Vec<u32> = self
                .props
                .instances
                .iter()
                .filter(|(_, i)| i.mesh == mesh)
                .map(|(id, _)| id)
                .collect();
            for id in users {
                self.props.instances.remove(id);
            }
            device.destroy_buffer(m.vertices);
            device.destroy_buffer(m.indices);
        }
    }

    fn material_entry(
        &mut self,
        device: &mut Device<'_>,
        desc: &MaterialDesc,
    ) -> Result<MaterialEntry> {
        let shading = match desc.shading {
            Shading::Standard => 0.0,
            Shading::Foliage => 1.0,
            Shading::Water => 2.0,
            Shading::Unlit => 3.0,
        };
        let data = MaterialData {
            base_color: desc.base_color,
            emissive: [
                desc.emissive[0],
                desc.emissive[1],
                desc.emissive[2],
                if desc.lamp { 1.0 } else { 0.0 },
            ],
            params: [
                desc.alpha_cutoff.unwrap_or(-1.0),
                desc.roughness,
                desc.metallic,
                shading,
            ],
            flags: [
                f32::from(u8::from(desc.base_texture.is_some())),
                f32::from(u8::from(desc.normal_texture.is_some())),
                f32::from(u8::from(desc.emissive_texture.is_some())),
                desc.normal_scale,
            ],
            base: None,
            normal: None,
            emission: None,
        };
        let base = self.prop_texture_id(desc.base_texture, self.white)?;
        let normal = self.prop_texture_id(desc.normal_texture, self.flat)?;
        let emissive = self.prop_texture_id(desc.emissive_texture, self.white)?;
        let p = &self.pipelines;
        let group = if desc.overlay {
            device.create_bind_group(
                p.overlay[0][0],
                1,
                &[
                    (0, Resource::Texture(base)),
                    (3, Resource::Sampler(self.material_sampler)),
                ],
            )?
        } else {
            device.create_bind_group(
                p.scene_prop,
                1,
                &[
                    (0, Resource::Texture(base)),
                    (1, Resource::Texture(normal)),
                    (2, Resource::Texture(emissive)),
                    (3, Resource::Sampler(self.material_sampler)),
                ],
            )?
        };
        let opaque = !desc.overlay && desc.blend == BlendMode::Opaque;
        let shadow_group = if opaque && desc.casts_shadow && desc.alpha_cutoff.is_some() {
            Some(device.create_bind_group(
                p.shadow_prop_masked,
                1,
                &[
                    (0, Resource::Texture(base)),
                    (3, Resource::Sampler(self.material_sampler)),
                ],
            )?)
        } else {
            None
        };
        Ok(MaterialEntry {
            desc: desc.clone(),
            data,
            group,
            shadow_group,
            detail: super::renderer::detail_block(&self.look, &desc.name).0,
        })
    }

    pub fn create_prop_material(
        &mut self,
        device: &mut Device<'_>,
        desc: &MaterialDesc,
    ) -> Result<PropMaterial> {
        let entry = self.material_entry(device, desc)?;
        Ok(PropMaterial(self.props.materials.insert(entry)?))
    }

    /// Changes a material in place (every instance that uses it changes with it).
    pub fn update_prop_material(
        &mut self,
        device: &mut Device<'_>,
        material: PropMaterial,
        desc: &MaterialDesc,
    ) -> Result<()> {
        if self.props.materials.get(material.0).is_none() {
            return Err(format!("{material:?} was destroyed"));
        }
        let entry = self.material_entry(device, desc)?;
        if let Some(old) = self.props.materials.get_mut(material.0) {
            let old = std::mem::replace(old, entry);
            device.destroy_bind_group(old.group);
            if let Some(g) = old.shadow_group {
                device.destroy_bind_group(g);
            }
        }
        Ok(())
    }

    /// The description a material was created or last updated with.
    pub fn prop_material(&self, material: PropMaterial) -> Result<&MaterialDesc> {
        self.props
            .materials
            .get(material.0)
            .map(|m| &m.desc)
            .ok_or_else(|| format!("{material:?} was destroyed"))
    }

    /// Destroys a material and every instance that uses it.
    pub fn destroy_prop_material(&mut self, device: &mut Device<'_>, material: PropMaterial) {
        if let Some(m) = self.props.materials.remove(material.0) {
            let users: Vec<u32> = self
                .props
                .instances
                .iter()
                .filter(|(_, i)| i.material == material)
                .map(|(id, _)| id)
                .collect();
            for id in users {
                self.props.instances.remove(id);
            }
            device.destroy_bind_group(m.group);
            if let Some(g) = m.shadow_group {
                device.destroy_bind_group(g);
            }
        }
    }

    /// Places `mesh` with `material` at `model` (column-major, model to world), visible.
    pub fn create_prop_instance(
        &mut self,
        mesh: PropMesh,
        material: PropMaterial,
        model: &M4,
    ) -> Result<PropInstance> {
        if self.props.meshes.get(mesh.0).is_none() {
            return Err(format!("{mesh:?} was destroyed"));
        }
        if self.props.materials.get(material.0).is_none() {
            return Err(format!("{material:?} was destroyed"));
        }
        let entry = InstanceEntry {
            mesh,
            material,
            model: *model,
            normal: normal_matrix(model),
            tint: [1.0; 4],
            visible: true,
            pick: 1,
        };
        Ok(PropInstance(self.props.instances.insert(entry)?))
    }

    /// Moves an instance and shows or hides it.
    pub fn set_prop_instance(
        &mut self,
        instance: PropInstance,
        model: &M4,
        visible: bool,
    ) -> Result<()> {
        let i = self
            .props
            .instances
            .get_mut(instance.0)
            .ok_or_else(|| format!("{instance:?} was destroyed"))?;
        if i.model != *model {
            i.model = *model;
            i.normal = normal_matrix(model);
        }
        i.visible = visible;
        Ok(())
    }

    /// Gives an instance another material.
    pub fn set_prop_instance_material(
        &mut self,
        instance: PropInstance,
        material: PropMaterial,
    ) -> Result<()> {
        if self.props.materials.get(material.0).is_none() {
            return Err(format!("{material:?} was destroyed"));
        }
        let i = self
            .props
            .instances
            .get_mut(instance.0)
            .ok_or_else(|| format!("{instance:?} was destroyed"))?;
        i.material = material;
        Ok(())
    }

    /// Multiplies an instance's material colour and alpha (fades, greying, highlights) without
    /// a material of its own.
    pub fn set_prop_instance_tint(&mut self, instance: PropInstance, tint: [f32; 4]) -> Result<()> {
        if tint.iter().any(|x| !x.is_finite()) {
            return Err("set_prop_instance_tint: values must be finite".into());
        }
        self.props
            .instances
            .get_mut(instance.0)
            .ok_or_else(|| format!("{instance:?} was destroyed"))?
            .tint = tint;
        Ok(())
    }

    /// The pick layers an instance answers to (bits; 0: never picked). New instances are on
    /// layer 1.
    pub fn set_prop_instance_pick(&mut self, instance: PropInstance, layers: u32) -> Result<()> {
        self.props
            .instances
            .get_mut(instance.0)
            .ok_or_else(|| format!("{instance:?} was destroyed"))?
            .pick = layers;
        Ok(())
    }

    pub fn destroy_prop_instance(&mut self, instance: PropInstance) {
        self.props.instances.remove(instance.0);
    }

    /// Live prop instances.
    pub fn prop_instances(&self) -> usize {
        self.props.instances.iter().count()
    }

    /// The visible prop under pixel `(x, y)` of a `size` image of `view` (top-left origin) on
    /// one of the pick `layers`, tested against its triangles. Like the image: an overlay drawn
    /// on top of everything (no depth test) wins, the last drawn of them (highest order, then
    /// nearest); otherwise the nearest hit.
    pub fn pick(
        &self,
        view: &View,
        size: (u32, u32),
        (x, y): (f32, f32),
        layers: u32,
    ) -> Option<Pick> {
        let eye = self.eye(view, [0.0, 0.0], size);
        let ndc = [
            2.0 * x / size.0.max(1) as f32 - 1.0,
            2.0 * y / size.1.max(1) as f32 - 1.0,
        ];
        let unproject = |z: f32| {
            let p = math::transform(&eye.inv, [ndc[0], ndc[1], z, 1.0]);
            [p[0] / p[3], p[1] / p[3], p[2] / p[3]]
        };
        // Reversed depth: 1 is the near plane.
        let near = unproject(1.0);
        let far = unproject(0.5);
        let dir = math::normalize(math::sub(far, near));
        let origin = view.eye;
        // (on top, order, distance) of the best hit so far.
        let mut best: Option<(Pick, bool, i32)> = None;
        let better = |top: bool, order: i32, dist: f32, b: &Option<(Pick, bool, i32)>| match b {
            None => true,
            Some((p, btop, border)) => match (top, *btop) {
                (true, false) => true,
                (false, true) => false,
                (true, true) => order > *border || (order == *border && dist < p.distance),
                (false, false) => dist < p.distance,
            },
        };
        for (id, inst) in self.props.instances.iter() {
            if !inst.visible || inst.pick & layers == 0 {
                continue;
            }
            let (Some(mesh), Some(mat)) = (
                self.props.meshes.get(inst.mesh.0),
                self.props.materials.get(inst.material.0),
            ) else {
                continue;
            };
            let top = mat.desc.overlay && !mat.desc.depth_test;
            let order = mat.desc.order;
            // Into model space: the ray's origin and direction through the inverse transform.
            let Some(inv) = math::invert(&inst.model) else {
                continue;
            };
            let o = math::transform_point(&inv, origin);
            let tip = math::transform_point(&inv, math::add(origin, dir));
            let d = math::sub(tip, o);
            let Some(enter) = ray_box(o, d, mesh.min, mesh.max) else {
                continue;
            };
            if !better(top, order, enter, &best) {
                continue;
            }
            let mut hit: Option<f32> = None;
            for t in mesh.triangles.chunks_exact(3) {
                let (a, b, c) = (
                    mesh.positions[t[0] as usize],
                    mesh.positions[t[1] as usize],
                    mesh.positions[t[2] as usize],
                );
                if let Some(s) = ray_triangle(o, d, a, b, c)
                    && hit.is_none_or(|h| s < h)
                {
                    hit = Some(s);
                }
            }
            // `d` is the unit world direction mapped into model space, so `s` is in world metres.
            if let Some(s) = hit
                && better(top, order, s, &best)
            {
                let pick = Pick {
                    instance: PropInstance(id),
                    distance: s,
                    point: math::add(origin, math::scale(dir, s)),
                };
                best = Some((pick, top, order));
            }
        }
        best.map(|b| b.0)
    }

    /// Where a world point lands in a `size` image of `view`: pixel x and y (top-left origin)
    /// and the depth in metres from the eye, or `None` behind the eye.
    pub fn project(&self, view: &View, size: (u32, u32), point: V3) -> Option<[f32; 3]> {
        let eye = self.eye(view, [0.0, 0.0], size);
        let c = math::transform(&eye.view_proj, [point[0], point[1], point[2], 1.0]);
        if c[3] <= 1e-6 {
            return None;
        }
        Some([
            (c[0] / c[3] * 0.5 + 0.5) * size.0 as f32,
            (c[1] / c[3] * 0.5 + 0.5) * size.1 as f32,
            c[3],
        ])
    }

    /// World bounds of the visible props, as of the last frame recorded.
    pub(super) fn props_bounds(&self) -> Option<(V3, V3)> {
        self.props.bounds
    }

    /// Recomputes the prop detail blocks after the Look changed.
    pub(super) fn restyle_props(&mut self) {
        let look = &self.look;
        let ids: Vec<u32> = self.props.materials.iter().map(|(id, _)| id).collect();
        for id in ids {
            if let Some(m) = self.props.materials.get_mut(id) {
                m.detail = super::renderer::detail_block(look, &m.desc.name).0;
            }
        }
    }

    /// Forgets the per-slot bind groups (after the frame slots were rebuilt).
    pub(super) fn reset_prop_slots(&mut self, device: &mut Device<'_>) {
        for s in std::mem::take(&mut self.props.slots).into_iter().flatten() {
            for g in s.scene.into_iter().chain([s.shadow, s.overlay]) {
                device.destroy_bind_group(g);
            }
            device.destroy_buffer(s.buffer);
        }
    }

    /// Orders this frame's visible instances and writes them to the slot's instance buffer
    /// (growing it, and rebuilding its bind groups, when needed): opaque ones batched by
    /// (material, mesh), then blended ones and then overlays one by one, each sorted by order
    /// and far to near from `eye`.
    pub(super) fn prepare_props(
        &mut self,
        device: &mut Device<'_>,
        slot: usize,
        eye: V3,
    ) -> Result<()> {
        struct Live<'a> {
            layer: Layer,
            order: i32,
            /// Negative squared distance: far first.
            far: f32,
            material: u32,
            mesh: u32,
            inst: &'a InstanceEntry,
        }
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        let mut live: Vec<Live> = Vec::new();
        for (_, inst) in self.props.instances.iter() {
            let (Some(m), Some(mat)) = (
                self.props.meshes.get(inst.mesh.0),
                self.props.materials.get(inst.material.0),
            ) else {
                continue;
            };
            if !inst.visible {
                continue;
            }
            let layer = mat.layer();
            let (mut blo, mut bhi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
            for corner in 0..8 {
                let p = [
                    if corner & 1 == 0 { m.min[0] } else { m.max[0] },
                    if corner & 2 == 0 { m.min[1] } else { m.max[1] },
                    if corner & 4 == 0 { m.min[2] } else { m.max[2] },
                ];
                let w = math::transform_point(&inst.model, p);
                for k in 0..3 {
                    blo[k] = blo[k].min(w[k]);
                    bhi[k] = bhi[k].max(w[k]);
                }
            }
            if layer != Layer::Overlay {
                for k in 0..3 {
                    lo[k] = lo[k].min(blo[k]);
                    hi[k] = hi[k].max(bhi[k]);
                }
            }
            let centre = math::scale(math::add(blo, bhi), 0.5);
            let to = math::sub(centre, eye);
            live.push(Live {
                layer,
                order: if layer == Layer::Opaque {
                    0
                } else {
                    mat.desc.order
                },
                far: if layer == Layer::Opaque {
                    0.0
                } else {
                    -math::dot(to, to)
                },
                material: inst.material.0,
                mesh: inst.mesh.0,
                inst,
            });
        }
        live.sort_by(|a, b| {
            (a.layer, a.order)
                .cmp(&(b.layer, b.order))
                .then(a.far.total_cmp(&b.far))
                .then((a.material, a.mesh).cmp(&(b.material, b.mesh)))
        });
        let mut batches: Vec<Batch> = Vec::new();
        let mut data: Vec<f32> = Vec::with_capacity(live.len() * 36);
        for (n, l) in live.iter().enumerate() {
            match batches.last_mut() {
                Some(b)
                    if l.layer == Layer::Opaque
                        && b.layer == Layer::Opaque
                        && b.material == l.material
                        && b.mesh == l.mesh =>
                {
                    b.count += 1;
                }
                _ => batches.push(Batch {
                    mesh: l.mesh,
                    material: l.material,
                    first: n as u32,
                    count: 1,
                    layer: l.layer,
                }),
            }
            data.extend_from_slice(&l.inst.model);
            data.extend_from_slice(&l.inst.normal);
            data.extend_from_slice(&l.inst.tint);
        }
        let count = live.len() as u32;
        drop(live);
        self.props.batches = batches;
        self.props.bounds = (lo[0] <= hi[0]).then_some((lo, hi));
        let needed = count.max(MIN_CAPACITY);
        if needed > self.props.capacity {
            self.reset_prop_slots(device);
            self.props.capacity = needed.next_power_of_two();
        }
        if self.props.slots.len() <= slot {
            self.props.slots.resize_with(slot + 1, || None);
        }
        if self.props.slots[slot].is_none() {
            self.props.slots[slot] = Some(self.create_prop_slot(device, slot)?);
        }
        if !data.is_empty()
            && let Some(s) = &self.props.slots[slot]
        {
            device.write_buffer(s.buffer, 0, &f32_bytes(&data))?;
        }
        Ok(())
    }

    /// Whether this frame has props to draw over the image.
    pub(super) fn has_overlays(&self) -> bool {
        self.props.batches.iter().any(|b| b.layer == Layer::Overlay)
    }

    fn create_prop_slot(&self, device: &mut Device<'_>, slot: usize) -> Result<PropSlot> {
        let buffer = device.create_buffer(&BufferDesc {
            size: u64::from(self.props.capacity) * PROP_BYTES,
            usage: BufferUsage::STORAGE,
            memory: Memory::Upload,
        })?;
        let buf = |id| Resource::Buffer {
            id,
            offset: 0,
            size: 0,
        };
        let s = &self.slots[slot];
        let t = &self.targets;
        let p = &self.pipelines;
        let mut scene = Vec::new();
        for v in 0..2 {
            let reflection = if v == 0 { t.refl_hdr } else { self.dummy };
            scene.push(device.create_bind_group(
                p.scene_prop,
                0,
                &[
                    (0, buf(s.frame[v])),
                    (1, buf(self.scene.light_buffer)),
                    (2, buf(s.tiles[v])),
                    (3, buf(s.index[v])),
                    (4, Resource::Texture(self.shadow_map)),
                    (5, Resource::Sampler(self.shadow_sampler)),
                    (6, Resource::Texture(reflection)),
                    (7, Resource::Sampler(self.linear_clamp)),
                    (8, Resource::Texture(self.ground_depth)),
                    (9, Resource::Texture(self.ground_mask)),
                    (10, Resource::Texture(self.world_depth)),
                    (11, buf(buffer)),
                ],
            )?);
        }
        let shadow = device.create_bind_group(p.shadow_prop, 0, &[(11, buf(buffer))])?;
        let overlay = device.create_bind_group(
            p.overlay[0][0],
            0,
            &[(0, buf(s.frame[0])), (11, buf(buffer))],
        )?;
        Ok(PropSlot {
            buffer,
            scene: [scene[0], scene[1]],
            shadow,
            overlay,
        })
    }

    /// Draws this frame's prop batches into a shadow cascade (`light_vp`).
    pub(super) fn draw_prop_shadows(
        &self,
        enc: &mut crate::gpu::Encoder,
        slot: usize,
        light_vp: &M4,
    ) {
        let Some(Some(s)) = self.props.slots.get(slot) else {
            return;
        };
        let p = &self.pipelines;
        let mut bound = None;
        for b in &self.props.batches {
            let (Some(mesh), Some(mat)) = (
                self.props.meshes.get(b.mesh),
                self.props.materials.get(b.material),
            ) else {
                continue;
            };
            if b.layer != Layer::Opaque || !mat.desc.casts_shadow {
                continue;
            }
            let masked = mat.shadow_group.is_some();
            if bound != Some(masked) {
                enc.set_pipeline(if masked {
                    p.shadow_prop_masked
                } else {
                    p.shadow_prop
                });
                enc.set_bind_group(0, s.shadow);
                bound = Some(masked);
            }
            if let Some(g) = mat.shadow_group {
                enc.set_bind_group(1, g);
            }
            let mut push = [0.0f32; 24];
            push[..16].copy_from_slice(light_vp);
            push[16..20].copy_from_slice(&[
                mat.data.params[0],
                mat.data.base_color[3],
                mat.data.flags[0],
                0.0,
            ]);
            push[20] = b.first as f32;
            enc.push_constants(&f32_bytes(&push));
            enc.set_vertex_buffer(mesh.vertices, 0);
            enc.set_index_buffer(mesh.indices, 0);
            enc.draw_indexed(mesh.index_count, b.count, 0, 0, 0);
        }
    }

    /// The `Draw` push constants of a prop batch.
    fn prop_push(mat: &MaterialEntry, first: u32, encode_srgb: bool) -> [f32; 32] {
        let mut push = [0.0f32; 32];
        push[..16].copy_from_slice(&mat.detail);
        let mut base = mat.data.base_color;
        if mat.detail[0] == 0.0 && mat.detail[8] > 0.5 {
            base[..3].copy_from_slice(&mat.detail[5..8]);
        }
        push[12] = first as f32;
        push[13] = if encode_srgb { 1.0 } else { 0.0 };
        push[14] = if mat.layer() == Layer::Opaque {
            0.0
        } else {
            1.0
        };
        push[16..20].copy_from_slice(&base);
        push[20..24].copy_from_slice(&mat.data.emissive);
        push[24..28].copy_from_slice(&mat.data.params);
        push[28..32].copy_from_slice(&mat.data.flags);
        push
    }

    /// Draws this frame's opaque prop batches with the scene shading (`view` 0 main, 1
    /// reflection).
    pub(super) fn draw_props(&self, enc: &mut crate::gpu::Encoder, slot: usize, view: usize) {
        let Some(Some(s)) = self.props.slots.get(slot) else {
            return;
        };
        if !self.props.batches.iter().any(|b| b.layer == Layer::Opaque) {
            return;
        }
        enc.set_pipeline(self.pipelines.scene_prop);
        enc.set_bind_group(0, s.scene[view]);
        let mut bound = None;
        for b in self
            .props
            .batches
            .iter()
            .filter(|b| b.layer == Layer::Opaque)
        {
            let (Some(mesh), Some(mat)) = (
                self.props.meshes.get(b.mesh),
                self.props.materials.get(b.material),
            ) else {
                continue;
            };
            if view == 1 && mat.data.params[3] == 2.0 {
                continue;
            }
            if bound != Some(b.material) {
                enc.set_bind_group(1, mat.group);
                bound = Some(b.material);
            }
            enc.push_constants(&f32_bytes(&Self::prop_push(mat, b.first, false)));
            enc.set_vertex_buffer(mesh.vertices, 0);
            enc.set_index_buffer(mesh.indices, 0);
            enc.draw_indexed(mesh.index_count, b.count, 0, 0, 0);
        }
    }

    /// Draws this frame's blended scene props, sorted, after the opaque scene (main view).
    pub(super) fn draw_blended_props(&self, enc: &mut crate::gpu::Encoder, slot: usize) {
        let Some(Some(s)) = self.props.slots.get(slot) else {
            return;
        };
        let mut pipeline = None;
        for b in self
            .props
            .batches
            .iter()
            .filter(|b| b.layer == Layer::Blended)
        {
            let (Some(mesh), Some(mat)) = (
                self.props.meshes.get(b.mesh),
                self.props.materials.get(b.material),
            ) else {
                continue;
            };
            let p = self.pipelines.scene_prop_blend[usize::from(mat.desc.blend == BlendMode::Add)];
            if pipeline != Some(p) {
                enc.set_pipeline(p);
                enc.set_bind_group(0, s.scene[0]);
                pipeline = Some(p);
            }
            enc.set_bind_group(1, mat.group);
            enc.push_constants(&f32_bytes(&Self::prop_push(mat, b.first, false)));
            enc.set_vertex_buffer(mesh.vertices, 0);
            enc.set_index_buffer(mesh.indices, 0);
            enc.draw_indexed(mesh.index_count, b.count, 0, 0, 0);
        }
    }

    /// Draws this frame's overlays into the finished image (a pass over the output target and
    /// the scene depth); `encode_srgb` when the target does not encode sRGB itself.
    pub(super) fn draw_overlays(
        &self,
        enc: &mut crate::gpu::Encoder,
        slot: usize,
        encode_srgb: bool,
    ) {
        let Some(Some(s)) = self.props.slots.get(slot) else {
            return;
        };
        let mut pipeline = None;
        for b in self
            .props
            .batches
            .iter()
            .filter(|b| b.layer == Layer::Overlay)
        {
            let (Some(mesh), Some(mat)) = (
                self.props.meshes.get(b.mesh),
                self.props.materials.get(b.material),
            ) else {
                continue;
            };
            let blend = match mat.desc.blend {
                BlendMode::Opaque => 0,
                BlendMode::Alpha => 1,
                BlendMode::Add => 2,
            };
            let p = self.pipelines.overlay[blend][usize::from(mat.desc.depth_test)];
            if pipeline != Some(p) {
                enc.set_pipeline(p);
                enc.set_bind_group(0, s.overlay);
                pipeline = Some(p);
            }
            enc.set_bind_group(1, mat.group);
            enc.push_constants(&f32_bytes(&Self::prop_push(mat, b.first, encode_srgb)));
            enc.set_vertex_buffer(mesh.vertices, 0);
            enc.set_index_buffer(mesh.indices, 0);
            enc.draw_indexed(mesh.index_count, b.count, 0, 0, 0);
        }
    }

    /// Releases every prop object (when the renderer is dropped with its device still alive).
    pub fn destroy_props(&mut self, device: &mut Device<'_>) {
        self.reset_prop_slots(device);
        for m in self.props.meshes.drain() {
            device.destroy_buffer(m.vertices);
            device.destroy_buffer(m.indices);
        }
        for m in self.props.materials.drain() {
            device.destroy_bind_group(m.group);
            if let Some(g) = m.shadow_group {
                device.destroy_bind_group(g);
            }
        }
        for t in self.props.textures.drain() {
            device.destroy_texture(t);
        }
        self.props.instances.drain();
        self.props.batches.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_from_json() {
        let d = MaterialDesc::from_json(
            br#"{"name": "felt", "color": [0.1, 0.3, 0.2, 1], "roughness": 2,
                 "shading": "unlit", "alphaCutoff": 0.5, "baseTexture": 7,
                 "castsShadow": false, "lamp": true, "emissive": [1, 0.5, 0]}"#,
        )
        .unwrap();
        assert_eq!(d.name, "felt");
        assert_eq!(d.base_color, [0.1, 0.3, 0.2, 1.0]);
        assert_eq!((d.roughness, d.shading), (1.0, Shading::Unlit));
        assert_eq!(
            (d.alpha_cutoff, d.base_texture),
            (Some(0.5), Some(PropTexture(7)))
        );
        assert!(!d.casts_shadow && d.lamp && d.emissive == [1.0, 0.5, 0.0]);
        assert_eq!(
            MaterialDesc::from_json(b"{}").unwrap(),
            MaterialDesc::default()
        );
        for bad in [
            &b"[]"[..],
            br#"{"colour": [1, 1, 1, 1]}"#,
            br#"{"color": [1, 1, 1]}"#,
            br#"{"shading": "toon"}"#,
            br#"{"baseTexture": 0}"#,
            br#"{"castsShadow": 1}"#,
        ] {
            assert!(
                MaterialDesc::from_json(bad).is_err(),
                "{}",
                String::from_utf8_lossy(bad)
            );
        }
    }
}
