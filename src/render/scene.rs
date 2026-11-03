//! A cooked world uploaded for the stylized renderer: one interleaved vertex buffer
//! (position, normal, uv) in world space, one index buffer, per-material textures, and lamp
//! lights.

use crate::gpu::{
    BufferDesc, BufferId, BufferUsage, Device, Format, Memory, Result, TextureDesc, TextureId,
    TextureUsage, f32_bytes, u32_bytes,
};
use crate::world::{AlphaMode, Emission, Shading, World};

use super::math::{V3, normalize};

/// One indexed draw of one material.
#[derive(Clone, Copy, Debug)]
pub(super) struct DrawItem {
    pub first_index: u32,
    pub count: u32,
    pub material: usize,
    pub water: bool,
}

/// Material constants in the `Draw` push-constant layout (after the model matrix).
#[derive(Clone, Copy, Debug)]
pub(super) struct MaterialData {
    pub base_color: [f32; 4],
    pub emissive: [f32; 4],
    pub params: [f32; 4],
    pub flags: [f32; 4],
    pub base: Option<usize>,
    pub normal: Option<usize>,
    pub emission: Option<usize>,
}

pub(super) struct GpuScene {
    pub vertices: BufferId,
    pub indices: BufferId,
    pub draws: Vec<DrawItem>,
    pub materials: Vec<MaterialData>,
    /// Material names (empty for the flat materials made for meshes without one), for Look
    /// rules that select materials by name.
    pub names: Vec<String>,
    pub textures: Vec<TextureId>,
    /// The world's lamps, as the shaders read them (`lights::LightRecord`).
    pub lights: Vec<[f32; 16]>,
    pub light_buffer: BufferId,
    pub water_level: Option<f32>,
    /// Bounds of everything drawn, and of everything but water (what casts shadows and what
    /// the world map covers; the same as all when there is no water).
    pub min: V3,
    pub max: V3,
    pub land_min: V3,
    pub land_max: V3,
    /// Mean position of the land's vertices: where its detail is densest.
    pub land_centre: V3,
    pub triangles: usize,
    /// What the CPU keeps so objects of the world can be moved and picked.
    pub edit: WorldEdit,
}

/// A world mesh as uploaded, kept for rewriting its instances' vertices and for picking.
pub(super) struct EditMesh {
    pub positions: Vec<V3>,
    pub normals: Vec<V3>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    pub bounds: (V3, V3),
}

pub(super) struct EditInstance {
    pub mesh: u32,
    /// Column-major model-to-world matrix as currently drawn.
    pub transform: [f32; 16],
    /// Where its vertices start in the vertex buffer (`None`: a mesh without triangles).
    pub first_vertex: Option<u32>,
    pub visible: bool,
}

pub(super) struct WorldEdit {
    pub meshes: Vec<EditMesh>,
    /// Per mesh: drawn as water.
    pub water_meshes: Vec<bool>,
    pub instances: Vec<EditInstance>,
    pub lights: Vec<crate::world::PointLight>,
}

const FLOATS_PER_VERTEX: usize = 8;

/// Appends `mesh`'s vertices placed by `m` (position, normal, uv), or all at one point when
/// hidden, which draws nothing.
fn write_vertices(mesh: &EditMesh, m: &[f32; 16], visible: bool, out: &mut Vec<f32>) {
    for (i, p) in mesh.positions.iter().enumerate() {
        let w = if visible {
            super::math::transform_point(m, *p)
        } else {
            [m[12], m[13], m[14]]
        };
        let n = mesh.normals.get(i).map_or([0.0, 1.0, 0.0], |n| {
            normalize([
                m[0] * n[0] + m[4] * n[1] + m[8] * n[2],
                m[1] * n[0] + m[5] * n[1] + m[9] * n[2],
                m[2] * n[0] + m[6] * n[1] + m[10] * n[2],
            ])
        });
        let uv = mesh.uvs.get(i).copied().unwrap_or([0.0, 0.0]);
        out.extend_from_slice(&[w[0], w[1], w[2], n[0], n[1], n[2], uv[0], uv[1]]);
    }
}

impl GpuScene {
    pub fn upload(device: &mut Device<'_>, world: &World) -> Result<Self> {
        world.validate()?;
        let mut vertices: Vec<f32> = vec![];
        let mut indices: Vec<u32> = vec![];
        let mut draws: Vec<DrawItem> = vec![];
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        let mut land_min = [f32::INFINITY; 3];
        let mut land_max = [f32::NEG_INFINITY; 3];
        let mut land_sum = [0.0f64; 3];
        let mut land_n = 0usize;
        let mut water_sum = 0.0f64;
        let mut water_n = 0usize;
        // A default material for meshes without one: their flat colour, matte.
        let mut materials: Vec<MaterialData> = world
            .materials
            .iter()
            .map(|m| {
                let shading = match m.shading {
                    Shading::Standard => 0.0,
                    Shading::Foliage => 1.0,
                    Shading::Water => 2.0,
                    Shading::Unlit => 3.0,
                };
                let cutoff = match m.alpha {
                    AlphaMode::Mask(c) => c,
                    _ => -1.0,
                };
                MaterialData {
                    base_color: m.base_color,
                    emissive: [
                        m.emissive[0],
                        m.emissive[1],
                        m.emissive[2],
                        if m.emission == Emission::Lamp {
                            1.0
                        } else {
                            0.0
                        },
                    ],
                    params: [cutoff, m.roughness, m.metallic, shading],
                    flags: [
                        f32::from(u8::from(m.base_color_texture.is_some())),
                        f32::from(u8::from(m.normal_texture.is_some())),
                        f32::from(u8::from(m.emissive_texture.is_some())),
                        m.normal_scale,
                    ],
                    base: m.base_color_texture.map(|t| t as usize),
                    normal: m.normal_texture.map(|t| t as usize),
                    emission: m.emissive_texture.map(|t| t as usize),
                }
            })
            .collect();
        let mut names: Vec<String> = world.materials.iter().map(|m| m.name.clone()).collect();
        let mut flat_materials = std::collections::HashMap::new();
        // Emit instances grouped by material (water last, blended after opaque, each in its
        // cooked order), so neighbouring instances of one material share one draw however many
        // separate objects a world keeps.
        let key = |i: usize| {
            let mesh = &world.meshes[world.instances[i].mesh as usize];
            let mat = mesh.material.map(|m| &world.materials[m as usize]);
            let water = mat.is_some_and(|m| m.shading == Shading::Water);
            let blend = mat.is_some_and(|m| m.alpha == AlphaMode::Blend);
            let group = match mesh.material {
                Some(m) => (0u8, m, [0u32; 3]),
                None => (1, 0, mesh.color.map(f32::to_bits)),
            };
            (
                water,
                blend,
                if blend { (2, i as u32, [0; 3]) } else { group },
            )
        };
        let mut order: Vec<usize> = (0..world.instances.len()).collect();
        order.sort_by_key(|&i| key(i));
        let mut edit = WorldEdit {
            meshes: world
                .meshes
                .iter()
                .map(|m| EditMesh {
                    positions: m.positions.iter().map(|p| [p.0, p.1, p.2]).collect(),
                    normals: m.normals.iter().map(|n| [n.0, n.1, n.2]).collect(),
                    uvs: m.uvs.clone(),
                    indices: m.indices.clone(),
                    bounds: super::props::bounds(
                        &m.positions
                            .iter()
                            .map(|p| [p.0, p.1, p.2])
                            .collect::<Vec<_>>(),
                    ),
                })
                .collect(),
            instances: world
                .instances
                .iter()
                .map(|i| EditInstance {
                    mesh: i.mesh,
                    transform: i.transform.0,
                    first_vertex: None,
                    visible: true,
                })
                .collect(),
            lights: world.lights.clone(),
            water_meshes: world
                .meshes
                .iter()
                .map(|m| {
                    m.material
                        .is_some_and(|k| world.materials[k as usize].shading == Shading::Water)
                })
                .collect(),
        };
        for &index in &order {
            let inst = &world.instances[index];
            let mesh = &world.meshes[inst.mesh as usize];
            if mesh.indices.is_empty() {
                continue;
            }
            let m = inst.transform.0;
            let base = u32::try_from(vertices.len() / FLOATS_PER_VERTEX)
                .map_err(|_| "too many vertices")?;
            edit.instances[index].first_vertex = Some(base);
            let water = mesh
                .material
                .is_some_and(|m| world.materials[m as usize].shading == Shading::Water);
            write_vertices(&edit.meshes[inst.mesh as usize], &m, true, &mut vertices);
            for p in &mesh.positions {
                let w = super::math::transform_point(&m, [p.0, p.1, p.2]);
                for k in 0..3 {
                    min[k] = min[k].min(w[k]);
                    max[k] = max[k].max(w[k]);
                    if !water {
                        land_min[k] = land_min[k].min(w[k]);
                        land_max[k] = land_max[k].max(w[k]);
                        land_sum[k] += f64::from(w[k]);
                    }
                }
                if !water {
                    land_n += 1;
                }
            }
            let first_index = indices.len() as u32;
            indices.extend(mesh.indices.iter().map(|i| i + base));
            let material = match mesh.material {
                Some(m) => m as usize,
                None => {
                    let key = mesh.color.map(f32::to_bits);
                    *flat_materials.entry(key).or_insert_with(|| {
                        materials.push(MaterialData {
                            base_color: [mesh.color[0], mesh.color[1], mesh.color[2], 1.0],
                            emissive: [0.0; 4],
                            params: [-1.0, 0.8, 0.0, 0.0],
                            flags: [0.0, 0.0, 0.0, 1.0],
                            base: None,
                            normal: None,
                            emission: None,
                        });
                        names.push(String::new());
                        materials.len() - 1
                    })
                }
            };
            if water {
                for p in &mesh.positions {
                    water_sum += f64::from(m[1] * p.0 + m[5] * p.1 + m[9] * p.2 + m[13]);
                    water_n += 1;
                }
            }
            let count = mesh.indices.len() as u32;
            match draws.last_mut() {
                Some(d)
                    if d.material == material
                        && d.water == water
                        && d.first_index + d.count == first_index =>
                {
                    d.count += count;
                }
                _ => draws.push(DrawItem {
                    first_index,
                    count,
                    material,
                    water,
                }),
            }
        }
        let triangles = indices.len() / 3;
        if draws.is_empty() {
            // Nothing cooked: a scene of props only. A placeholder keeps the buffers non-empty
            // and the bounds finite.
            (min, max) = ([-1.0; 3], [1.0; 3]);
            vertices = vec![0.0; FLOATS_PER_VERTEX];
            indices = vec![0; 3];
        }
        if land_min[0] > land_max[0] {
            (land_min, land_max) = (min, max);
        }
        let land_centre = if land_n > 0 {
            land_sum.map(|v| (v / land_n as f64) as f32)
        } else {
            [
                0.5 * (min[0] + max[0]),
                0.5 * (min[1] + max[1]),
                0.5 * (min[2] + max[2]),
            ]
        };
        // Water last, so the opaque world is already in the depth buffer.
        draws.sort_by_key(|d| d.water);
        let vb = device.create_buffer(&BufferDesc {
            size: (vertices.len() * 4) as u64,
            usage: BufferUsage::VERTEX | BufferUsage::COPY_DST,
            memory: Memory::Device,
        })?;
        device.write_buffer(vb, 0, &f32_bytes(&vertices))?;
        let ib = device.create_buffer(&BufferDesc {
            size: (indices.len() * 4) as u64,
            usage: BufferUsage::INDEX | BufferUsage::COPY_DST,
            memory: Memory::Device,
        })?;
        device.write_buffer(ib, 0, &u32_bytes(&indices))?;

        let mut textures = Vec::with_capacity(world.textures.len());
        for (i, t) in world.textures.iter().enumerate() {
            // Encoded PNGs are decoded here; other formats are the host's to decode first.
            let decoded;
            let levels: &[Vec<u8>] = match &t.encoded {
                Some(e) if e.mime == "image/png" => {
                    let d = crate::assets::png::decode(&e.data)
                        .map_err(|err| format!("texture {i}: {err}"))?;
                    if (d.width, d.height) != (t.width, t.height) {
                        return Err(format!(
                            "texture {i}: the PNG is not {}x{}",
                            t.width, t.height
                        ));
                    }
                    decoded = [d.rgba];
                    &decoded
                }
                Some(e) => {
                    return Err(format!(
                        "texture {i} is still encoded as {}: decode it before creating the \
                         renderer (createEngine does this in the browser)",
                        e.mime
                    ));
                }
                None => &t.levels,
            };
            // A top level alone gets the rest of its chain made on the GPU.
            let full = crate::world::Texture::level_count(t.width, t.height) as u32;
            let generate = (levels.len() as u32) < full;
            let mut usage = TextureUsage::SAMPLED | TextureUsage::COPY_DST;
            if generate {
                usage = usage | TextureUsage::COPY_SRC | TextureUsage::COLOR_TARGET;
            }
            let id = device.create_texture(&TextureDesc {
                width: t.width,
                height: t.height,
                layers: 1,
                mips: full,
                format: if t.srgb {
                    Format::Rgba8Srgb
                } else {
                    Format::Rgba8Unorm
                },
                usage,
            })?;
            for (level, data) in levels.iter().enumerate() {
                device.write_texture(id, level as u32, 0, data)?;
            }
            if generate {
                device.generate_mips(id)?;
            }
            textures.push(id);
        }

        let lights: Vec<[f32; 16]> = world
            .lights
            .iter()
            .map(|l| {
                // glTF: range 0 means unlimited; a very large range lights every tile.
                let range = if l.range > 0.0 { l.range } else { 1.0e4 };
                super::lights::lamp_record(
                    [l.position.0, l.position.1, l.position.2],
                    range,
                    l.color,
                    l.intensity,
                )
            })
            .collect();
        let light_data: Vec<f32> = lights.iter().flatten().copied().collect();
        // The lamps, then room for a game's own lights (`lights.rs`).
        let light_buffer = device.create_buffer(&BufferDesc {
            size: ((lights.len() + super::lights::MAX_GAME_LIGHTS) * 64) as u64,
            usage: BufferUsage::STORAGE | BufferUsage::COPY_DST,
            memory: Memory::Device,
        })?;
        if !light_data.is_empty() {
            device.write_buffer(light_buffer, 0, &f32_bytes(&light_data))?;
        }

        Ok(Self {
            vertices: vb,
            indices: ib,
            draws,
            materials,
            names,
            textures,
            lights,
            light_buffer,
            water_level: (water_n > 0).then(|| (water_sum / water_n as f64) as f32),
            min,
            max,
            land_min,
            land_max,
            land_centre,
            triangles,
            edit,
        })
    }

    /// Moves world instance `index` to `transform` (column-major), or hides or shows it,
    /// rewriting only its vertices. Bounds and the water level follow on `refresh_bounds`.
    pub fn set_instance(
        &mut self,
        device: &mut Device<'_>,
        index: usize,
        transform: [f32; 16],
        visible: bool,
    ) -> Result<()> {
        if !transform.iter().all(|v| v.is_finite())
            || [transform[3], transform[7], transform[11], transform[15]] != [0.0, 0.0, 0.0, 1.0]
        {
            return Err("set_world_instance: an affine transform with finite values".into());
        }
        let inst = self
            .edit
            .instances
            .get_mut(index)
            .ok_or_else(|| format!("set_world_instance: no world instance {index}"))?;
        inst.transform = transform;
        inst.visible = visible;
        let Some(first) = inst.first_vertex else {
            return Ok(());
        };
        let mesh = &self.edit.meshes[inst.mesh as usize];
        let mut data = Vec::with_capacity(mesh.positions.len() * FLOATS_PER_VERTEX);
        write_vertices(mesh, &transform, visible, &mut data);
        device.write_buffer(
            self.vertices,
            u64::from(first) * (FLOATS_PER_VERTEX as u64 * 4),
            &f32_bytes(&data),
        )
    }

    /// Moves world lamp `index` to `position`.
    pub fn set_light(&mut self, device: &mut Device<'_>, index: usize, position: V3) -> Result<()> {
        if !position.iter().all(|v| v.is_finite()) {
            return Err("set_world_light: a finite position".into());
        }
        let l = self
            .edit
            .lights
            .get_mut(index)
            .ok_or_else(|| format!("set_world_light: no world lamp {index}"))?;
        l.position = crate::Vec3(position[0], position[1], position[2]);
        let range = if l.range > 0.0 { l.range } else { 1.0e4 };
        let record = super::lights::lamp_record(position, range, l.color, l.intensity);
        self.lights[index] = record;
        device.write_buffer(self.light_buffer, index as u64 * 64, &f32_bytes(&record))
    }

    /// Recomputes the bounds, the land's centre and the water level from the instances as
    /// they are now placed.
    pub fn refresh_bounds(&mut self) {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        let mut land_min = [f32::INFINITY; 3];
        let mut land_max = [f32::NEG_INFINITY; 3];
        let (mut land_sum, mut land_n) = ([0.0f64; 3], 0usize);
        let (mut water_sum, mut water_n) = (0.0f64, 0usize);
        for inst in &self.edit.instances {
            if inst.first_vertex.is_none() || !inst.visible {
                continue;
            }
            let mesh = &self.edit.meshes[inst.mesh as usize];
            let water = self.instance_is_water(inst);
            for p in &mesh.positions {
                let w = super::math::transform_point(&inst.transform, *p);
                for k in 0..3 {
                    min[k] = min[k].min(w[k]);
                    max[k] = max[k].max(w[k]);
                }
                if water {
                    water_sum += f64::from(w[1]);
                    water_n += 1;
                } else {
                    for k in 0..3 {
                        land_min[k] = land_min[k].min(w[k]);
                        land_max[k] = land_max[k].max(w[k]);
                        land_sum[k] += f64::from(w[k]);
                    }
                    land_n += 1;
                }
            }
        }
        if min[0] > max[0] {
            (min, max) = ([-1.0; 3], [1.0; 3]);
        }
        if land_min[0] > land_max[0] {
            (land_min, land_max) = (min, max);
        }
        self.min = min;
        self.max = max;
        self.land_min = land_min;
        self.land_max = land_max;
        self.land_centre = if land_n > 0 {
            land_sum.map(|v| (v / land_n as f64) as f32)
        } else {
            [
                0.5 * (min[0] + max[0]),
                0.5 * (min[1] + max[1]),
                0.5 * (min[2] + max[2]),
            ]
        };
        if water_n > 0 {
            self.water_level = Some((water_sum / water_n as f64) as f32);
        }
    }

    fn instance_is_water(&self, inst: &EditInstance) -> bool {
        self.edit
            .water_meshes
            .get(inst.mesh as usize)
            .copied()
            .unwrap_or(false)
    }

    /// The nearest world instance a ray from `origin` along unit `dir` meets: its index, the
    /// distance in metres and the point.
    pub fn pick(&self, origin: V3, dir: V3) -> Option<(usize, f32, V3)> {
        let mut best: Option<(usize, f32)> = None;
        for (i, inst) in self.edit.instances.iter().enumerate() {
            if inst.first_vertex.is_none() || !inst.visible {
                continue;
            }
            let Some(inv) = super::math::invert(&inst.transform) else {
                continue;
            };
            let o = super::math::transform_point(&inv, origin);
            let tip = super::math::transform_point(&inv, super::math::add(origin, dir));
            let d = super::math::sub(tip, o);
            let mesh = &self.edit.meshes[inst.mesh as usize];
            let Some(enter) = super::props::ray_box(o, d, mesh.bounds.0, mesh.bounds.1) else {
                continue;
            };
            if best.is_some_and(|(_, b)| enter > b) {
                continue;
            }
            for t in mesh.indices.chunks_exact(3) {
                let (a, b, c) = (
                    mesh.positions[t[0] as usize],
                    mesh.positions[t[1] as usize],
                    mesh.positions[t[2] as usize],
                );
                if let Some(s) = super::props::ray_triangle(o, d, a, b, c)
                    && best.is_none_or(|(_, bd)| s < bd)
                {
                    best = Some((i, s));
                }
            }
        }
        best.map(|(i, s)| (i, s, super::math::add(origin, super::math::scale(dir, s))))
    }
}
