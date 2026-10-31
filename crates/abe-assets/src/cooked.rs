//! ABEW v1. Little-endian, indexed triangle meshes and affine mesh instances.
use abe_core::{
    Vec3,
    world::{Instance, Mesh, Transform, World},
};
const MAGIC: &[u8; 4] = b"ABEW";
pub const MAX_BYTES: usize = 256 * 1024 * 1024;
const MAX_MESHES: usize = 65_536;
const MAX_INSTANCES: usize = 250_000;

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
pub fn encode(w: &World) -> Result<Vec<u8>, String> {
    w.validate()?;
    if w.meshes.len() > MAX_MESHES || w.instances.len() > MAX_INSTANCES {
        return Err("too many meshes or instances".into());
    }
    let size = w
        .meshes
        .iter()
        .try_fold(16_u64 + 68 * w.instances.len() as u64, |a, m| {
            a.checked_add(20 + 12 * m.positions.len() as u64 + 4 * m.indices.len() as u64)
                .filter(|s| *s <= MAX_BYTES as u64)
        })
        .ok_or("cooked file too large")?;
    let mut v = Vec::with_capacity(size as usize);
    v.extend_from_slice(MAGIC);
    put_u32(&mut v, 1)?;
    put_u32(&mut v, w.meshes.len())?;
    put_u32(&mut v, w.instances.len())?;
    for m in &w.meshes {
        put_u32(&mut v, m.positions.len())?;
        put_u32(&mut v, m.indices.len())?;
        put_floats(&mut v, &m.color);
        for &Vec3(x, y, z) in &m.positions {
            put_floats(&mut v, &[x, y, z]);
        }
        for i in &m.indices {
            v.extend_from_slice(&i.to_le_bytes());
        }
    }
    for i in &w.instances {
        v.extend_from_slice(&i.mesh.to_le_bytes());
        put_floats(&mut v, &i.transform.0);
    }
    Ok(v)
}
struct Read<'a>(&'a [u8]);
impl Read<'_> {
    fn word(&mut self) -> Result<[u8; 4], String> {
        let (w, left) = self.0.split_at_checked(4).ok_or("truncated cooked scene")?;
        self.0 = left;
        Ok(w.try_into().expect("four bytes"))
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
}
pub fn decode(data: &[u8]) -> Result<World, String> {
    if data.len() > MAX_BYTES {
        return Err("cooked file too large".into());
    }
    let mut r = Read(data);
    if &r.word()? != MAGIC || r.uint()? != 1 {
        return Err("unsupported ABEW header or version".into());
    }
    let meshes = r.uint()? as usize;
    let instances = r.uint()? as usize;
    if meshes > MAX_MESHES || instances > MAX_INSTANCES || 20 * meshes + 68 * instances > r.0.len()
    {
        return Err("invalid world counts".into());
    }
    let mut world = World {
        meshes: Vec::with_capacity(meshes),
        instances: Vec::with_capacity(instances),
    };
    for _ in 0..meshes {
        let n = r.uint()? as usize;
        let k = r.uint()? as usize;
        let color = r.floats()?;
        if n as u64 * 12 + k as u64 * 4 > r.0.len() as u64 || k % 3 != 0 {
            return Err("invalid mesh ranges".into());
        }
        let mut positions = Vec::with_capacity(n);
        let mut indices = Vec::with_capacity(k);
        for _ in 0..n {
            let [x, y, z] = r.floats()?;
            positions.push(Vec3(x, y, z));
        }
        for _ in 0..k {
            indices.push(r.uint()?);
        }
        world.meshes.push(Mesh {
            positions,
            indices,
            color,
        });
    }
    for _ in 0..instances {
        world.instances.push(Instance {
            mesh: r.uint()?,
            transform: Transform(r.floats()?),
        })
    }
    if !r.0.is_empty() {
        return Err("trailing bytes after scene".into());
    }
    world.validate()?;
    Ok(world)
}
