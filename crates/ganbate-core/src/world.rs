//! Mesh geometry is stored once, with affine instances for world placement. Materials,
//! mip-mapped RGBA8 textures and point lights describe surfaces and lamps for shaded renderers.
use crate::{Rgb, Scene, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform(pub [f32; 16]); // column-major, right-handed, Y-up
impl Transform {
    pub const IDENTITY: Self = Self([
        1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
    ]);
    pub fn from_trs(t: [f32; 3], q: [f32; 4], s: [f32; 3]) -> Result<Self, String> {
        if !t.into_iter().chain(q).chain(s).all(f32::is_finite) {
            return Err("non-finite transform".into());
        }
        let len = q.iter().map(|v| v * v).sum::<f32>().sqrt();
        if !len.is_finite() || len < 1e-6 {
            return Err("invalid rotation quaternion".into());
        }
        let [x, y, z, w] = q.map(|a| a / len);
        Ok(Self([
            (1. - 2. * (y * y + z * z)) * s[0],
            2. * (x * y + z * w) * s[0],
            2. * (x * z - y * w) * s[0],
            0.,
            2. * (x * y - z * w) * s[1],
            (1. - 2. * (x * x + z * z)) * s[1],
            2. * (y * z + x * w) * s[1],
            0.,
            2. * (x * z + y * w) * s[2],
            2. * (y * z - x * w) * s[2],
            (1. - 2. * (x * x + y * y)) * s[2],
            0.,
            t[0],
            t[1],
            t[2],
            1.,
        ]))
    }
    pub fn compose(self, local: Self) -> Self {
        let mut out = [0.0; 16];
        for (i, v) in out.iter_mut().enumerate() {
            let row = i % 4;
            let col = (i / 4) * 4;
            *v = (0..4).map(|k| self.0[row + 4 * k] * local.0[col + k]).sum();
        }
        Self(out)
    }
    pub fn point(self, Vec3(x, y, z): Vec3) -> Vec3 {
        let m = self.0;
        Vec3(
            m[0] * x + m[4] * y + m[8] * z + m[12],
            m[1] * x + m[5] * y + m[9] * z + m[13],
            m[2] * x + m[6] * y + m[10] * z + m[14],
        )
    }
    pub fn valid(self) -> bool {
        self.0.iter().all(|v| v.is_finite())
            && [self.0[3], self.0[7], self.0[11], self.0[15]] == [0., 0., 0., 1.]
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub positions: Vec<Vec3>,
    /// Empty, or exactly one per position.
    pub normals: Vec<Vec3>,
    /// Empty, or exactly one per position.
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    /// Flat colour for untextured paths. Equals the material's base colour RGB when one is set.
    pub color: Rgb,
    /// Index into `World::materials`.
    pub material: Option<u32>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Instance {
    pub mesh: u32,
    pub transform: Transform,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum AlphaMode {
    #[default]
    Opaque,
    /// Fragments with alpha below the cutoff are discarded.
    Mask(f32),
    Blend,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Shading {
    #[default]
    Standard,
    Foliage,
    Water,
    Unlit,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Emission {
    #[default]
    Constant,
    /// The renderer scales the emissive term with its lamp level (time of day).
    Lamp,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub name: String,
    /// Linear RGBA factor.
    pub base_color: [f32; 4],
    /// Index into `World::textures` (an sRGB texture).
    pub base_color_texture: Option<u32>,
    /// Index into `World::textures` (a linear texture).
    pub normal_texture: Option<u32>,
    pub normal_scale: f32,
    /// Linear: emissive factor times emissive strength.
    pub emissive: Rgb,
    /// Index into `World::textures` (an sRGB texture).
    pub emissive_texture: Option<u32>,
    pub metallic: f32,
    pub roughness: f32,
    pub alpha: AlphaMode,
    pub double_sided: bool,
    pub shading: Shading,
    pub emission: Emission,
}
/// The glTF 2.0 default material.
impl Default for Material {
    fn default() -> Self {
        Self {
            name: String::new(),
            base_color: [1.; 4],
            base_color_texture: None,
            normal_texture: None,
            normal_scale: 1.,
            emissive: [0.; 3],
            emissive_texture: None,
            metallic: 1.,
            roughness: 1.,
            alpha: AlphaMode::Opaque,
            double_sided: false,
            shading: Shading::Standard,
            emission: Emission::Constant,
        }
    }
}
/// An image kept in its file format for the host to decode: browsers decode PNG, JPEG and WebP
/// natively, and the engine decodes PNG itself.
#[derive(Clone, Debug, PartialEq)]
pub struct EncodedImage {
    /// `image/png`, `image/jpeg` or `image/webp`.
    pub mime: String,
    pub data: Vec<u8>,
}
pub const IMAGE_TYPES: [&str; 3] = ["image/png", "image/jpeg", "image/webp"];

/// RGBA8 texels, level 0 first. Level `i` is `max(1, width >> i)` by `max(1, height >> i)`;
/// `levels` holds the complete chain down to 1x1, or only level 0 (the renderer makes the
/// rest), or nothing while the image is still `encoded`.
#[derive(Clone, Debug, PartialEq)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub srgb: bool,
    pub levels: Vec<Vec<u8>>,
    pub encoded: Option<EncodedImage>,
}
impl Texture {
    /// Levels in a complete chain for a nonempty texture.
    pub fn level_count(width: u32, height: u32) -> usize {
        (u32::BITS - width.max(height).max(1).leading_zeros()) as usize
    }
    pub fn level_extent(width: u32, height: u32, level: usize) -> (u32, u32) {
        let shift = |v: u32| v.checked_shr(level as u32).unwrap_or(0).max(1);
        (shift(width), shift(height))
    }
    fn check(&self) -> Result<(), String> {
        let limit = 1..=MAX_TEXTURE_SIZE;
        if !limit.contains(&self.width) || !limit.contains(&self.height) {
            return Err(format!(
                "size {}x{} outside limits",
                self.width, self.height
            ));
        }
        if let Some(e) = &self.encoded {
            if !self.levels.is_empty() {
                return Err("an encoded image also has texels".into());
            }
            if !IMAGE_TYPES.contains(&e.mime.as_str()) || e.data.is_empty() {
                return Err(format!("encoded image of type {:?}", e.mime));
            }
            return Ok(());
        }
        let full = Self::level_count(self.width, self.height);
        if self.levels.len() != full && self.levels.len() != 1 {
            return Err("incomplete mip chain".into());
        }
        for (i, level) in self.levels.iter().enumerate() {
            let (w, h) = Self::level_extent(self.width, self.height, i);
            if level.len() as u64 != u64::from(w) * u64::from(h) * 4 {
                return Err(format!("level {i} is not {w}x{h} RGBA8"));
            }
        }
        Ok(())
    }
}
/// World-space point light. A range of 0 is unlimited.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointLight {
    pub position: Vec3,
    pub color: Rgb,
    pub intensity: f32,
    pub range: f32,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct World {
    pub meshes: Vec<Mesh>,
    pub instances: Vec<Instance>,
    pub materials: Vec<Material>,
    pub textures: Vec<Texture>,
    pub lights: Vec<PointLight>,
}

// Limit expansion from many instances of one mesh before allocating output.
pub const MAX_TRIANGLES: usize = 2_000_000;
pub const MAX_MATERIALS: usize = 16_384;
pub const MAX_TEXTURES: usize = 16_384;
pub const MAX_LIGHTS: usize = 16_384;
/// Largest texture width or height.
pub const MAX_TEXTURE_SIZE: u32 = 16_384;
pub const MAX_NAME_BYTES: usize = 4_096;

fn finite(values: &[f32]) -> bool {
    values.iter().all(|v| v.is_finite())
}
fn nonnegative(values: &[f32]) -> bool {
    values.iter().all(|v| v.is_finite() && *v >= 0.)
}
impl Material {
    fn check(&self, textures: usize) -> Result<(), String> {
        if self.name.len() > MAX_NAME_BYTES {
            return Err("name too long".into());
        }
        let cutoff = match self.alpha {
            AlphaMode::Mask(c) => c,
            _ => 0.,
        };
        if !nonnegative(&self.base_color)
            || !nonnegative(&self.emissive)
            || !nonnegative(&[cutoff])
            || !finite(&[self.normal_scale])
            || ![self.metallic, self.roughness]
                .iter()
                .all(|v| (0.0..=1.0).contains(v))
        {
            return Err("factor out of range".into());
        }
        let maps = [
            self.base_color_texture,
            self.normal_texture,
            self.emissive_texture,
        ];
        if maps.into_iter().flatten().any(|t| t as usize >= textures) {
            return Err("references missing texture".into());
        }
        Ok(())
    }
}
impl World {
    pub fn validate(&self) -> Result<usize, String> {
        if self.materials.len() > MAX_MATERIALS
            || self.textures.len() > MAX_TEXTURES
            || self.lights.len() > MAX_LIGHTS
        {
            return Err("too many materials, textures or lights".into());
        }
        for (i, t) in self.textures.iter().enumerate() {
            t.check().map_err(|e| format!("texture {i}: {e}"))?;
        }
        for (i, m) in self.materials.iter().enumerate() {
            m.check(self.textures.len())
                .map_err(|e| format!("material {i}: {e}"))?;
        }
        for (i, m) in self.meshes.iter().enumerate() {
            if m.indices.len() % 3 != 0
                || m.indices.iter().any(|j| *j as usize >= m.positions.len())
            {
                return Err(format!("invalid triangle indices in mesh {i}"));
            }
            let n = m.positions.len();
            if ![0, n].contains(&m.normals.len()) || ![0, n].contains(&m.uvs.len()) {
                return Err(format!("mesh {i} needs one normal and UV per position"));
            }
            if m.positions
                .iter()
                .chain(&m.normals)
                .any(|p| !finite(&[p.0, p.1, p.2]))
                || !m.uvs.iter().all(|uv| finite(uv))
                || !finite(&m.color)
            {
                return Err(format!("non-finite data in mesh {i}"));
            }
            if m.material
                .is_some_and(|k| k as usize >= self.materials.len())
            {
                return Err(format!("mesh {i} references missing material"));
            }
        }
        for (i, l) in self.lights.iter().enumerate() {
            let Vec3(x, y, z) = l.position;
            if !finite(&[x, y, z])
                || !nonnegative(&l.color)
                || !nonnegative(&[l.intensity, l.range])
            {
                return Err(format!("invalid point light {i}"));
            }
        }
        let mut count = 0usize;
        for inst in &self.instances {
            let m = self
                .meshes
                .get(inst.mesh as usize)
                .ok_or("instance references missing mesh")?;
            if !inst.transform.valid() {
                return Err("invalid instance transform".into());
            }
            count = count
                .checked_add(m.indices.len() / 3)
                .filter(|c| *c <= MAX_TRIANGLES)
                .ok_or("scene triangle limit exceeded")?;
        }
        Ok(count)
    }
    pub fn scene(&self) -> Result<Scene, String> {
        let count = self.validate()?;
        let mut out = Scene {
            triangles: Vec::with_capacity(count),
        };
        for inst in &self.instances {
            let m = &self.meshes[inst.mesh as usize];
            for f in m.indices.chunks_exact(3) {
                let [a, b, c] =
                    [f[0], f[1], f[2]].map(|i| inst.transform.point(m.positions[i as usize]));
                if ![a, b, c]
                    .iter()
                    .all(|p| [p.0, p.1, p.2].into_iter().all(f32::is_finite))
                {
                    return Err("transformed geometry exceeds numeric range".into());
                }
                out.triangle(a, b, c, m.color);
            }
        }
        Ok(out)
    }
}

impl Transform {
    /// The inverse of an affine transform, or `None` when it is singular.
    pub fn inverse(self) -> Option<Self> {
        let m = self.0;
        // The 3x3 part, row r column c at m[c * 4 + r].
        let a = |r: usize, c: usize| m[c * 4 + r];
        let det = a(0, 0) * (a(1, 1) * a(2, 2) - a(1, 2) * a(2, 1))
            - a(0, 1) * (a(1, 0) * a(2, 2) - a(1, 2) * a(2, 0))
            + a(0, 2) * (a(1, 0) * a(2, 1) - a(1, 1) * a(2, 0));
        if !det.is_finite() || det.abs() < 1e-20 {
            return None;
        }
        let inv_det = 1.0 / det;
        let mut r = [[0.0f32; 3]; 3];
        r[0][0] = (a(1, 1) * a(2, 2) - a(1, 2) * a(2, 1)) * inv_det;
        r[0][1] = (a(0, 2) * a(2, 1) - a(0, 1) * a(2, 2)) * inv_det;
        r[0][2] = (a(0, 1) * a(1, 2) - a(0, 2) * a(1, 1)) * inv_det;
        r[1][0] = (a(1, 2) * a(2, 0) - a(1, 0) * a(2, 2)) * inv_det;
        r[1][1] = (a(0, 0) * a(2, 2) - a(0, 2) * a(2, 0)) * inv_det;
        r[1][2] = (a(0, 2) * a(1, 0) - a(0, 0) * a(1, 2)) * inv_det;
        r[2][0] = (a(1, 0) * a(2, 1) - a(1, 1) * a(2, 0)) * inv_det;
        r[2][1] = (a(0, 1) * a(2, 0) - a(0, 0) * a(2, 1)) * inv_det;
        r[2][2] = (a(0, 0) * a(1, 1) - a(0, 1) * a(1, 0)) * inv_det;
        let t = [m[12], m[13], m[14]];
        let mut out = [0.0f32; 16];
        for row in 0..3 {
            for col in 0..3 {
                out[col * 4 + row] = r[row][col];
            }
            out[12 + row] = -(r[row][0] * t[0] + r[row][1] * t[1] + r[row][2] * t[2]);
        }
        out[15] = 1.0;
        let inv = Self(out);
        inv.valid().then_some(inv)
    }
}

/// One authored entity of a cooked scene: its identity, and the run of the world's instances
/// and lights it produced, so tools and games can find, move and hide objects of a world.
#[derive(Clone, Debug, PartialEq)]
pub struct EntityRecord {
    /// The entity's 64-bit id (its 16-digit hexadecimal id in the scene file).
    pub id: u64,
    pub name: String,
    /// Index of the parent record; parents come before their children.
    pub parent: Option<u32>,
    /// The entity's world transform when cooked. Moving the entity to `m` moves each of its
    /// instances `i` to `m * transform⁻¹ * i`.
    pub transform: Transform,
    /// The entity's own instances and lights (not its children's): runs of `World::instances`
    /// and `World::lights`.
    pub instances: std::ops::Range<u32>,
    pub lights: std::ops::Range<u32>,
}

/// Where a cooked world came from: one record per entity, plus the data the renderer does not
/// draw (game components, cameras, markers, the scene's settings) as canonical JSON.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EntityTable {
    pub entities: Vec<EntityRecord>,
    pub data: String,
}

pub const MAX_ENTITIES: usize = 250_000;
/// Largest `EntityTable::data`.
pub const MAX_ENTITY_DATA: usize = 16 * 1024 * 1024;

impl EntityTable {
    /// Checks the table against the world it describes.
    pub fn validate(&self, world: &World) -> Result<(), String> {
        if self.entities.len() > MAX_ENTITIES || self.data.len() > MAX_ENTITY_DATA {
            return Err("entity table too large".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for (i, e) in self.entities.iter().enumerate() {
            if !ids.insert(e.id) {
                return Err(format!("entity {:016x} appears twice", e.id));
            }
            if e.name.len() > MAX_NAME_BYTES {
                return Err(format!("entity {i}: name too long"));
            }
            if e.parent.is_some_and(|p| p as usize >= i) {
                return Err(format!("entity {i}: its parent does not come before it"));
            }
            if !e.transform.valid() {
                return Err(format!("entity {i}: invalid transform"));
            }
            if e.instances.start > e.instances.end
                || e.instances.end as usize > world.instances.len()
                || e.lights.start > e.lights.end
                || e.lights.end as usize > world.lights.len()
            {
                return Err(format!("entity {i}: instances or lights out of range"));
            }
        }
        Ok(())
    }

    /// The record of entity `id`.
    pub fn find(&self, id: u64) -> Option<(usize, &EntityRecord)> {
        self.entities.iter().enumerate().find(|(_, e)| e.id == id)
    }
}
