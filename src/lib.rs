//! ABE's portable scene data and camera. Game-specific content belongs to clients.

#[cfg(any(target_os = "linux", target_os = "windows", target_os = "android"))]
pub mod vulkan;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec3(pub f32, pub f32, pub f32);

impl Vec3 {
    fn dot(self, b: Self) -> f32 {
        self.0 * b.0 + self.1 * b.1 + self.2 * b.2
    }
    fn cross(self, b: Self) -> Self {
        Self(
            self.1 * b.2 - self.2 * b.1,
            self.2 * b.0 - self.0 * b.2,
            self.0 * b.1 - self.1 * b.0,
        )
    }
    fn sub(self, b: Self) -> Self {
        Self(self.0 - b.0, self.1 - b.1, self.2 - b.2)
    }
    fn unit(self) -> Option<Self> {
        let length = self.dot(self).sqrt();
        (length.is_finite() && length > 1e-6)
            .then(|| Self(self.0 / length, self.1 / length, self.2 / length))
    }
}

/// RGB values in the output target's 0..1 range.
pub type Rgb = [f32; 3];

#[derive(Clone, Copy, Debug)]
pub struct Triangle {
    pub positions: [Vec3; 3],
    pub color: Rgb,
}

#[derive(Default, Debug)]
pub struct Scene {
    pub triangles: Vec<Triangle>,
}

impl Scene {
    pub fn triangle(&mut self, a: Vec3, b: Vec3, c: Vec3, color: Rgb) {
        self.triangles.push(Triangle {
            positions: [a, b, c],
            color,
        });
    }
    pub fn quad(&mut self, corners: [Vec3; 4], color: Rgb) {
        let [a, b, c, d] = corners;
        self.triangle(a, b, c, color);
        self.triangle(a, c, d, color);
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub eye: Vec3,
    pub target: Vec3,
    pub vertical_fov_degrees: f32,
    pub near: f32,
    pub far: f32,
}

/// Vulkan homogeneous clip coordinates. +Y in world space points up.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    pub clip: [f32; 4],
    pub rgba: [f32; 4],
}

impl Camera {
    pub fn vertices(
        self,
        scene: &Scene,
        width: u32,
        height: u32,
    ) -> Result<Vec<Vertex>, &'static str> {
        if width == 0
            || height == 0
            || !self.near.is_finite()
            || !self.far.is_finite()
            || self.near <= 0.0
            || self.far <= self.near
            || !(0.0 < self.vertical_fov_degrees && self.vertical_fov_degrees < 179.0)
        {
            return Err("invalid viewport or camera frustum");
        }
        let forward = self
            .target
            .sub(self.eye)
            .unit()
            .ok_or("invalid camera direction")?;
        let right = forward
            .cross(Vec3(0.0, 1.0, 0.0))
            .unit()
            .ok_or("camera direction parallel to world up")?;
        let up = right.cross(forward);
        let scale = 1.0 / (0.5 * self.vertical_fov_degrees.to_radians()).tan();
        let aspect = width as f32 / height as f32;
        let depth = self.far / (self.far - self.near);
        let mut vertices = Vec::with_capacity(scene.triangles.len() * 3);
        for tri in &scene.triangles {
            for p in tri.positions {
                let delta = p.sub(self.eye);
                let z = delta.dot(forward);
                vertices.push(Vertex {
                    clip: [
                        delta.dot(right) * scale / aspect,
                        -delta.dot(up) * scale,
                        (z - self.near) * depth,
                        z,
                    ],
                    rgba: [tri.color[0], tri.color[1], tri.color[2], 1.0],
                });
            }
        }
        Ok(vertices)
    }
}
