//! Instance creation (with optional Khronos validation), adapter selection and device creation.
use super::*;
use core::ffi::{CStr, c_char, c_void};
use std::io::Write as _;
use std::sync::{Arc, Mutex};

const VALIDATION_LAYER: &CStr = c"VK_LAYER_KHRONOS_validation";
const DEBUG_UTILS: &CStr = c"VK_EXT_debug_utils";
const VALIDATION_FEATURES: &CStr = c"VK_EXT_validation_features";
const SEVERITY_ERROR: u32 = 0x1000;
/// VK_VALIDATION_FEATURE_ENABLE_SYNCHRONIZATION_VALIDATION_EXT
const SYNC_VALIDATION: i32 = 4;

/// The debug messenger and the log its callback appends to.
pub(super) struct Debug {
    messenger: u64,
    destroy: Option<PFN_vkDestroyDebugUtilsMessengerEXT>,
    pub log: ValidationLog,
}
impl Debug {
    /// # Safety
    /// `instance` must be the instance the messenger was created for, and no other thread may
    /// use the messenger.
    pub unsafe fn destroy_messenger(&self, instance: Handle) {
        if let Some(destroy) = self.destroy {
            if self.messenger != 0 {
                // SAFETY: guaranteed by the caller.
                unsafe { destroy(instance, self.messenger, null()) };
            }
        }
    }
}

/// Collects validation errors. Runs on whichever thread triggered the message.
unsafe extern "system" fn on_message(
    severity: u32,
    _types: u32,
    data: *const VkDebugUtilsMessengerCallbackDataEXT,
    user: *mut c_void,
) -> u32 {
    if severity & SEVERITY_ERROR == 0 || data.is_null() || user.is_null() {
        return 0;
    }
    // SAFETY: the loader passes callback data valid for this call. `user` points into the
    // Arc'd log owned by the Gpu; the Arc outlives the instance and so every callback.
    let (data, log) = unsafe { (&*data, &*(user as *const Mutex<Vec<String>>)) };
    let text = |p: *const c_char| {
        if p.is_null() {
            String::new()
        } else {
            // SAFETY: non-null strings in the callback data are NUL-terminated.
            unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
        }
    };
    let message = format!("{}: {}", text(data.pMessageIdName), text(data.pMessage));
    let _ = writeln!(std::io::stderr(), "[vulkan validation] {message}");
    if let Ok(mut log) = log.lock() {
        log.push(message);
    }
    0
}

fn validation_requested() -> bool {
    std::env::var("GANBATE_VULKAN_VALIDATION").is_ok_and(|v| !v.is_empty() && v != "0")
}

fn names<T>(
    what: &str,
    mut list: impl FnMut(*mut u32, *mut T) -> i32,
    name: impl Fn(&T) -> String,
) -> Result<Vec<String>> {
    let mut count = 0;
    check(list(&mut count, null_mut()), what)?;
    // SAFETY: Vulkan property structs are plain C data for which zero is a valid value.
    let mut items: Vec<T> = (0..count).map(|_| unsafe { core::mem::zeroed() }).collect();
    let rc = list(&mut count, items.as_mut_ptr());
    if rc != 5 {
        // VK_INCOMPLETE only means the list grew; the returned prefix is still valid.
        check(rc, what)?;
    }
    items.truncate(count as usize);
    Ok(items.iter().map(name).collect())
}

fn c_name(raw: &[c_char]) -> String {
    let bytes: Vec<u8> = raw
        .iter()
        .take_while(|c| **c != 0)
        .map(|c| *c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

impl<'w> Gpu<'w> {
    /// A `Gpu` with an instance but no device yet. Dropping it cleans up whatever exists.
    pub(super) fn with_instance(extensions: &[&CStr]) -> Result<Self> {
        let validate = validation_requested();
        let mut enabled: Vec<*const c_char> = extensions.iter().map(|e| e.as_ptr()).collect();
        let mut layers = Vec::new();
        let mut sync_validation = false;
        let log = ValidationLog(Arc::new(Mutex::new(Vec::new())));
        if validate {
            // SAFETY: plain enumeration calls with counts and arrays sized by Vulkan.
            let available = names(
                "vkEnumerateInstanceLayerProperties",
                |n, p| unsafe { vkEnumerateInstanceLayerProperties(n, p) },
                |p: &VkLayerProperties| c_name(&p.layerName),
            )?;
            if !available
                .iter()
                .any(|l| l.as_bytes() == VALIDATION_LAYER.to_bytes())
            {
                return Err(
                    "GANBATE_VULKAN_VALIDATION is set but VK_LAYER_KHRONOS_validation is \
                            not installed"
                        .into(),
                );
            }
            let mut exts = names(
                "vkEnumerateInstanceExtensionProperties",
                |n, p| unsafe { vkEnumerateInstanceExtensionProperties(null(), n, p) },
                |p: &VkExtensionProperties| c_name(&p.extensionName),
            )?;
            exts.extend(names(
                "vkEnumerateInstanceExtensionProperties(validation layer)",
                |n, p| unsafe {
                    vkEnumerateInstanceExtensionProperties(VALIDATION_LAYER.as_ptr(), n, p)
                },
                |p: &VkExtensionProperties| c_name(&p.extensionName),
            )?);
            let has = |e: &CStr| exts.iter().any(|x| x.as_bytes() == e.to_bytes());
            if !has(DEBUG_UTILS) {
                return Err("the validation layer does not provide VK_EXT_debug_utils".into());
            }
            layers.push(VALIDATION_LAYER.as_ptr());
            enabled.push(DEBUG_UTILS.as_ptr());
            if has(VALIDATION_FEATURES) {
                enabled.push(VALIDATION_FEATURES.as_ptr());
                sync_validation = true;
            }
        }
        let features = [SYNC_VALIDATION];
        let mut sync = VkValidationFeaturesEXT::new();
        sync.enabledValidationFeatureCount = 1;
        sync.pEnabledValidationFeatures = features.as_ptr();
        let mut messenger = VkDebugUtilsMessengerCreateInfoEXT::new();
        messenger.messageSeverity = SEVERITY_ERROR;
        messenger.messageType = 0x7; // general | validation | performance
        messenger.pfnUserCallback = Some(on_message);
        messenger.pUserData = Arc::as_ptr(&log.0) as *mut c_void;
        if sync_validation {
            messenger.pNext = (&sync as *const VkValidationFeaturesEXT).cast();
        }
        let mut app = VkApplicationInfo::new();
        app.pApplicationName = c"Ganbate Engine".as_ptr();
        app.pEngineName = c"Ganbate".as_ptr();
        app.apiVersion = 1 << 22; // Vulkan 1.0
        let mut ci = VkInstanceCreateInfo::new();
        ci.pApplicationInfo = &app;
        ci.enabledLayerCount = layers.len() as u32;
        ci.ppEnabledLayerNames = if layers.is_empty() {
            null()
        } else {
            layers.as_ptr()
        };
        ci.enabledExtensionCount = enabled.len() as u32;
        ci.ppEnabledExtensionNames = if enabled.is_empty() {
            null()
        } else {
            enabled.as_ptr()
        };
        if validate {
            // Also reports problems inside vkCreateInstance/vkDestroyInstance.
            ci.pNext = (&messenger as *const VkDebugUtilsMessengerCreateInfoEXT).cast();
        }
        // SAFETY: every pointer in the create-info chain refers to locals that outlive the call.
        let instance = create("vkCreateInstance", |o| unsafe {
            vkCreateInstance(&ci, null(), o)
        })?;
        let mut gpu = Self::empty(instance);
        if validate {
            let mut debug = Debug {
                messenger: 0,
                destroy: None,
                log,
            };
            // SAFETY: the names are valid; the returned pointers are the documented
            // signatures of these VK_EXT_debug_utils entry points.
            unsafe {
                let create_fn =
                    vkGetInstanceProcAddr(instance, c"vkCreateDebugUtilsMessengerEXT".as_ptr());
                let destroy_fn =
                    vkGetInstanceProcAddr(instance, c"vkDestroyDebugUtilsMessengerEXT".as_ptr());
                if let (Some(c), Some(d)) = (create_fn, destroy_fn) {
                    let c: PFN_vkCreateDebugUtilsMessengerEXT = core::mem::transmute(c);
                    debug.destroy = Some(core::mem::transmute::<
                        PFN_vkVoidFunction,
                        PFN_vkDestroyDebugUtilsMessengerEXT,
                    >(d));
                    messenger.pNext = null();
                    debug.messenger = create("vkCreateDebugUtilsMessengerEXT", |o| {
                        c(instance, &messenger, null(), o)
                    })?;
                } else {
                    return Err("VK_EXT_debug_utils entry points are missing".into());
                }
            }
            gpu.debug = Some(debug);
        }
        Ok(gpu)
    }

    fn empty(instance: Handle) -> Self {
        Self {
            instance,
            physical: null_mut(),
            device: null_mut(),
            queue: null_mut(),
            family: 0,
            // SAFETY: plain C data; filled in by open_device.
            memory: unsafe { core::mem::zeroed() },
            limits: unsafe { core::mem::zeroed() },
            features: Features::default(),
            name: String::new(),
            debug: None,
            slabs: Vec::new(),
            buffers: Pool::default(),
            textures: Pool::default(),
            samplers: Pool::default(),
            pipelines: Pool::default(),
            groups: Pool::default(),
            sampler_cache: HashMap::new(),
            set_layouts: HashMap::new(),
            layout_ids: HashMap::new(),
            pipeline_layouts: HashMap::new(),
            pipeline_cache: 0,
            render_passes: HashMap::new(),
            framebuffers: HashMap::new(),
            descriptor_pools: Vec::new(),
            command_pool: 0,
            subs: Default::default(),
            frame: 0,
            serial: 0,
            garbage: Vec::new(),
            staging: Vec::new(),
            xfer_slot: 0,
            xfer_recording: false,
            xfer_plan: Default::default(),
            plan: Default::default(),
            scope: Default::default(),
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            present: None,
            _window: core::marker::PhantomData,
        }
    }

    /// Picks the first adapter (filtered by `GANBATE_VULKAN_DEVICE`) with a graphics+compute queue
    /// that can present to `surface` (if non-zero) and render and sample `Depth32Float`.
    pub(super) fn open_device(&mut self, surface: u64) -> Result<()> {
        let pick = std::env::var("GANBATE_VULKAN_DEVICE")
            .unwrap_or_default()
            .to_lowercase();
        let mut last_error = if pick.is_empty() {
            "no Vulkan device with a graphics+compute queue and sampled Depth32Float attachments"
                .to_string()
        } else {
            format!("no compatible Vulkan device matches {pick}")
        };
        // SAFETY: counts come from Vulkan and output arrays are allocated to those counts; every
        // create-info pointer refers to locals that outlive the call.
        unsafe {
            let mut count = 0;
            check(
                vkEnumeratePhysicalDevices(self.instance, &mut count, null_mut()),
                "count devices",
            )?;
            let mut physical = vec![null_mut(); count as usize];
            if count > 0 {
                let rc =
                    vkEnumeratePhysicalDevices(self.instance, &mut count, physical.as_mut_ptr());
                if rc != 5 {
                    check(rc, "enumerate devices")?;
                }
            }
            physical.truncate(count as usize);
            if pick.is_empty() {
                // Discrete, then integrated, virtual, other and CPU devices.
                let rank = |gpu: &Handle| {
                    let mut props: VkPhysicalDeviceProperties = core::mem::zeroed();
                    vkGetPhysicalDeviceProperties(*gpu, &mut props);
                    match props.deviceType {
                        2 => 0,
                        1 => 1,
                        3 => 2,
                        4 => 4,
                        _ => 3,
                    }
                };
                physical.sort_by_key(rank);
            }
            for gpu in physical {
                let mut props: VkPhysicalDeviceProperties = core::mem::zeroed();
                vkGetPhysicalDeviceProperties(gpu, &mut props);
                let name = c_name(&props.deviceName);
                if !name.to_lowercase().contains(&pick) {
                    continue;
                }
                let mut n = 0;
                vkGetPhysicalDeviceQueueFamilyProperties(gpu, &mut n, null_mut());
                let mut families = vec![core::mem::zeroed::<VkQueueFamilyProperties>(); n as usize];
                vkGetPhysicalDeviceQueueFamilyProperties(gpu, &mut n, families.as_mut_ptr());
                families.truncate(n as usize);
                let family = families.iter().enumerate().position(|(i, q)| {
                    if q.queueCount == 0 || q.queueFlags & 3 != 3 {
                        return false;
                    }
                    let mut ok = 0;
                    surface == 0
                        || (vkGetPhysicalDeviceSurfaceSupportKHR(gpu, i as u32, surface, &mut ok)
                            == 0
                            && ok != 0)
                });
                let Some(family) = family else {
                    last_error = format!("{name}: no graphics+compute queue for this surface");
                    continue;
                };
                let mut depth: VkFormatProperties = core::mem::zeroed();
                vkGetPhysicalDeviceFormatProperties(gpu, 126, &mut depth);
                if depth.optimalTilingFeatures & 0x201 != 0x201 {
                    last_error = format!("{name}: Depth32Float cannot be a sampled attachment");
                    continue;
                }
                let mut supported: VkPhysicalDeviceFeatures = core::mem::zeroed();
                vkGetPhysicalDeviceFeatures(gpu, &mut supported);
                // Optional features the API can use; each is enabled only when supported.
                let mut features: VkPhysicalDeviceFeatures = core::mem::zeroed();
                features.samplerAnisotropy = supported.samplerAnisotropy;
                features.depthClamp = supported.depthClamp;
                features.independentBlend = supported.independentBlend;
                features.vertexPipelineStoresAndAtomics = supported.vertexPipelineStoresAndAtomics;
                features.fragmentStoresAndAtomics = supported.fragmentStoresAndAtomics;
                features.shaderStorageImageExtendedFormats =
                    supported.shaderStorageImageExtendedFormats;
                features.textureCompressionBC = supported.textureCompressionBC;
                features.textureCompressionASTC_LDR = supported.textureCompressionASTC_LDR;
                let priority = 1.0f32;
                let mut q = VkDeviceQueueCreateInfo::new();
                q.queueFamilyIndex = family as u32;
                q.queueCount = 1;
                q.pQueuePriorities = &priority;
                let swapchain = c"VK_KHR_swapchain".as_ptr();
                let mut d = VkDeviceCreateInfo::new();
                d.queueCreateInfoCount = 1;
                d.pQueueCreateInfos = &q;
                d.pEnabledFeatures = (&features as *const VkPhysicalDeviceFeatures).cast();
                if surface != 0 {
                    d.enabledExtensionCount = 1;
                    d.ppEnabledExtensionNames = &swapchain;
                }
                match create("vkCreateDevice", |o| vkCreateDevice(gpu, &d, null(), o)) {
                    Ok(device) => self.device = device,
                    Err(e) => {
                        last_error = format!("{name}: {e}");
                        continue;
                    }
                }
                self.physical = gpu;
                self.family = family as u32;
                vkGetDeviceQueue(self.device, self.family, 0, &mut self.queue);
                vkGetPhysicalDeviceMemoryProperties(gpu, &mut self.memory);
                self.limits = props.limits;
                self.features = Features {
                    anisotropy: features.samplerAnisotropy != 0,
                    depth_clamp: features.depthClamp != 0,
                    independent_blend: features.independentBlend != 0,
                    vertex_stores: features.vertexPipelineStoresAndAtomics != 0,
                    fragment_stores: features.fragmentStoresAndAtomics != 0,
                    storage_formats: features.shaderStorageImageExtendedFormats != 0,
                    bc_compression: features.textureCompressionBC != 0,
                    astc_compression: features.textureCompressionASTC_LDR != 0,
                };
                self.name = name;
                return self.create_submissions();
            }
        }
        Err(last_error)
    }

    fn create_submissions(&mut self) -> Result<()> {
        let dev = self.device;
        let mut pool = VkCommandPoolCreateInfo::new();
        pool.queueFamilyIndex = self.family;
        pool.flags = 2; // RESET_COMMAND_BUFFER
        // SAFETY: create-info structs outlive each call; the handles are owned by self and
        // destroyed in Drop.
        unsafe {
            let _ = vkCreatePipelineCache(
                dev,
                &VkPipelineCacheCreateInfo::new(),
                null(),
                &mut self.pipeline_cache,
            );
            self.command_pool = create("vkCreateCommandPool", |o| {
                vkCreateCommandPool(dev, &pool, null(), o)
            })?;
            let mut ai = VkCommandBufferAllocateInfo::new();
            ai.commandPool = self.command_pool;
            ai.commandBufferCount = 1;
            for which in 0..self.subs.len() {
                let cmd = create("vkAllocateCommandBuffers", |o| {
                    vkAllocateCommandBuffers(dev, &ai, o)
                })?;
                let fence = create("vkCreateFence", |o| {
                    vkCreateFence(dev, &VkFenceCreateInfo::new(), null(), o)
                })?;
                let sub = &mut self.subs[which];
                sub.cmd = cmd;
                sub.fence = fence;
                if (1..=FRAMES).contains(&which) {
                    sub.acquire = create("vkCreateSemaphore", |o| {
                        vkCreateSemaphore(dev, &VkSemaphoreCreateInfo::new(), null(), o)
                    })?;
                }
            }
        }
        Ok(())
    }
}
