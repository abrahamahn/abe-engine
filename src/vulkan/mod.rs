//! Direct Vulkan 1.0 graphics. Only the system Vulkan loader is linked.
pub mod bench;
mod ffi;
mod frame;
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub mod present;
mod resident;
mod shaders;
use crate::Vertex;
use core::ffi::{CStr, c_void};
use core::ptr::{null, null_mut};
use ffi::*;

type Handle = *mut c_void;
pub type Result<T> = std::result::Result<T, String>;

fn check(code: i32, operation: &str) -> Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(format!("{operation}: VkResult {code}"))
    }
}
fn create<T: Default>(operation: &str, call: impl FnOnce(&mut T) -> i32) -> Result<T> {
    let mut value = T::default();
    check(call(&mut value), operation)?;
    Ok(value)
}

pub struct VulkanRenderer {
    instance: Handle,
    device: Handle,
    physical: Handle,
    surface: u64,
    queue: Handle,
    family: u32,
    memory: VkPhysicalDeviceMemoryProperties,
    depth_format: i32,
    name: String,
    max_extent: u32,
    timestamp_bits: u32,
    timestamp_period_ns: f64,
}
impl VulkanRenderer {
    pub fn new() -> Result<Self> {
        let mut ctx = Self::instance(&[])?;
        ctx.select_device()?;
        Ok(ctx)
    }
    fn instance(extensions: &[*const core::ffi::c_char]) -> Result<Self> {
        let mut app = VkApplicationInfo::new();
        app.pApplicationName = c"ABE Engine".as_ptr();
        app.apiVersion = 1 << 22; // Vulkan 1.0
        let mut ci = VkInstanceCreateInfo::new();
        ci.pApplicationInfo = &app;
        ci.enabledExtensionCount = extensions.len() as u32;
        ci.ppEnabledExtensionNames = if extensions.is_empty() {
            null()
        } else {
            extensions.as_ptr()
        };
        // SAFETY: all Vulkan pointers in the create structures live through each call.
        let instance = create("vkCreateInstance", |o| unsafe {
            vkCreateInstance(&ci, null(), o)
        })?;
        Ok(Self {
            instance,
            device: null_mut(),
            physical: null_mut(),
            surface: 0,
            queue: null_mut(),
            family: 0,
            memory: unsafe { core::mem::zeroed() },
            depth_format: 0,
            name: String::new(),
            max_extent: 0,
            timestamp_bits: 0,
            timestamp_period_ns: 0.0,
        })
    }
    fn select_device(&mut self) -> Result<()> {
        // SAFETY: all counts are obtained from Vulkan, and output arrays are allocated to
        // those counts. Enumeration may return INCOMPLETE if the device list changes.
        unsafe {
            let mut count = 0;
            check(
                vkEnumeratePhysicalDevices(self.instance, &mut count, null_mut()),
                "count devices",
            )?;
            let mut physical = vec![null_mut(); count as usize];
            if count > 0 {
                check(
                    vkEnumeratePhysicalDevices(self.instance, &mut count, physical.as_mut_ptr()),
                    "enumerate devices",
                )?;
            }
            physical.truncate(count as usize);
            let mut last_error =
                String::from("no graphics-capable Vulkan device with a supported depth attachment");
            let pick = std::env::var("ABE_VULKAN_DEVICE")
                .unwrap_or_default()
                .to_lowercase();
            if !pick.is_empty() {
                last_error = format!("no compatible Vulkan device matches {pick}");
            }
            for gpu in physical {
                let mut props = core::mem::zeroed();
                vkGetPhysicalDeviceProperties(gpu, &mut props);
                let name = CStr::from_ptr(props.deviceName.as_ptr()).to_string_lossy();
                if !name.to_lowercase().contains(&pick) {
                    continue;
                }
                let mut count = 0;
                vkGetPhysicalDeviceQueueFamilyProperties(gpu, &mut count, null_mut());
                let mut families =
                    vec![core::mem::zeroed::<VkQueueFamilyProperties>(); count as usize];
                vkGetPhysicalDeviceQueueFamilyProperties(gpu, &mut count, families.as_mut_ptr());
                let Some(family) =
                    families
                        .iter()
                        .take(count as usize)
                        .enumerate()
                        .find_map(|(i, q)| {
                            if q.queueCount == 0 || q.queueFlags & 1 == 0 {
                                return None;
                            }
                            if self.surface != 0 {
                                let mut ok = 0;
                                if vkGetPhysicalDeviceSurfaceSupportKHR(
                                    gpu,
                                    i as u32,
                                    self.surface,
                                    &mut ok,
                                ) != 0
                                    || ok == 0
                                {
                                    return None;
                                }
                            }
                            Some(i)
                        })
                else {
                    continue;
                };
                let Some(depth_format) = [126, 124].into_iter().find(|fmt| {
                    let mut props = core::mem::zeroed();
                    vkGetPhysicalDeviceFormatProperties(gpu, *fmt, &mut props);
                    props.optimalTilingFeatures & 0x200 != 0
                }) else {
                    continue;
                };
                let mut q = VkDeviceQueueCreateInfo::new();
                let priority = 1.0f32;
                q.queueCount = 1;
                q.queueFamilyIndex = family as u32;
                q.pQueuePriorities = &priority;
                let mut d = VkDeviceCreateInfo::new();
                d.queueCreateInfoCount = 1;
                d.pQueueCreateInfos = &q;
                let swapchain_ext = c"VK_KHR_swapchain".as_ptr();
                if self.surface != 0 {
                    d.enabledExtensionCount = 1;
                    d.ppEnabledExtensionNames = &swapchain_ext;
                }
                match create("vkCreateDevice", |o| vkCreateDevice(gpu, &d, null(), o)) {
                    Ok(device) => self.device = device,
                    Err(err) => {
                        last_error = err;
                        continue;
                    }
                }
                self.physical = gpu;
                self.family = family as u32;
                self.depth_format = depth_format;
                vkGetDeviceQueue(self.device, self.family, 0, &mut self.queue);
                vkGetPhysicalDeviceMemoryProperties(gpu, &mut self.memory);
                self.timestamp_bits = families[family].timestampValidBits;
                self.timestamp_period_ns = f64::from(props.limits.timestampPeriod);
                self.max_extent = props
                    .limits
                    .maxImageDimension2D
                    .min(props.limits.maxFramebufferWidth)
                    .min(props.limits.maxFramebufferHeight);
                self.name = CStr::from_ptr(props.deviceName.as_ptr())
                    .to_string_lossy()
                    .into_owned();
                return Ok(());
            }
            Err(last_error)
        }
    }
    pub fn device_name(&self) -> &str {
        &self.name
    }
    /// Render an ordered triangle list to top-down RGBA8 pixels. The hardware depth test
    /// resolves occlusion. All rendering completes before this method returns.
    pub fn render(
        &mut self,
        width: u32,
        height: u32,
        clear: [f32; 4],
        vertices: &[Vertex],
    ) -> Result<Vec<u8>> {
        self.check_extent(width, height)?;
        if vertices.len() % 3 != 0 || vertices.len() > u32::MAX as usize {
            return Err("expected a triangle list representable by one Vulkan draw".into());
        }
        frame::Frame::new(self).draw(width, height, clear, vertices, None)
    }
    /// Render world-space geometry using the same GPU vertex transform as the player.
    /// This is a synchronous readback oracle for content tools and regression tests.
    pub fn render_scene(
        &mut self,
        width: u32,
        height: u32,
        clear: [f32; 4],
        scene: &crate::Scene,
        camera: crate::Camera,
    ) -> Result<Vec<u8>> {
        self.check_extent(width, height)?;
        let matrix = camera.clip_matrix(width, height)?;
        let resident = resident::Resident::upload(self, scene)?;
        frame::Frame::new(self).draw(width, height, clear, &[], Some(resident.draw(matrix)))
    }
    fn check_extent(&self, width: u32, height: u32) -> Result<()> {
        let pixels = u64::from(width) * u64::from(height);
        if width == 0
            || height == 0
            || width > self.max_extent
            || height > self.max_extent
            || pixels > 16_777_216
        {
            return Err(
                "invalid dimensions (maximum 16 megapixels and the device's attachment limit)"
                    .into(),
            );
        }
        Ok(())
    }
}
impl Drop for VulkanRenderer {
    fn drop(&mut self) {
        unsafe {
            if !self.device.is_null() {
                vkDeviceWaitIdle(self.device);
                vkDestroyDevice(self.device, null());
            }
            if self.surface != 0 {
                vkDestroySurfaceKHR(self.instance, self.surface, null());
            }
            vkDestroyInstance(self.instance, null());
        }
    }
}
