//! Immutable world-space vertices. The owning draw path fences outstanding uses before drop.
use super::{
    frame::{Draw, Frame},
    *,
};
use crate::{Scene, world::MAX_TRIANGLES};

pub(super) struct Resident<'a> {
    _resources: Frame<'a>,
    buffer: u64,
    count: u32,
}
impl<'a> Resident<'a> {
    pub fn upload(gpu: &'a VulkanRenderer, scene: &Scene) -> Result<Self> {
        if scene.triangles.len() > MAX_TRIANGLES {
            return Err("scene exceeds triangle limit".into());
        }
        let count = (scene.triangles.len() * 3) as u32;
        let mut resources = Frame::new(gpu);
        let mut buffer = 0;
        if count > 0 {
            let bytes = u64::from(count) * size_of::<Vertex>() as u64;
            let (vertex, memory) = resources.buffer(bytes, 0x80)?;
            // SAFETY: coherent memory is sized for exactly count aligned vertices. The
            // buffer is new and unreferenced by the graphics queue until this returns.
            unsafe {
                let mapped = create("map resident geometry", |p| {
                    vkMapMemory(gpu.device, memory, 0, bytes, 0, p)
                })?;
                let mut dest = mapped.cast::<Vertex>();
                for t in &scene.triangles {
                    let [r, g, b] = t.color;
                    for p in t.positions {
                        dest.write(Vertex {
                            clip: [p.0, p.1, p.2, 1.],
                            rgba: [r, g, b, 1.],
                        });
                        dest = dest.add(1);
                    }
                }
                vkUnmapMemory(gpu.device, memory);
            }
            buffer = vertex;
        }
        Ok(Self {
            _resources: resources,
            buffer,
            count,
        })
    }
    pub fn draw(&self, matrix: [f32; 16]) -> Draw {
        Draw {
            buffer: self.buffer,
            count: self.count,
            matrix,
        }
    }
}
