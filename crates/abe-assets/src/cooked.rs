//! ABEW cooked worlds. Little-endian, deterministic bytes; every count is bounded and checked
//! against the remaining input before allocation, and trailing bytes are rejected.
//!
//! Version 2 (written): header `ABEW`, version, then counts of meshes, instances, materials,
//! textures and lights, followed by those sections in that order.
//! - mesh: position count, index count, flags (1 normals, 2 UVs), colour (3 f32), material
//!   (`u32::MAX` for none), positions (3 f32 each), normals, UVs (2 f32 each), indices (u32).
//! - instance: mesh, column-major affine transform (16 f32).
//! - material: name byte length, UTF-8 name zero-padded to 4 bytes, base colour (4 f32),
//!   base colour texture, normal texture, normal scale, emissive (3 f32), emissive texture,
//!   metallic, roughness, alpha mode (0 opaque, 1 mask, 2 blend), alpha cutoff (0 unless mask),
//!   double-sided (0/1), shading (0 standard, 1 foliage, 2 water, 3 unlit), emission
//!   (0 constant, 1 lamp). Absent textures are `u32::MAX`.
//! - texture: width, height, sRGB (0/1), level count (the complete chain), then each level's
//!   RGBA8 texels; level sizes follow from the dimensions.
//! - light: position (3 f32), colour (3 f32), intensity, range.
//!
//! Version 1 (read only): mesh and instance counts, then meshes as position count, index
//! count, colour, positions and indices, then instances as above.
use abe_core::{
    Vec3,
    world::{
        AlphaMode, Emission, Instance, MAX_LIGHTS, MAX_MATERIALS, MAX_NAME_BYTES, MAX_TEXTURE_SIZE,
        MAX_TEXTURES, Material, Mesh, PointLight, Shading, Texture, Transform, World,
    },
};
const MAGIC: &[u8; 4] = b"ABEW";
pub const VERSION: u32 = 2;
pub const MAX_BYTES: usize = 256 * 1024 * 1024;
const MAX_MESHES: usize = 65_536;
const MAX_INSTANCES: usize = 250_000;
const NONE: u32 = u32::MAX;
// Smallest encoded records, for validating counts before allocation.
const MESH_BYTES: u64 = 28;
const INSTANCE_BYTES: u64 = 68;
const MATERIAL_BYTES: u64 = 76;
const TEXTURE_BYTES: u64 = 20;
const LIGHT_BYTES: u64 = 32;

fn put_u32(v: &mut Vec<u8>, n: usize) -> Result<(), String> {
    v.extend_from_slice(
        &u32::try_from(n)
            .map_err(|_| "count exceeds u32")?
            .to_le_bytes(),
    );
    Ok(())
}
fn put_floats(v: &mut Vec<u8>, values: &[f32]) {
    for n in values {
        v.extend_from_slice(&n.to_le_bytes())
    }
}
fn put_index(v: &mut Vec<u8>, i: Option<u32>) {
    v.extend_from_slice(&i.unwrap_or(NONE).to_le_bytes());
}
fn padded(len: usize) -> u64 {
    len.next_multiple_of(4) as u64
}

/// Version of an ABEW file, if it has the ABEW magic.
pub fn version(data: &[u8]) -> Option<u32> {
    let (magic, rest) = data.split_first_chunk::<4>()?;
    let (v, _) = rest.split_first_chunk::<4>()?;
    (magic == MAGIC).then(|| u32::from_le_bytes(*v))
}

pub fn encode(w: &World) -> Result<Vec<u8>, String> {
    w.validate()?;
    if w.meshes.len() > MAX_MESHES || w.instances.len() > MAX_INSTANCES {
        return Err("too many meshes or instances".into());
    }
    let mut size =
        28 + INSTANCE_BYTES * w.instances.len() as u64 + LIGHT_BYTES * w.lights.len() as u64;
    for m in &w.meshes {
        let n = m.positions.len() as u64;
        size += MESH_BYTES + 12 * n + 4 * m.indices.len() as u64;
        size += 12 * m.normals.len() as u64 + 8 * m.uvs.len() as u64;
    }
    for m in &w.materials {
        size += MATERIAL_BYTES + padded(m.name.len());
    }
    for t in &w.textures {
        size += 16 + t.levels.iter().map(|l| l.len() as u64).sum::<u64>();
    }
    if size > MAX_BYTES as u64 {
        return Err("cooked file too large".into());
    }
    let mut v = Vec::with_capacity(size as usize);
    v.extend_from_slice(MAGIC);
    put_u32(&mut v, VERSION as usize)?;
    for n in [
        w.meshes.len(),
        w.instances.len(),
        w.materials.len(),
        w.textures.len(),
        w.lights.len(),
    ] {
        put_u32(&mut v, n)?;
    }
    for m in &w.meshes {
        put_u32(&mut v, m.positions.len())?;
        put_u32(&mut v, m.indices.len())?;
        let flags = usize::from(!m.normals.is_empty()) | usize::from(!m.uvs.is_empty()) << 1;
        put_u32(&mut v, flags)?;
        put_floats(&mut v, &m.color);
        put_index(&mut v, m.material);
        for &Vec3(x, y, z) in m.positions.iter().chain(&m.normals) {
            put_floats(&mut v, &[x, y, z]);
        }
        for uv in &m.uvs {
            put_floats(&mut v, uv);
        }
        for i in &m.indices {
            v.extend_from_slice(&i.to_le_bytes());
        }
    }
    for i in &w.instances {
        v.extend_from_slice(&i.mesh.to_le_bytes());
        put_floats(&mut v, &i.transform.0);
    }
    for m in &w.materials {
        put_u32(&mut v, m.name.len())?;
        v.extend_from_slice(m.name.as_bytes());
        v.resize(v.len() + (padded(m.name.len()) as usize - m.name.len()), 0);
        put_floats(&mut v, &m.base_color);
        put_index(&mut v, m.base_color_texture);
        put_index(&mut v, m.normal_texture);
        put_floats(&mut v, &[m.normal_scale]);
        put_floats(&mut v, &m.emissive);
        put_index(&mut v, m.emissive_texture);
        put_floats(&mut v, &[m.metallic, m.roughness]);
        let (mode, cutoff) = match m.alpha {
            AlphaMode::Opaque => (0, 0.),
            AlphaMode::Mask(c) => (1, c),
            AlphaMode::Blend => (2, 0.),
        };
        put_u32(&mut v, mode)?;
        put_floats(&mut v, &[cutoff]);
        put_u32(&mut v, usize::from(m.double_sided))?;
        put_u32(&mut v, m.shading as usize)?;
        put_u32(&mut v, m.emission as usize)?;
    }
    for t in &w.textures {
        put_u32(&mut v, t.width as usize)?;
        put_u32(&mut v, t.height as usize)?;
        put_u32(&mut v, usize::from(t.srgb))?;
        put_u32(&mut v, t.levels.len())?;
        for level in &t.levels {
            v.extend_from_slice(level);
        }
    }
    for l in &w.lights {
        let Vec3(x, y, z) = l.position;
        put_floats(&mut v, &[x, y, z]);
        put_floats(&mut v, &l.color);
        put_floats(&mut v, &[l.intensity, l.range]);
    }
    debug_assert_eq!(v.len() as u64, size);
    Ok(v)
}

struct Read<'a>(&'a [u8]);
impl<'a> Read<'a> {
    fn bytes(&mut self, n: u64) -> Result<&'a [u8], String> {
        let n = usize::try_from(n).map_err(|_| "truncated cooked scene")?;
        let (b, left) = self.0.split_at_checked(n).ok_or("truncated cooked scene")?;
        self.0 = left;
        Ok(b)
    }
    fn word(&mut self) -> Result<[u8; 4], String> {
        Ok(self.bytes(4)?.try_into().expect("four bytes"))
    }
    fn uint(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.word()?))
    }
    fn floats<const N: usize>(&mut self) -> Result<[f32; N], String> {
        let mut v = [0.0; N];
        for f in &mut v {
            *f = f32::from_le_bytes(self.word()?)
        }
        Ok(v)
    }
    fn vec3s(&mut self, n: usize) -> Result<Vec<Vec3>, String> {
        (0..n)
            .map(|_| self.floats().map(|[x, y, z]| Vec3(x, y, z)))
            .collect()
    }
    fn index(&mut self) -> Result<Option<u32>, String> {
        Ok(Some(self.uint()?).filter(|&i| i != NONE))
    }
    fn choice(&mut self, options: u32) -> Result<u32, String> {
        Some(self.uint()?)
            .filter(|&v| v < options)
            .ok_or_else(|| "invalid enumeration in cooked scene".into())
    }
    /// Fail before allocating `bytes` that the input cannot hold.
    fn holds(&self, bytes: u64) -> Result<(), String> {
        if bytes > self.0.len() as u64 {
            return Err("cooked counts exceed file size".into());
        }
        Ok(())
    }
}

fn instances(r: &mut Read, count: usize) -> Result<Vec<Instance>, String> {
    (0..count)
        .map(|_| {
            Ok(Instance {
                mesh: r.uint()?,
                transform: Transform(r.floats()?),
            })
        })
        .collect()
}

fn decode_v1(r: &mut Read) -> Result<World, String> {
    let meshes = r.uint()? as usize;
    let instance_count = r.uint()? as usize;
    if meshes > MAX_MESHES || instance_count > MAX_INSTANCES {
        return Err("invalid world counts".into());
    }
    r.holds(20 * meshes as u64 + INSTANCE_BYTES * instance_count as u64)?;
    let mut world = World {
        meshes: Vec::with_capacity(meshes),
        ..World::default()
    };
    for _ in 0..meshes {
        let n = r.uint()? as usize;
        let k = r.uint()? as usize;
        let color = r.floats()?;
        if k % 3 != 0 {
            return Err("invalid mesh ranges".into());
        }
        r.holds(n as u64 * 12 + k as u64 * 4)?;
        let positions = r.vec3s(n)?;
        let indices = (0..k).map(|_| r.uint()).collect::<Result<_, _>>()?;
        world.meshes.push(Mesh {
            positions,
            indices,
            color,
            ..Mesh::default()
        });
    }
    world.instances = instances(r, instance_count)?;
    Ok(world)
}

fn decode_v2(r: &mut Read) -> Result<World, String> {
    let [meshes, instance_count, materials, textures, lights] =
        [(); 5].map(|()| r.uint().map(|n| n as usize));
    let (meshes, instance_count, materials, textures, lights) =
        (meshes?, instance_count?, materials?, textures?, lights?);
    if meshes > MAX_MESHES
        || instance_count > MAX_INSTANCES
        || materials > MAX_MATERIALS
        || textures > MAX_TEXTURES
        || lights > MAX_LIGHTS
    {
        return Err("invalid world counts".into());
    }
    r.holds(
        MESH_BYTES * meshes as u64
            + INSTANCE_BYTES * instance_count as u64
            + MATERIAL_BYTES * materials as u64
            + TEXTURE_BYTES * textures as u64
            + LIGHT_BYTES * lights as u64,
    )?;
    let mut world = World {
        meshes: Vec::with_capacity(meshes),
        materials: Vec::with_capacity(materials),
        textures: Vec::with_capacity(textures),
        lights: Vec::with_capacity(lights),
        ..World::default()
    };
    for _ in 0..meshes {
        let n = r.uint()? as usize;
        let k = r.uint()? as usize;
        let flags = r.uint()?;
        let color = r.floats()?;
        let material = r.index()?;
        if k % 3 != 0 || flags > 3 {
            return Err("invalid mesh ranges".into());
        }
        let (normals, uvs) = (flags & 1 != 0, flags & 2 != 0);
        let per_vertex = 12 + 12 * u64::from(normals) + 8 * u64::from(uvs);
        r.holds(n as u64 * per_vertex + k as u64 * 4)?;
        let positions = r.vec3s(n)?;
        let normals = if normals { r.vec3s(n)? } else { vec![] };
        let uvs = if uvs {
            (0..n).map(|_| r.floats()).collect::<Result<_, _>>()?
        } else {
            vec![]
        };
        let indices = (0..k).map(|_| r.uint()).collect::<Result<_, _>>()?;
        world.meshes.push(Mesh {
            positions,
            normals,
            uvs,
            indices,
            color,
            material,
        });
    }
    world.instances = instances(r, instance_count)?;
    for _ in 0..materials {
        let len = r.uint()? as usize;
        if len > MAX_NAME_BYTES {
            return Err("material name too long".into());
        }
        let raw = r.bytes(padded(len))?;
        if raw[len..].iter().any(|&b| b != 0) {
            return Err("nonzero material name padding".into());
        }
        let name =
            String::from_utf8(raw[..len].to_vec()).map_err(|_| "material name is not UTF-8")?;
        let base_color = r.floats()?;
        let base_color_texture = r.index()?;
        let normal_texture = r.index()?;
        let [normal_scale] = r.floats()?;
        let emissive = r.floats()?;
        let emissive_texture = r.index()?;
        let [metallic, roughness] = r.floats()?;
        let mode = r.choice(3)?;
        let [cutoff] = r.floats()?;
        if mode != 1 && cutoff.to_bits() != 0 {
            return Err("alpha cutoff without alpha mask".into());
        }
        let alpha = [AlphaMode::Opaque, AlphaMode::Mask(cutoff), AlphaMode::Blend][mode as usize];
        let double_sided = r.choice(2)? == 1;
        let shading = [
            Shading::Standard,
            Shading::Foliage,
            Shading::Water,
            Shading::Unlit,
        ][r.choice(4)? as usize];
        let emission = [Emission::Constant, Emission::Lamp][r.choice(2)? as usize];
        world.materials.push(Material {
            name,
            base_color,
            base_color_texture,
            normal_texture,
            normal_scale,
            emissive,
            emissive_texture,
            metallic,
            roughness,
            alpha,
            double_sided,
            shading,
            emission,
        });
    }
    for _ in 0..textures {
        let [width, height] = [r.uint()?, r.uint()?];
        let srgb = r.choice(2)? == 1;
        let count = r.uint()? as usize;
        let limit = 1..=MAX_TEXTURE_SIZE;
        if !limit.contains(&width)
            || !limit.contains(&height)
            || count != Texture::level_count(width, height)
        {
            return Err("invalid texture size or mip chain".into());
        }
        let sizes: Vec<u64> = (0..count)
            .map(|i| {
                let (w, h) = Texture::level_extent(width, height, i);
                u64::from(w) * u64::from(h) * 4
            })
            .collect();
        r.holds(sizes.iter().sum())?;
        let levels = sizes
            .into_iter()
            .map(|n| r.bytes(n).map(<[u8]>::to_vec))
            .collect::<Result<_, _>>()?;
        world.textures.push(Texture {
            width,
            height,
            srgb,
            levels,
        });
    }
    for _ in 0..lights {
        let [x, y, z] = r.floats()?;
        let color = r.floats()?;
        let [intensity, range] = r.floats()?;
        world.lights.push(PointLight {
            position: Vec3(x, y, z),
            color,
            intensity,
            range,
        });
    }
    Ok(world)
}

/// Decode an ABEW file of version 1 or 2.
pub fn decode(data: &[u8]) -> Result<World, String> {
    if data.len() > MAX_BYTES {
        return Err("cooked file too large".into());
    }
    let mut r = Read(data);
    if &r.word()? != MAGIC {
        return Err("unsupported ABEW header or version".into());
    }
    let world = match r.uint()? {
        1 => decode_v1(&mut r)?,
        2 => decode_v2(&mut r)?,
        _ => return Err("unsupported ABEW header or version".into()),
    };
    if !r.0.is_empty() {
        return Err("trailing bytes after scene".into());
    }
    world.validate()?;
    Ok(world)
}
