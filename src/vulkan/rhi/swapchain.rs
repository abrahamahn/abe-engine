//! Desktop presentation: X11 or Win32 surface, FIFO swapchain, two frames in flight.
//!
//! Synchronization: each frame slot owns an acquire semaphore and a fence; each swapchain image
//! owns the semaphore its present waits on (it is reused only when the image is acquired
//! again, after that present was consumed). The frame's submission waits on the acquire
//! semaphore in the colour-output and transfer stages; the first barrier on the swapchain
//! texture uses those stages as its source scope, chaining after the wait. A final barrier moves
//! the texture to the present layout.
use super::record::Key;
use super::track::stage;
use super::*;
use crate::gpu::{Encoder, TextureUsage};
use crate::window::Window;
use core::ffi::CStr;

const SUBOPTIMAL: i32 = 1_000_001_003;
const OUT_OF_DATE: i32 = -1_000_001_004;
const ACQUIRE_TIMEOUT_NS: u64 = 1_000_000_000;

#[cfg(target_os = "linux")]
const NATIVE_SURFACE: &CStr = c"VK_KHR_xlib_surface";
#[cfg(target_os = "windows")]
const NATIVE_SURFACE: &CStr = c"VK_KHR_win32_surface";

struct Chain {
    handle: u64,
    images: Vec<TextureId>,
    /// Present-wait semaphore per image.
    finished: Vec<u64>,
    extent: (u32, u32),
    /// Window size the chain was created for.
    requested: (u32, u32),
}

pub(super) struct Presentation<'w> {
    window: &'w Window,
    pub surface: u64,
    format: VkSurfaceFormatKHR,
    chain: Option<Chain>,
    acquired: Option<(u32, TextureId)>,
    dirty: bool,
}
impl Presentation<'_> {
    /// Destroys the swapchain and its semaphores. The device must be idle and the swapchain
    /// textures (and their views) already removed.
    pub unsafe fn destroy_chain(&mut self, device: Handle) {
        if let Some(chain) = self.chain.take() {
            // SAFETY: guaranteed by the caller.
            unsafe {
                for s in chain.finished {
                    vkDestroySemaphore(device, s, null());
                }
                vkDestroySwapchainKHR(device, chain.handle, null());
            }
        }
        self.acquired = None;
    }
}

fn surface_format(f: i32) -> Option<Format> {
    Some(match f {
        50 => Format::Bgra8Srgb,
        44 => Format::Bgra8Unorm,
        43 => Format::Rgba8Srgb,
        37 => Format::Rgba8Unorm,
        _ => return None,
    })
}

impl<'w> Gpu<'w> {
    pub fn for_window(window: &'w Window) -> Result<Self> {
        let mut gpu = Self::with_instance(&[c"VK_KHR_surface", NATIVE_SURFACE])?;
        let surface;
        // SAFETY (both blocks): the window's native handles stay valid for 'w, which outlives
        // the Gpu and so the surface, destroyed in Drop before the instance.
        #[cfg(target_os = "linux")]
        {
            let mut ci = VkXlibSurfaceCreateInfoKHR::new();
            ci.dpy = window.display;
            ci.window = window.id;
            surface = create("vkCreateXlibSurfaceKHR", |o| unsafe {
                vkCreateXlibSurfaceKHR(gpu.instance, &ci, null(), o)
            })?;
        }
        #[cfg(target_os = "windows")]
        {
            let mut ci = VkWin32SurfaceCreateInfoKHR::new();
            ci.hinstance = window.instance;
            ci.hwnd = window.id;
            surface = create("vkCreateWin32SurfaceKHR", |o| unsafe {
                vkCreateWin32SurfaceKHR(gpu.instance, &ci, null(), o)
            })?;
        }
        gpu.present = Some(Presentation {
            window,
            surface,
            format: VkSurfaceFormatKHR {
                format: 0,
                colorSpace: 0,
            },
            chain: None,
            acquired: None,
            dirty: true,
        });
        gpu.open_device(surface)?;
        let format = gpu.pick_surface_format(surface)?;
        gpu.present.as_mut().expect("set above").format = format;
        Ok(gpu)
    }

    fn pick_surface_format(&self, surface: u64) -> Result<VkSurfaceFormatKHR> {
        // SAFETY: counts come from Vulkan; the array is sized to them.
        let formats = unsafe {
            let mut count = 0;
            check(
                vkGetPhysicalDeviceSurfaceFormatsKHR(
                    self.physical,
                    surface,
                    &mut count,
                    null_mut(),
                ),
                "count surface formats",
            )?;
            let mut formats = vec![core::mem::zeroed::<VkSurfaceFormatKHR>(); count as usize];
            let rc = vkGetPhysicalDeviceSurfaceFormatsKHR(
                self.physical,
                surface,
                &mut count,
                formats.as_mut_ptr(),
            );
            if rc != 5 {
                check(rc, "get surface formats")?;
            }
            formats.truncate(count as usize);
            formats
        };
        if let [only] = formats.as_slice() {
            if only.format == 0 {
                return Ok(VkSurfaceFormatKHR {
                    format: 50,
                    colorSpace: only.colorSpace,
                });
            }
        }
        // sRGB first: the renderer works in linear light and lets the hardware encode.
        for preferred in [50, 44, 43, 37] {
            if let Some(f) = formats
                .iter()
                .find(|f| f.format == preferred && f.colorSpace == 0)
            {
                return Ok(*f);
            }
        }
        Err("the surface offers no 8-bit BGRA/RGBA format in the sRGB colour space".into())
    }

    pub fn surface_format(&self) -> Format {
        self.present
            .as_ref()
            .and_then(|p| surface_format(p.format.format))
            .unwrap_or(Format::Bgra8Srgb)
    }

    pub fn surface_size(&self) -> (u32, u32) {
        match &self.present {
            Some(p) => p.chain.as_ref().map_or(p.window.size(), |c| c.extent),
            None => (0, 0),
        }
    }

    /// Replaces the swapchain. Leaves no chain when the surface has a zero extent.
    fn recreate(&mut self, size: (u32, u32)) -> Result<()> {
        if self.xfer_recording {
            self.flush_transfers(true)?;
        }
        let dev = self.device;
        // SAFETY: waiting for idle makes every swapchain image, view, framebuffer and
        // semaphore unused before they are destroyed.
        unsafe { check(vkDeviceWaitIdle(dev), "vkDeviceWaitIdle")? };
        for sub in &self.subs {
            sub.pending.set(None);
        }
        self.collect();
        let p = self.present.as_mut().expect("presentation device");
        let old: Vec<TextureId> = p
            .chain
            .as_ref()
            .map(|c| c.images.clone())
            .unwrap_or_default();
        for id in old {
            if let Some(t) = self.textures.remove(id.raw()) {
                let views: Vec<u64> = t.views.iter().map(|(_, v)| *v).collect();
                self.forget_views(&views);
                self.collect();
                self.free(Garbage::Image(t.image, Alloc::NONE, views));
            }
        }
        let p = self.present.as_mut().expect("presentation device");
        // SAFETY: the device is idle and the swapchain textures are gone.
        unsafe { p.destroy_chain(dev) };
        p.dirty = false;
        let (surface, format) = (p.surface, p.format);
        // SAFETY: plain queries and creation with locals that outlive each call.
        let (handle, images, extent, usage) = unsafe {
            let mut caps: VkSurfaceCapabilitiesKHR = core::mem::zeroed();
            check(
                vkGetPhysicalDeviceSurfaceCapabilitiesKHR(self.physical, surface, &mut caps),
                "surface capabilities",
            )?;
            let extent = if caps.currentExtent.width != u32::MAX {
                (caps.currentExtent.width, caps.currentExtent.height)
            } else {
                (
                    size.0
                        .clamp(caps.minImageExtent.width, caps.maxImageExtent.width),
                    size.1
                        .clamp(caps.minImageExtent.height, caps.maxImageExtent.height),
                )
            };
            if extent.0 == 0 || extent.1 == 0 {
                return Ok(());
            }
            if caps.supportedUsageFlags & 0x10 == 0 {
                return Err("the surface cannot be a colour attachment".into());
            }
            // COLOR_ATTACHMENT plus TRANSFER_DST (blits into the swapchain) and TRANSFER_SRC
            // (screenshots) when the surface allows them.
            let vk_usage = 0x10 | (caps.supportedUsageFlags & 0x3);
            let mut usage = TextureUsage::COLOR_TARGET;
            if vk_usage & 0x2 != 0 {
                usage |= TextureUsage::COPY_DST;
            }
            if vk_usage & 0x1 != 0 {
                usage |= TextureUsage::COPY_SRC;
            }
            let mut ci = VkSwapchainCreateInfoKHR::new();
            ci.surface = surface;
            ci.minImageCount = caps.minImageCount.saturating_add(1);
            if caps.maxImageCount != 0 {
                ci.minImageCount = ci.minImageCount.min(caps.maxImageCount);
            }
            ci.imageFormat = format.format;
            ci.imageColorSpace = format.colorSpace;
            ci.imageExtent = VkExtent2D {
                width: extent.0,
                height: extent.1,
            };
            ci.imageArrayLayers = 1;
            ci.imageUsage = vk_usage;
            ci.preTransform = caps.currentTransform;
            ci.compositeAlpha = [1, 2, 4, 8]
                .into_iter()
                .find(|v| caps.supportedCompositeAlpha & *v != 0)
                .ok_or("no supported composite alpha mode")? as i32;
            ci.presentMode = 2; // FIFO: always available, paced by the display
            ci.clipped = 1;
            let mut handle = 0;
            let rc = vkCreateSwapchainKHR(dev, &ci, null(), &mut handle);
            if rc == OUT_OF_DATE {
                let p = self.present.as_mut().expect("presentation device");
                p.dirty = true;
                return Ok(());
            }
            check(rc, "vkCreateSwapchainKHR")?;
            let mut count = 0;
            let mut images = Vec::new();
            let mut rc = vkGetSwapchainImagesKHR(dev, handle, &mut count, null_mut());
            if rc == 0 {
                images = vec![0u64; count as usize];
                rc = vkGetSwapchainImagesKHR(dev, handle, &mut count, images.as_mut_ptr());
                images.truncate(count as usize);
            }
            if rc != 0 {
                vkDestroySwapchainKHR(dev, handle, null());
                return Err(format!("vkGetSwapchainImagesKHR: VkResult {rc}"));
            }
            (handle, images, extent, usage)
        };
        let mut chain = Chain {
            handle,
            images: Vec::new(),
            finished: Vec::new(),
            extent,
            requested: size,
        };
        let texture_format = surface_format(format.format).expect("picked from known formats");
        for image in images {
            let texture = Texture {
                image,
                alloc: Alloc::NONE,
                desc: TextureDesc {
                    width: extent.0,
                    height: extent.1,
                    layers: 1,
                    mips: 1,
                    format: texture_format,
                    usage,
                },
                format: format.format,
                aspect: 1,
                sw_decompress: None,
                states: vec![State::default()],
                views: Vec::new(),
                swapchain: true,
                last_use: 0,
            };
            let raw = match self.textures.insert(texture) {
                Ok(raw) => raw,
                Err(e) => return Err(self.abandon(chain, e)),
            };
            chain.images.push(TextureId::from_raw(raw));
            // SAFETY: the semaphore is owned by the chain and destroyed with it.
            match create("vkCreateSemaphore", |o| unsafe {
                vkCreateSemaphore(dev, &VkSemaphoreCreateInfo::new(), null(), o)
            }) {
                Ok(s) => chain.finished.push(s),
                Err(e) => return Err(self.abandon(chain, e)),
            }
        }
        self.present.as_mut().expect("presentation device").chain = Some(chain);
        Ok(())
    }

    /// Destroys a chain that failed to build (nothing has used it) and returns `error`.
    fn abandon(&mut self, chain: Chain, error: String) -> String {
        for id in &chain.images {
            self.textures.remove(id.raw());
        }
        // SAFETY: the swapchain was just created and none of its images were acquired.
        unsafe {
            for s in chain.finished {
                vkDestroySemaphore(self.device, s, null());
            }
            vkDestroySwapchainKHR(self.device, chain.handle, null());
        }
        self.present.as_mut().expect("presentation device").dirty = true;
        error
    }

    pub fn acquire(&mut self) -> Result<Option<TextureId>> {
        if !self.garbage.is_empty() {
            self.collect();
        }
        let p = self
            .present
            .as_mut()
            .ok_or("acquire() needs a device created with Device::for_window")?;
        if let Some((_, id)) = p.acquired {
            return Ok(Some(id));
        }
        let size = p.window.size();
        if !p.window.visible() || size.0 == 0 || size.1 == 0 {
            p.dirty = true;
            return Ok(None);
        }
        for _ in 0..2 {
            let p = self.present.as_ref().expect("presentation device");
            if p.dirty || p.chain.as_ref().is_none_or(|c| c.requested != size) {
                self.recreate(size)?;
            }
            let p = self.present.as_ref().expect("presentation device");
            let Some(chain) = p.chain.as_ref() else {
                return Ok(None);
            };
            let slot = 1 + self.frame;
            self.wait(slot)?;
            let mut index = 0;
            // SAFETY: the slot's acquire semaphore is unsignalled: its previous signal was
            // consumed by the slot's last submission, which has completed (waited above).
            let rc = unsafe {
                vkAcquireNextImageKHR(
                    self.device,
                    chain.handle,
                    ACQUIRE_TIMEOUT_NS,
                    self.subs[slot].acquire,
                    0,
                    &mut index,
                )
            };
            let id = match rc {
                0 | SUBOPTIMAL => chain.images[index as usize],
                OUT_OF_DATE => {
                    self.present.as_mut().expect("presentation device").dirty = true;
                    continue;
                }
                1 | 2 => return Ok(None), // NOT_READY, TIMEOUT
                other => return Err(format!("vkAcquireNextImageKHR: VkResult {other}")),
            };
            // The image's previous contents are discarded. Its first barrier waits for the
            // stages in which the submission waits on the acquire semaphore.
            if let Some(t) = self.textures.get_mut(id.raw()) {
                t.states[0] = State {
                    write_stages: stage::COLOR_OUTPUT | stage::TRANSFER,
                    ..State::default()
                };
            }
            let p = self.present.as_mut().expect("presentation device");
            p.dirty |= rc == SUBOPTIMAL;
            p.acquired = Some((index, id));
            return Ok(Some(id));
        }
        Ok(None)
    }

    pub fn present(&mut self, e: &Encoder) -> Result<()> {
        e.finish()?;
        let (index, id, swapchain, finished) = {
            let p = self
                .present
                .as_ref()
                .ok_or("present() needs a device created with Device::for_window")?;
            let (index, id) = p
                .acquired
                .ok_or("present() without a texture from acquire()")?;
            let chain = p.chain.as_ref().expect("an acquired image has a chain");
            (index, id, chain.handle, chain.finished[index as usize])
        };
        self.flush_transfers(false)?;
        let slot = 1 + self.frame;
        let cmd = self.begin(slot)?;
        let mut plan = core::mem::take(&mut self.plan);
        plan.reset();
        let exec_res = (|| -> Result<()> {
            self.record(cmd, e, &mut plan, Some(id))?;
            let mut scope = core::mem::take(&mut self.scope);
            scope.clear();
            let scope_res = (|| -> Result<()> {
                scope.add(
                    Key::Texture(id, 0, 0),
                    Use::new(track::PRESENT_SRC, stage::BOTTOM, 0),
                )?;
                self.apply(&mut plan, &scope)
            })();
            self.scope = scope;
            scope_res?;
            self.execute(slot, cmd, &mut plan, Some((self.subs[slot].acquire, finished)))
        })();
        self.plan = plan;
        exec_res?;
        let p = self.present.as_mut().expect("presentation device");
        p.acquired = None;
        let mut pi = VkPresentInfoKHR::new();
        pi.waitSemaphoreCount = 1;
        pi.pWaitSemaphores = &finished;
        pi.swapchainCount = 1;
        pi.pSwapchains = &swapchain;
        pi.pImageIndices = &index;
        // SAFETY: the image was acquired and its rendering submission signals `finished`.
        let rc = unsafe { vkQueuePresentKHR(self.queue, &pi) };
        match rc {
            0 => {}
            SUBOPTIMAL | OUT_OF_DATE => p.dirty = true,
            other => return Err(format!("vkQueuePresentKHR: VkResult {other}")),
        }
        self.frame = (self.frame + 1) % FRAMES;
        // Free the next slot's per-frame resources before the application writes them.
        self.wait(1 + self.frame)?;
        if !self.garbage.is_empty() {
            self.collect();
        }
        Ok(())
    }
}
