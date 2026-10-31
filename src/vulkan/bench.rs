//! Unpaced offscreen rendering. The two slots use the same draw path as the native player.
use super::{frame::Frame, resident::Resident, *};
use crate::{Camera, Scene};
use std::time::Instant;

#[derive(Debug)]
pub struct Run {
    pub frames: usize,
    pub seconds: f64,
    pub cpu_ms: Vec<f64>,
    pub gpu_ms: Vec<f64>,
    pub fence_ms: Vec<f64>,
}
impl Run {
    pub fn throughput(&self) -> f64 {
        self.frames as f64 / self.seconds
    }
}

struct Slot<'a> {
    gpu: &'a VulkanRenderer,
    frame: Frame<'a>,
    framebuffer: u64,
    command: Handle,
    fence: u64,
    queries: u64,
    pending: bool,
}
impl<'a> Slot<'a> {
    fn new(gpu: &'a VulkanRenderer, (w, h): (u32, u32)) -> Result<Self> {
        let mut frame = Frame::new(gpu);
        let (_, color) = frame.image(w, h, 37, 0x10, 1)?;
        let (_, depth) = frame.image(w, h, gpu.depth_format, 0x20, 2)?;
        frame.render_pass(37, 2)?;
        let framebuffer = frame.framebuffer(w, h, color, depth)?;
        frame.create_pipeline(w, h)?;
        let command = frame.commands()?;
        let mut s = Self {
            gpu,
            frame,
            framebuffer,
            command,
            fence: 0,
            queries: 0,
            pending: false,
        };
        unsafe {
            s.fence = create("create benchmark fence", |p| {
                vkCreateFence(gpu.device, &VkFenceCreateInfo::new(), null(), p)
            })?;
            if gpu.timestamp_bits > 0 && gpu.timestamp_period_ns > 0.0 {
                let mut info = VkQueryPoolCreateInfo::new();
                info.queryType = 2;
                info.queryCount = 2;
                s.queries = create("create timestamp queries", |p| {
                    vkCreateQueryPool(gpu.device, &info, null(), p)
                })?;
            }
        }
        Ok(s)
    }
    fn collect(&mut self, out: &mut Run) -> Result<()> {
        if !self.pending {
            return Ok(());
        }
        let start = Instant::now();
        // SAFETY: our fence covers the previous submission, including both query writes.
        unsafe {
            check(
                vkWaitForFences(self.gpu.device, 1, &self.fence, 1, u64::MAX),
                "await benchmark frame",
            )?;
            self.pending = false;
            out.fence_ms.push(start.elapsed().as_secs_f64() * 1000.);
            if self.queries != 0 {
                let mut times = [0_u64; 2];
                check(
                    vkGetQueryPoolResults(
                        self.gpu.device,
                        self.queries,
                        0,
                        2,
                        16,
                        times.as_mut_ptr().cast(),
                        8,
                        1,
                    ),
                    "read timestamps",
                )?;
                let mask = u64::MAX >> (64 - self.gpu.timestamp_bits);
                out.gpu_ms.push(
                    ((times[1].wrapping_sub(times[0])) & mask) as f64
                        * self.gpu.timestamp_period_ns
                        * 1e-6,
                );
            }
        }
        Ok(())
    }
    fn submit(&mut self, geom: &Resident<'_>, size: (u32, u32), matrix: [f32; 16]) -> Result<()> {
        let cmd = self.command;
        // SAFETY: collect completed previous uses of this slot. Its attachments and
        // command buffer remain alive until the fence signals, including on early exit.
        unsafe {
            check(vkResetCommandBuffer(cmd, 0), "reset benchmark command")?;
            let mut begin = VkCommandBufferBeginInfo::new();
            begin.flags = 1;
            check(vkBeginCommandBuffer(cmd, &begin), "begin benchmark frame")?;
            if self.queries != 0 {
                vkCmdResetQueryPool(cmd, self.queries, 0, 2);
                vkCmdWriteTimestamp(cmd, 1, self.queries, 0);
            }
            self.frame.record_draw(
                cmd,
                self.framebuffer,
                size,
                [0.03, 0.045, 0.07, 1.0],
                geom.draw(matrix),
            );
            if self.queries != 0 {
                vkCmdWriteTimestamp(cmd, 0x2000, self.queries, 1);
            }
            check(vkEndCommandBuffer(cmd), "end benchmark frame")?;
            check(
                vkResetFences(self.gpu.device, 1, &self.fence),
                "reset benchmark fence",
            )?;
            let mut sub = VkSubmitInfo::new();
            sub.commandBufferCount = 1;
            sub.pCommandBuffers = &cmd;
            check(
                vkQueueSubmit(self.gpu.queue, 1, &sub, self.fence),
                "submit benchmark frame",
            )?;
            self.pending = true;
        }
        Ok(())
    }
}
impl Drop for Slot<'_> {
    fn drop(&mut self) {
        unsafe {
            if self.pending {
                vkDeviceWaitIdle(self.gpu.device);
            }
            vkDestroyQueryPool(self.gpu.device, self.queries, null());
            vkDestroyFence(self.gpu.device, self.fence, null());
        }
    }
}
/// Setup, asset upload, warmup and pixel readback are outside measured time. The
/// measured interval ends only after all submitted frames have completed.
pub fn run(
    gpu: &mut VulkanRenderer,
    scene: &Scene,
    size: (u32, u32),
    warmup: usize,
    frames: usize,
    mut camera: impl FnMut(usize) -> Camera,
) -> Result<Run> {
    let (w, h) = size;
    gpu.check_extent(w, h)?;
    if frames == 0 || frames > 100_000 || warmup > 100_000 {
        return Err("invalid benchmark frame count".into());
    }
    // Geometry is dropped after both slots, including if recording or collection fails.
    let geom = Resident::upload(gpu, scene)?;
    let mut slots = [Slot::new(gpu, size)?, Slot::new(gpu, size)?];
    let mut batch = |count, offset| -> Result<Run> {
        let mut out = Run {
            frames: count,
            seconds: 0.,
            cpu_ms: Vec::with_capacity(count),
            gpu_ms: Vec::with_capacity(count),
            fence_ms: Vec::with_capacity(count),
        };
        let wall = Instant::now();
        for i in 0..count {
            let slot = &mut slots[i % 2];
            slot.collect(&mut out)?;
            let start = Instant::now();
            let matrix = camera(i + offset).clip_matrix(w, h)?;
            slot.submit(&geom, size, matrix)?;
            out.cpu_ms.push(start.elapsed().as_secs_f64() * 1000.);
        }
        for slot in &mut slots {
            slot.collect(&mut out)?;
        }
        out.seconds = wall.elapsed().as_secs_f64();
        Ok(out)
    };
    batch(warmup, 0)?;
    batch(frames, warmup)
}
