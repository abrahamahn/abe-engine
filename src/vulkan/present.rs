//! Desktop Vulkan WSI. One graphics/present queue, two frames in flight, one
//! render-finished semaphore per swapchain image. The ordinary frame path
//! performs neither CPU pixel readback nor queue/device-wide idle waits.
use super::{frame::Frame, *};
use crate::{Camera, Scene, window::Window};
use core::ffi::CStr;
use core::mem::{size_of_val, zeroed};

const PRESENT_LAYOUT: i32 = 1_000_001_002;
const SUBOPTIMAL: i32 = 1_000_001_003;
const OUT_OF_DATE: i32 = -1_000_001_004;
const COLOR_ATTACHMENT: u32 = 0x10;
const FRAMES: usize = 2;
const TIMEOUT_NS: u64 = 100_000_000;

#[cfg(target_os = "linux")]
const NATIVE_SURFACE: &CStr = c"VK_KHR_xlib_surface";
#[cfg(target_os = "windows")]
const NATIVE_SURFACE: &CStr = c"VK_KHR_win32_surface";

/// Owns the GPU device and surface. The native window must outlive both.
pub struct DesktopRenderer<'w> {
    gpu: VulkanRenderer,
    window: &'w Window,
}
impl<'w> DesktopRenderer<'w> {
    pub fn new(window: &'w Window) -> Result<Self> {
        let mut gpu =
            VulkanRenderer::instance(&[c"VK_KHR_surface".as_ptr(), NATIVE_SURFACE.as_ptr()])?;
        #[cfg(target_os = "linux")]
        {
            let mut ci = VkXlibSurfaceCreateInfoKHR::new();
            ci.dpy = window.display;
            ci.window = window.id;
            gpu.surface = create("vkCreateXlibSurfaceKHR", |o| unsafe {
                vkCreateXlibSurfaceKHR(gpu.instance, &ci, null(), o)
            })?;
        }
        #[cfg(target_os = "windows")]
        {
            let mut ci = VkWin32SurfaceCreateInfoKHR::new();
            ci.hinstance = window.instance;
            ci.hwnd = window.id;
            gpu.surface = create("vkCreateWin32SurfaceKHR", |o| unsafe {
                vkCreateWin32SurfaceKHR(gpu.instance, &ci, null(), o)
            })?;
        }
        gpu.select_device()?;
        Ok(Self { gpu, window })
    }
    pub fn device_name(&self) -> &str {
        self.gpu.device_name()
    }
    pub fn presenter(&mut self) -> Presenter<'_> {
        Presenter {
            gpu: &self.gpu,
            window: self.window,
            chain: None,
            dirty: true,
            poisoned: false,
            stats: Default::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentStatus {
    Presented,
    Suspended,
    Retry,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PresentStats {
    pub frames: u64,
    pub swapchains: u64,
    pub vertex_allocations: u64,
}

pub struct Presenter<'a> {
    gpu: &'a VulkanRenderer,
    window: &'a Window,
    chain: Option<Chain<'a>>,
    dirty: bool,
    poisoned: bool,
    stats: PresentStats,
}
impl Presenter<'_> {
    pub fn stats(&self) -> PresentStats {
        self.stats
    }
    pub fn draw(
        &mut self,
        scene: &Scene,
        camera: Camera,
        clear: [f32; 4],
    ) -> Result<PresentStatus> {
        if self.poisoned {
            return Err("presentation failed previously; recreate the presenter".into());
        }
        let size = self.window.size();
        if !self.window.visible() || size.0 == 0 || size.1 == 0 {
            self.dirty = true;
            return Ok(PresentStatus::Suspended);
        }
        if self.dirty || self.chain.as_ref().is_none_or(|c| c.requested != size) {
            // Retire every reference to the old surface images before replacement.
            self.chain.take();
            self.chain = Chain::new(self.gpu, size)?;
            if self.chain.is_none() {
                return Ok(PresentStatus::Retry);
            }
            self.stats.swapchains += 1;
            self.dirty = false;
        }
        let chain = self.chain.as_mut().expect("chain initialized");
        let (w, h) = chain.size;
        let vertices = camera.vertices(scene, w, h).map_err(str::to_string)?;
        if vertices.len() > u32::MAX as usize {
            return Err("too many vertices".into());
        }
        match chain.draw(&vertices, clear, &mut self.stats) {
            Ok((status, rebuild)) => {
                self.dirty = rebuild;
                if status == PresentStatus::Presented {
                    self.stats.frames += 1;
                }
                Ok(status)
            }
            Err(e) => {
                self.poisoned = true;
                Err(e)
            }
        }
    }
}
struct Swap<'a> {
    gpu: &'a VulkanRenderer,
    handle: u64,
}
impl Drop for Swap<'_> {
    fn drop(&mut self) {
        unsafe { vkDestroySwapchainKHR(self.gpu.device, self.handle, null()) }
    }
}
struct Sync<'a> {
    gpu: &'a VulkanRenderer,
    semaphores: Vec<u64>,
    fences: Vec<u64>,
}
impl Sync<'_> {
    fn semaphore(&mut self) -> Result<u64> {
        let h = create("vkCreateSemaphore", |o| unsafe {
            vkCreateSemaphore(self.gpu.device, &VkSemaphoreCreateInfo::new(), null(), o)
        })?;
        self.semaphores.push(h);
        Ok(h)
    }
    fn fence(&mut self) -> Result<u64> {
        let mut ci = VkFenceCreateInfo::new();
        ci.flags = 1; // signaled on first use
        let h = create("vkCreateFence", |o| unsafe {
            vkCreateFence(self.gpu.device, &ci, null(), o)
        })?;
        self.fences.push(h);
        Ok(h)
    }
}
impl Drop for Sync<'_> {
    fn drop(&mut self) {
        unsafe {
            for &h in &self.semaphores {
                vkDestroySemaphore(self.gpu.device, h, null());
            }
            for &h in &self.fences {
                vkDestroyFence(self.gpu.device, h, null());
            }
        }
    }
}
struct Slot {
    cmd: Handle,
    available: u64,
    fence: u64,
    buffer: u64,
    memory: u64,
    capacity: usize,
}
struct Target {
    framebuffer: u64,
    finished: u64,
}
struct Chain<'a> {
    // Fields drop in declaration order: image views & framebuffers before the swapchain.
    resources: Frame<'a>,
    _sync: Sync<'a>,
    swap: Swap<'a>,
    slots: Vec<Slot>,
    targets: Vec<Target>,
    slot: usize,
    size: (u32, u32),
    requested: (u32, u32),
}
impl Drop for Chain<'_> {
    fn drop(&mut self) {
        unsafe {
            vkDeviceWaitIdle(self.swap.gpu.device);
        }
    }
}

fn format_for(formats: &[VkSurfaceFormatKHR]) -> Result<VkSurfaceFormatKHR> {
    if formats.len() == 1 && formats[0].format == 0 {
        return Ok(VkSurfaceFormatKHR {
            format: 44,
            colorSpace: formats[0].colorSpace,
        });
    }
    // BGRA/RGBA UNORM or sRGB, always in the standard nonlinear color space.
    for preferred in [44, 37, 50, 43] {
        if let Some(f) = formats
            .iter()
            .find(|f| f.format == preferred && f.colorSpace == 0)
        {
            return Ok(*f);
        }
    }
    Err("no supported 8-bit RGB surface format".into())
}
fn extent_for(caps: &VkSurfaceCapabilitiesKHR, size: (u32, u32)) -> (u32, u32) {
    if caps.currentExtent.width != u32::MAX {
        return (caps.currentExtent.width, caps.currentExtent.height);
    }
    (
        size.0
            .clamp(caps.minImageExtent.width, caps.maxImageExtent.width),
        size.1
            .clamp(caps.minImageExtent.height, caps.maxImageExtent.height),
    )
}

impl<'a> Chain<'a> {
    fn new(gpu: &'a VulkanRenderer, requested: (u32, u32)) -> Result<Option<Self>> {
        unsafe {
            let mut caps = zeroed();
            check(
                vkGetPhysicalDeviceSurfaceCapabilitiesKHR(gpu.physical, gpu.surface, &mut caps),
                "surface capabilities",
            )?;
            let (w, h) = extent_for(&caps, requested);
            if w == 0 || h == 0 {
                return Ok(None);
            }
            if caps.supportedUsageFlags & COLOR_ATTACHMENT == 0 {
                return Err("surface cannot be used as a color attachment".into());
            }
            let mut count = 0;
            check(
                vkGetPhysicalDeviceSurfaceFormatsKHR(
                    gpu.physical,
                    gpu.surface,
                    &mut count,
                    null_mut(),
                ),
                "count surface formats",
            )?;
            let mut formats = vec![zeroed(); count as usize];
            check(
                vkGetPhysicalDeviceSurfaceFormatsKHR(
                    gpu.physical,
                    gpu.surface,
                    &mut count,
                    formats.as_mut_ptr(),
                ),
                "get surface formats",
            )?;
            formats.truncate(count as usize);
            let format = format_for(&formats)?;
            let mut ci = VkSwapchainCreateInfoKHR::new();
            ci.surface = gpu.surface;
            ci.minImageCount = caps.minImageCount.saturating_add(1);
            if caps.maxImageCount != 0 {
                ci.minImageCount = ci.minImageCount.min(caps.maxImageCount);
            }
            ci.imageFormat = format.format;
            ci.imageColorSpace = format.colorSpace;
            ci.imageExtent = VkExtent2D {
                width: w,
                height: h,
            };
            ci.imageArrayLayers = 1;
            ci.imageUsage = COLOR_ATTACHMENT;
            ci.preTransform = caps.currentTransform;
            ci.compositeAlpha = [1, 2, 4, 8]
                .into_iter()
                .find(|v| caps.supportedCompositeAlpha & *v != 0)
                .ok_or("no composite alpha mode")? as i32;
            ci.presentMode = 2; // FIFO is required on all Vulkan surfaces
            ci.clipped = 1;
            let mut raw = 0;
            let rc = vkCreateSwapchainKHR(gpu.device, &ci, null(), &mut raw);
            if rc == OUT_OF_DATE {
                return Ok(None);
            }
            check(rc, "vkCreateSwapchainKHR")?;
            let swap = Swap { gpu, handle: raw };
            let mut resources = Frame::new(gpu);
            let mut sync = Sync {
                gpu,
                semaphores: vec![],
                fences: vec![],
            };
            let mut count = 0;
            check(
                vkGetSwapchainImagesKHR(gpu.device, raw, &mut count, null_mut()),
                "count swapchain images",
            )?;
            let mut images = vec![0; count as usize];
            check(
                vkGetSwapchainImagesKHR(gpu.device, raw, &mut count, images.as_mut_ptr()),
                "get swapchain images",
            )?;
            images.truncate(count as usize);
            resources.render_pass(format.format, PRESENT_LAYOUT)?;
            resources.create_pipeline(w, h)?;
            let mut targets = Vec::with_capacity(images.len());
            for image in images {
                let color = resources.view(image, format.format, 1)?;
                let (_, depth) = resources.image(w, h, gpu.depth_format, 0x20, 2)?;
                let framebuffer = resources.framebuffer(w, h, color, depth)?;
                targets.push(Target {
                    framebuffer,
                    finished: sync.semaphore()?,
                });
            }
            let mut slots = Vec::with_capacity(FRAMES);
            for _ in 0..FRAMES {
                slots.push(Slot {
                    cmd: resources.commands()?,
                    available: sync.semaphore()?,
                    fence: sync.fence()?,
                    buffer: 0,
                    memory: 0,
                    capacity: 0,
                });
            }
            Ok(Some(Self {
                resources,
                _sync: sync,
                swap,
                targets,
                slots,
                size: (w, h),
                requested,
                slot: 0,
            }))
        }
    }
    fn draw(
        &mut self,
        vertices: &[Vertex],
        clear: [f32; 4],
        stats: &mut PresentStats,
    ) -> Result<(PresentStatus, bool)> {
        let gpu = self.swap.gpu;
        let slot = &mut self.slots[self.slot];
        let dev = gpu.device;
        // SAFETY: the slot fence covers command-buffer and vertex-buffer reuse; the
        // acquire semaphore gates access to the acquired image. The presentation wait
        // semaphore belongs to that image and is not reused until it is acquired again.
        unsafe {
            let wait = vkWaitForFences(dev, 1, &slot.fence, 1, TIMEOUT_NS);
            if wait == 2 {
                return Ok((PresentStatus::Retry, false));
            }
            check(wait, "frame fence")?;
            let bytes = size_of_val(vertices);
            if bytes > 0 {
                if bytes > slot.capacity {
                    let capacity = bytes
                        .checked_next_power_of_two()
                        .ok_or("vertex buffer size overflow")?;
                    let (buf, mem) = self.resources.buffer(capacity as u64, 0x80)?;
                    if slot.buffer != 0 {
                        self.resources.release_buffer(slot.buffer, slot.memory);
                    }
                    slot.buffer = buf;
                    slot.memory = mem;
                    slot.capacity = capacity;
                    stats.vertex_allocations += 1;
                }
                let mapped = create("map frame vertices", |o| {
                    vkMapMemory(dev, slot.memory, 0, bytes as u64, 0, o)
                })?;
                core::ptr::copy_nonoverlapping(
                    vertices.as_ptr().cast::<u8>(),
                    mapped.cast::<u8>(),
                    bytes,
                );
                vkUnmapMemory(dev, slot.memory);
            }
            let mut index = 0;
            let acquire = vkAcquireNextImageKHR(
                dev,
                self.swap.handle,
                TIMEOUT_NS,
                slot.available,
                0,
                &mut index,
            );
            match acquire {
                OUT_OF_DATE => return Ok((PresentStatus::Retry, true)),
                1 | 2 => return Ok((PresentStatus::Retry, false)),
                0 | SUBOPTIMAL => {}
                other => check(other, "vkAcquireNextImageKHR")?,
            }
            let target = &self.targets[index as usize];
            check(vkResetCommandBuffer(slot.cmd, 0), "vkResetCommandBuffer")?;
            let mut begin = VkCommandBufferBeginInfo::new();
            begin.flags = 1;
            check(
                vkBeginCommandBuffer(slot.cmd, &begin),
                "vkBeginCommandBuffer",
            )?;
            self.resources.record_draw(
                slot.cmd,
                target.framebuffer,
                self.size,
                clear,
                slot.buffer,
                vertices.len() as u32,
            );
            check(vkEndCommandBuffer(slot.cmd), "vkEndCommandBuffer")?;
            // Reset only after successful acquire and recording, just before submitting.
            check(vkResetFences(dev, 1, &slot.fence), "vkResetFences")?;
            let stages = 0x1_0000u32; // ALL_COMMANDS; covers the color and depth attachments
            let mut submit = VkSubmitInfo::new();
            submit.waitSemaphoreCount = 1;
            submit.pWaitSemaphores = &slot.available;
            submit.pWaitDstStageMask = &stages;
            submit.commandBufferCount = 1;
            submit.pCommandBuffers = &slot.cmd;
            submit.signalSemaphoreCount = 1;
            submit.pSignalSemaphores = &target.finished;
            check(
                vkQueueSubmit(gpu.queue, 1, &submit, slot.fence),
                "present draw submit",
            )?;
            let mut pi = VkPresentInfoKHR::new();
            pi.waitSemaphoreCount = 1;
            pi.pWaitSemaphores = &target.finished;
            pi.swapchainCount = 1;
            pi.pSwapchains = &self.swap.handle;
            pi.pImageIndices = &index;
            let result = vkQueuePresentKHR(gpu.queue, &pi);
            self.slot = (self.slot + 1) % self.slots.len();
            match result {
                OUT_OF_DATE => Ok((PresentStatus::Retry, true)),
                0 | SUBOPTIMAL => Ok((
                    PresentStatus::Presented,
                    result == SUBOPTIMAL || acquire == SUBOPTIMAL,
                )),
                other => Err(format!("vkQueuePresentKHR: VkResult {other}")),
            }
        }
    }
}
