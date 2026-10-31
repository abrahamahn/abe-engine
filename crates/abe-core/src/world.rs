//! Mesh geometry is stored once, with affine instances for world placement.
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

#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    pub positions: Vec<Vec3>,
    pub indices: Vec<u32>,
    pub color: Rgb,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Instance {
    pub mesh: u32,
    pub transform: Transform,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct World {
    pub meshes: Vec<Mesh>,
    pub instances: Vec<Instance>,
}

// Limit expansion from many instances of one mesh before allocating output.
pub const MAX_TRIANGLES: usize = 2_000_000;
impl World {
    pub fn validate(&self) -> Result<usize, String> {
        for (i, m) in self.meshes.iter().enumerate() {
            if m.indices.len() % 3 != 0
                || m.indices.iter().any(|j| *j as usize >= m.positions.len())
            {
                return Err(format!("invalid triangle indices in mesh {i}"));
            }
            if m.positions
                .iter()
                .any(|p| ![p.0, p.1, p.2].into_iter().all(f32::is_finite))
                || !m.color.into_iter().all(f32::is_finite)
            {
                return Err(format!("non-finite data in mesh {i}"));
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
