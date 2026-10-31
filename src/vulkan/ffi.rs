// Vulkan registry declarations: Copyright 2015-2024 The Khronos Group Inc.
// Adapted from Vulkan-Headers v1.3.275 under Apache-2.0. See docs/Vulkan-Registry-LICENSE.txt.
//! Minimal Vulkan 1.0 C ABI declarations, derived from Khronos Vulkan-Headers v1.3.275.
#![allow(non_snake_case, non_camel_case_types, dead_code)]
use core::ffi::{c_char, c_void};

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkXlibSurfaceCreateInfoKHR {
    pub sType: i32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub dpy: *mut c_void,
    pub window: core::ffi::c_ulong,
}
impl VkXlibSurfaceCreateInfoKHR {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 1000004000,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkWin32SurfaceCreateInfoKHR {
    pub sType: i32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub hinstance: *mut c_void,
    pub hwnd: *mut c_void,
}
impl VkWin32SurfaceCreateInfoKHR {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 1000009000,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkExtent2D {
    pub width: u32,
    pub height: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkSurfaceCapabilitiesKHR {
    pub minImageCount: u32,
    pub maxImageCount: u32,
    pub currentExtent: VkExtent2D,
    pub minImageExtent: VkExtent2D,
    pub maxImageExtent: VkExtent2D,
    pub maxImageArrayLayers: u32,
    pub supportedTransforms: u32,
    pub currentTransform: i32,
    pub supportedCompositeAlpha: u32,
    pub supportedUsageFlags: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkSurfaceFormatKHR {
    pub format: i32,
    pub colorSpace: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkSwapchainCreateInfoKHR {
    pub sType: i32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub surface: u64,
    pub minImageCount: u32,
    pub imageFormat: i32,
    pub imageColorSpace: i32,
    pub imageExtent: VkExtent2D,
    pub imageArrayLayers: u32,
    pub imageUsage: u32,
    pub imageSharingMode: i32,
    pub queueFamilyIndexCount: u32,
    pub pQueueFamilyIndices: *const u32,
    pub preTransform: i32,
    pub compositeAlpha: i32,
    pub presentMode: i32,
    pub clipped: u32,
    pub oldSwapchain: u64,
}
impl VkSwapchainCreateInfoKHR {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 1000001000,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkPresentInfoKHR {
    pub sType: i32,
    pub pNext: *const c_void,
    pub waitSemaphoreCount: u32,
    pub pWaitSemaphores: *const u64,
    pub swapchainCount: u32,
    pub pSwapchains: *const u64,
    pub pImageIndices: *const u32,
    pub pResults: *mut i32,
}
impl VkPresentInfoKHR {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 1000001001,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkFenceCreateInfo {
    pub sType: i32,
    pub pNext: *const c_void,
    pub flags: u32,
}
impl VkFenceCreateInfo {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 8,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkSemaphoreCreateInfo {
    pub sType: i32,
    pub pNext: *const c_void,
    pub flags: u32,
}
impl VkSemaphoreCreateInfo {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 9,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkApplicationInfo {
    pub sType: i32,
    pub pNext: *const c_void,
    pub pApplicationName: *const c_char,
    pub applicationVersion: u32,
    pub pEngineName: *const c_char,
    pub engineVersion: u32,
    pub apiVersion: u32,
}
impl VkApplicationInfo {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 0,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkInstanceCreateInfo {
    pub sType: i32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub pApplicationInfo: *const VkApplicationInfo,
    pub enabledLayerCount: u32,
    pub ppEnabledLayerNames: *const *const c_char,
    pub enabledExtensionCount: u32,
    pub ppEnabledExtensionNames: *const *const c_char,
}
impl VkInstanceCreateInfo {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 1,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkExtent3D {
    pub width: u32,
    pub height: u32,
    pub depth: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkQueueFamilyProperties {
    pub queueFlags: u32,
    pub queueCount: u32,
    pub timestampValidBits: u32,
    pub minImageTransferGranularity: VkExtent3D,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkMemoryType {
    pub propertyFlags: u32,
    pub heapIndex: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkMemoryHeap {
    pub size: u64,
    pub flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkPhysicalDeviceMemoryProperties {
    pub memoryTypeCount: u32,
    pub memoryTypes: [VkMemoryType; 32],
    pub memoryHeapCount: u32,
    pub memoryHeaps: [VkMemoryHeap; 16],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkPhysicalDeviceLimits {
    pub maxImageDimension1D: u32,
    pub maxImageDimension2D: u32,
    pub maxImageDimension3D: u32,
    pub maxImageDimensionCube: u32,
    pub maxImageArrayLayers: u32,
    pub maxTexelBufferElements: u32,
    pub maxUniformBufferRange: u32,
    pub maxStorageBufferRange: u32,
    pub maxPushConstantsSize: u32,
    pub maxMemoryAllocationCount: u32,
    pub maxSamplerAllocationCount: u32,
    pub bufferImageGranularity: u64,
    pub sparseAddressSpaceSize: u64,
    pub maxBoundDescriptorSets: u32,
    pub maxPerStageDescriptorSamplers: u32,
    pub maxPerStageDescriptorUniformBuffers: u32,
    pub maxPerStageDescriptorStorageBuffers: u32,
    pub maxPerStageDescriptorSampledImages: u32,
    pub maxPerStageDescriptorStorageImages: u32,
    pub maxPerStageDescriptorInputAttachments: u32,
    pub maxPerStageResources: u32,
    pub maxDescriptorSetSamplers: u32,
    pub maxDescriptorSetUniformBuffers: u32,
    pub maxDescriptorSetUniformBuffersDynamic: u32,
    pub maxDescriptorSetStorageBuffers: u32,
    pub maxDescriptorSetStorageBuffersDynamic: u32,
    pub maxDescriptorSetSampledImages: u32,
    pub maxDescriptorSetStorageImages: u32,
    pub maxDescriptorSetInputAttachments: u32,
    pub maxVertexInputAttributes: u32,
    pub maxVertexInputBindings: u32,
    pub maxVertexInputAttributeOffset: u32,
    pub maxVertexInputBindingStride: u32,
    pub maxVertexOutputComponents: u32,
    pub maxTessellationGenerationLevel: u32,
    pub maxTessellationPatchSize: u32,
    pub maxTessellationControlPerVertexInputComponents: u32,
    pub maxTessellationControlPerVertexOutputComponents: u32,
    pub maxTessellationControlPerPatchOutputComponents: u32,
    pub maxTessellationControlTotalOutputComponents: u32,
    pub maxTessellationEvaluationInputComponents: u32,
    pub maxTessellationEvaluationOutputComponents: u32,
    pub maxGeometryShaderInvocations: u32,
    pub maxGeometryInputComponents: u32,
    pub maxGeometryOutputComponents: u32,
    pub maxGeometryOutputVertices: u32,
    pub maxGeometryTotalOutputComponents: u32,
    pub maxFragmentInputComponents: u32,
    pub maxFragmentOutputAttachments: u32,
    pub maxFragmentDualSrcAttachments: u32,
    pub maxFragmentCombinedOutputResources: u32,
    pub maxComputeSharedMemorySize: u32,
    pub maxComputeWorkGroupCount: [u32; 3],
    pub maxComputeWorkGroupInvocations: u32,
    pub maxComputeWorkGroupSize: [u32; 3],
    pub subPixelPrecisionBits: u32,
    pub subTexelPrecisionBits: u32,
    pub mipmapPrecisionBits: u32,
    pub maxDrawIndexedIndexValue: u32,
    pub maxDrawIndirectCount: u32,
    pub maxSamplerLodBias: f32,
    pub maxSamplerAnisotropy: f32,
    pub maxViewports: u32,
    pub maxViewportDimensions: [u32; 2],
    pub viewportBoundsRange: [f32; 2],
    pub viewportSubPixelBits: u32,
    pub minMemoryMapAlignment: usize,
    pub minTexelBufferOffsetAlignment: u64,
    pub minUniformBufferOffsetAlignment: u64,
    pub minStorageBufferOffsetAlignment: u64,
    pub minTexelOffset: i32,
    pub maxTexelOffset: u32,
    pub minTexelGatherOffset: i32,
    pub maxTexelGatherOffset: u32,
    pub minInterpolationOffset: f32,
    pub maxInterpolationOffset: f32,
    pub subPixelInterpolationOffsetBits: u32,
    pub maxFramebufferWidth: u32,
    pub maxFramebufferHeight: u32,
    pub maxFramebufferLayers: u32,
    pub framebufferColorSampleCounts: u32,
    pub framebufferDepthSampleCounts: u32,
    pub framebufferStencilSampleCounts: u32,
    pub framebufferNoAttachmentsSampleCounts: u32,
    pub maxColorAttachments: u32,
    pub sampledImageColorSampleCounts: u32,
    pub sampledImageIntegerSampleCounts: u32,
    pub sampledImageDepthSampleCounts: u32,
    pub sampledImageStencilSampleCounts: u32,
    pub storageImageSampleCounts: u32,
    pub maxSampleMaskWords: u32,
    pub timestampComputeAndGraphics: u32,
    pub timestampPeriod: f32,
    pub maxClipDistances: u32,
    pub maxCullDistances: u32,
    pub maxCombinedClipAndCullDistances: u32,
    pub discreteQueuePriorities: u32,
    pub pointSizeRange: [f32; 2],
    pub lineWidthRange: [f32; 2],
    pub pointSizeGranularity: f32,
    pub lineWidthGranularity: f32,
    pub strictLines: u32,
    pub standardSampleLocations: u32,
    pub optimalBufferCopyOffsetAlignment: u64,
    pub optimalBufferCopyRowPitchAlignment: u64,
    pub nonCoherentAtomSize: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkPhysicalDeviceSparseProperties {
    pub residencyStandard2DBlockShape: u32,
    pub residencyStandard2DMultisampleBlockShape: u32,
    pub residencyStandard3DBlockShape: u32,
    pub residencyAlignedMipSize: u32,
    pub residencyNonResidentStrict: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkPhysicalDeviceProperties {
    pub apiVersion: u32,
    pub driverVersion: u32,
    pub vendorID: u32,
    pub deviceID: u32,
    pub deviceType: i32,
    pub deviceName: [c_char; 256],
    pub pipelineCacheUUID: [u8; 16],
    pub limits: VkPhysicalDeviceLimits,
    pub sparseProperties: VkPhysicalDeviceSparseProperties,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkFormatProperties {
    pub linearTilingFeatures: u32,
    pub optimalTilingFeatures: u32,
    pub bufferFeatures: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkDeviceQueueCreateInfo {
    pub sType: i32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub queueFamilyIndex: u32,
    pub queueCount: u32,
    pub pQueuePriorities: *const f32,
}
impl VkDeviceQueueCreateInfo {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 2,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkDeviceCreateInfo {
    pub sType: i32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub queueCreateInfoCount: u32,
    pub pQueueCreateInfos: *const VkDeviceQueueCreateInfo,
    pub enabledLayerCount: u32,
    pub ppEnabledLayerNames: *const *const c_char,
    pub enabledExtensionCount: u32,
    pub ppEnabledExtensionNames: *const *const c_char,
    pub pEnabledFeatures: *const c_void,
}
impl VkDeviceCreateInfo {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 3,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkSubmitInfo {
    pub sType: i32,
    pub pNext: *const c_void,
    pub waitSemaphoreCount: u32,
    pub pWaitSemaphores: *const u64,
    pub pWaitDstStageMask: *const u32,
    pub commandBufferCount: u32,
    pub pCommandBuffers: *const *mut c_void,
    pub signalSemaphoreCount: u32,
    pub pSignalSemaphores: *const u64,
}
impl VkSubmitInfo {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 4,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkBufferCreateInfo {
    pub sType: i32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub size: u64,
    pub usage: u32,
    pub sharingMode: i32,
    pub queueFamilyIndexCount: u32,
    pub pQueueFamilyIndices: *const u32,
}
impl VkBufferCreateInfo {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 12,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkMemoryRequirements {
    pub size: u64,
    pub alignment: u64,
    pub memoryTypeBits: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkImageCreateInfo {
    pub sType: i32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub imageType: i32,
    pub format: i32,
    pub extent: VkExtent3D,
    pub mipLevels: u32,
    pub arrayLayers: u32,
    pub samples: i32,
    pub tiling: i32,
    pub usage: u32,
    pub sharingMode: i32,
    pub queueFamilyIndexCount: u32,
    pub pQueueFamilyIndices: *const u32,
    pub initialLayout: i32,
}
impl VkImageCreateInfo {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 14,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkMemoryAllocateInfo {
    pub sType: i32,
    pub pNext: *const c_void,
    pub allocationSize: u64,
    pub memoryTypeIndex: u32,
}
impl VkMemoryAllocateInfo {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 5,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkComponentMapping {
    pub r: i32,
    pub g: i32,
    pub b: i32,
    pub a: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkImageSubresourceRange {
    pub aspectMask: u32,
    pub baseMipLevel: u32,
    pub levelCount: u32,
    pub baseArrayLayer: u32,
    pub layerCount: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkImageViewCreateInfo {
    pub sType: i32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub image: u64,
    pub viewType: i32,
    pub format: i32,
    pub components: VkComponentMapping,
    pub subresourceRange: VkImageSubresourceRange,
}
impl VkImageViewCreateInfo {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 15,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkAttachmentDescription {
    pub flags: u32,
    pub format: i32,
    pub samples: i32,
    pub loadOp: i32,
    pub storeOp: i32,
    pub stencilLoadOp: i32,
    pub stencilStoreOp: i32,
    pub initialLayout: i32,
    pub finalLayout: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkAttachmentReference {
    pub attachment: u32,
    pub layout: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkSubpassDescription {
    pub flags: u32,
    pub pipelineBindPoint: i32,
    pub inputAttachmentCount: u32,
    pub pInputAttachments: *const VkAttachmentReference,
    pub colorAttachmentCount: u32,
    pub pColorAttachments: *const VkAttachmentReference,
    pub pResolveAttachments: *const VkAttachmentReference,
    pub pDepthStencilAttachment: *const VkAttachmentReference,
    pub preserveAttachmentCount: u32,
    pub pPreserveAttachments: *const u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkSubpassDependency {
    pub srcSubpass: u32,
    pub dstSubpass: u32,
    pub srcStageMask: u32,
    pub dstStageMask: u32,
    pub srcAccessMask: u32,
    pub dstAccessMask: u32,
    pub dependencyFlags: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkRenderPassCreateInfo {
    pub sType: i32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub attachmentCount: u32,
    pub pAttachments: *const VkAttachmentDescription,
    pub subpassCount: u32,
    pub pSubpasses: *const VkSubpassDescription,
    pub dependencyCount: u32,
    pub pDependencies: *const VkSubpassDependency,
}
impl VkRenderPassCreateInfo {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 38,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkFramebufferCreateInfo {
    pub sType: i32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub renderPass: u64,
    pub attachmentCount: u32,
    pub pAttachments: *const u64,
    pub width: u32,
    pub height: u32,
    pub layers: u32,
}
impl VkFramebufferCreateInfo {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 37,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkShaderModuleCreateInfo {
    pub sType: i32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub codeSize: usize,
    pub pCode: *const u32,
}
impl VkShaderModuleCreateInfo {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 16,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkPipelineLayoutCreateInfo {
    pub sType: i32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub setLayoutCount: u32,
    pub pSetLayouts: *const u64,
    pub pushConstantRangeCount: u32,
    pub pPushConstantRanges: *const c_void,
}
impl VkPipelineLayoutCreateInfo {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 30,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkPipelineShaderStageCreateInfo {
    pub sType: i32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub stage: i32,
    pub module: u64,
    pub pName: *const c_char,
    pub pSpecializationInfo: *const c_void,
}
impl VkPipelineShaderStageCreateInfo {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 18,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkVertexInputBindingDescription {
    pub binding: u32,
    pub stride: u32,
    pub inputRate: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkVertexInputAttributeDescription {
    pub location: u32,
    pub binding: u32,
    pub format: i32,
    pub offset: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkPipelineVertexInputStateCreateInfo {
    pub sType: i32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub vertexBindingDescriptionCount: u32,
    pub pVertexBindingDescriptions: *const VkVertexInputBindingDescription,
    pub vertexAttributeDescriptionCount: u32,
    pub pVertexAttributeDescriptions: *const VkVertexInputAttributeDescription,
}
impl VkPipelineVertexInputStateCreateInfo {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 19,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkPipelineInputAssemblyStateCreateInfo {
    pub sType: i32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub topology: i32,
    pub primitiveRestartEnable: u32,
}
impl VkPipelineInputAssemblyStateCreateInfo {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 20,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkViewport {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub minDepth: f32,
    pub maxDepth: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkOffset2D {
    pub x: i32,
    pub y: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkRect2D {
    pub offset: VkOffset2D,
    pub extent: VkExtent2D,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkPipelineViewportStateCreateInfo {
    pub sType: i32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub viewportCount: u32,
    pub pViewports: *const VkViewport,
    pub scissorCount: u32,
    pub pScissors: *const VkRect2D,
}
impl VkPipelineViewportStateCreateInfo {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 22,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkPipelineRasterizationStateCreateInfo {
    pub sType: i32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub depthClampEnable: u32,
    pub rasterizerDiscardEnable: u32,
    pub polygonMode: i32,
    pub cullMode: u32,
    pub frontFace: i32,
    pub depthBiasEnable: u32,
    pub depthBiasConstantFactor: f32,
    pub depthBiasClamp: f32,
    pub depthBiasSlopeFactor: f32,
    pub lineWidth: f32,
}
impl VkPipelineRasterizationStateCreateInfo {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 23,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkPipelineMultisampleStateCreateInfo {
    pub sType: i32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub rasterizationSamples: i32,
    pub sampleShadingEnable: u32,
    pub minSampleShading: f32,
    pub pSampleMask: *const u32,
    pub alphaToCoverageEnable: u32,
    pub alphaToOneEnable: u32,
}
impl VkPipelineMultisampleStateCreateInfo {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 24,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkStencilOpState {
    pub failOp: i32,
    pub passOp: i32,
    pub depthFailOp: i32,
    pub compareOp: i32,
    pub compareMask: u32,
    pub writeMask: u32,
    pub reference: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkPipelineDepthStencilStateCreateInfo {
    pub sType: i32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub depthTestEnable: u32,
    pub depthWriteEnable: u32,
    pub depthCompareOp: i32,
    pub depthBoundsTestEnable: u32,
    pub stencilTestEnable: u32,
    pub front: VkStencilOpState,
    pub back: VkStencilOpState,
    pub minDepthBounds: f32,
    pub maxDepthBounds: f32,
}
impl VkPipelineDepthStencilStateCreateInfo {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 25,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkPipelineColorBlendAttachmentState {
    pub blendEnable: u32,
    pub srcColorBlendFactor: i32,
    pub dstColorBlendFactor: i32,
    pub colorBlendOp: i32,
    pub srcAlphaBlendFactor: i32,
    pub dstAlphaBlendFactor: i32,
    pub alphaBlendOp: i32,
    pub colorWriteMask: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkPipelineColorBlendStateCreateInfo {
    pub sType: i32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub logicOpEnable: u32,
    pub logicOp: i32,
    pub attachmentCount: u32,
    pub pAttachments: *const VkPipelineColorBlendAttachmentState,
    pub blendConstants: [f32; 4],
}
impl VkPipelineColorBlendStateCreateInfo {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 26,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkGraphicsPipelineCreateInfo {
    pub sType: i32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub stageCount: u32,
    pub pStages: *const VkPipelineShaderStageCreateInfo,
    pub pVertexInputState: *const VkPipelineVertexInputStateCreateInfo,
    pub pInputAssemblyState: *const VkPipelineInputAssemblyStateCreateInfo,
    pub pTessellationState: *const c_void,
    pub pViewportState: *const VkPipelineViewportStateCreateInfo,
    pub pRasterizationState: *const VkPipelineRasterizationStateCreateInfo,
    pub pMultisampleState: *const VkPipelineMultisampleStateCreateInfo,
    pub pDepthStencilState: *const VkPipelineDepthStencilStateCreateInfo,
    pub pColorBlendState: *const VkPipelineColorBlendStateCreateInfo,
    pub pDynamicState: *const c_void,
    pub layout: u64,
    pub renderPass: u64,
    pub subpass: u32,
    pub basePipelineHandle: u64,
    pub basePipelineIndex: i32,
}
impl VkGraphicsPipelineCreateInfo {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 28,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkCommandPoolCreateInfo {
    pub sType: i32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub queueFamilyIndex: u32,
}
impl VkCommandPoolCreateInfo {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 39,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkCommandBufferAllocateInfo {
    pub sType: i32,
    pub pNext: *const c_void,
    pub commandPool: u64,
    pub level: i32,
    pub commandBufferCount: u32,
}
impl VkCommandBufferAllocateInfo {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 40,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkCommandBufferInheritanceInfo {
    pub sType: i32,
    pub pNext: *const c_void,
    pub renderPass: u64,
    pub subpass: u32,
    pub framebuffer: u64,
    pub occlusionQueryEnable: u32,
    pub queryFlags: u32,
    pub pipelineStatistics: u32,
}
impl VkCommandBufferInheritanceInfo {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 41,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkCommandBufferBeginInfo {
    pub sType: i32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub pInheritanceInfo: *const VkCommandBufferInheritanceInfo,
}
impl VkCommandBufferBeginInfo {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 42,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) union VkClearColorValue {
    pub float32: [f32; 4],
    pub int32: [i32; 4],
    pub uint32: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkClearDepthStencilValue {
    pub depth: f32,
    pub stencil: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) union VkClearValue {
    pub color: VkClearColorValue,
    pub depthStencil: VkClearDepthStencilValue,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkRenderPassBeginInfo {
    pub sType: i32,
    pub pNext: *const c_void,
    pub renderPass: u64,
    pub framebuffer: u64,
    pub renderArea: VkRect2D,
    pub clearValueCount: u32,
    pub pClearValues: *const VkClearValue,
}
impl VkRenderPassBeginInfo {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 43,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkImageSubresourceLayers {
    pub aspectMask: u32,
    pub mipLevel: u32,
    pub baseArrayLayer: u32,
    pub layerCount: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkOffset3D {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkBufferImageCopy {
    pub bufferOffset: u64,
    pub bufferRowLength: u32,
    pub bufferImageHeight: u32,
    pub imageSubresource: VkImageSubresourceLayers,
    pub imageOffset: VkOffset3D,
    pub imageExtent: VkExtent3D,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct VkBufferMemoryBarrier {
    pub sType: i32,
    pub pNext: *const c_void,
    pub srcAccessMask: u32,
    pub dstAccessMask: u32,
    pub srcQueueFamilyIndex: u32,
    pub dstQueueFamilyIndex: u32,
    pub buffer: u64,
    pub offset: u64,
    pub size: u64,
}
impl VkBufferMemoryBarrier {
    pub fn new() -> Self {
        // All other fields are C scalars, handles or pointers; callers populate required fields.
        Self {
            sType: 44,
            ..unsafe { core::mem::zeroed() }
        }
    }
}

#[cfg_attr(
    target_os = "linux",
    link(name = "libvulkan.so.1", kind = "dylib", modifiers = "+verbatim")
)]
#[cfg_attr(target_os = "android", link(name = "vulkan"))]
#[cfg_attr(target_os = "windows", link(name = "vulkan-1", kind = "raw-dylib"))]
unsafe extern "system" {
    #[cfg(target_os = "linux")]
    pub(super) fn vkCreateXlibSurfaceKHR(
        instance: *mut c_void,
        pCreateInfo: *const VkXlibSurfaceCreateInfoKHR,
        pAllocator: *const c_void,
        pSurface: *mut u64,
    ) -> i32;
    #[cfg(target_os = "windows")]
    pub(super) fn vkCreateWin32SurfaceKHR(
        instance: *mut c_void,
        pCreateInfo: *const VkWin32SurfaceCreateInfoKHR,
        pAllocator: *const c_void,
        pSurface: *mut u64,
    ) -> i32;
    pub(super) fn vkDestroySurfaceKHR(
        instance: *mut c_void,
        surface: u64,
        pAllocator: *const c_void,
    );
    pub(super) fn vkGetPhysicalDeviceSurfaceSupportKHR(
        physicalDevice: *mut c_void,
        queueFamilyIndex: u32,
        surface: u64,
        pSupported: *mut u32,
    ) -> i32;
    pub(super) fn vkGetPhysicalDeviceSurfaceCapabilitiesKHR(
        physicalDevice: *mut c_void,
        surface: u64,
        pSurfaceCapabilities: *mut VkSurfaceCapabilitiesKHR,
    ) -> i32;
    pub(super) fn vkGetPhysicalDeviceSurfaceFormatsKHR(
        physicalDevice: *mut c_void,
        surface: u64,
        pSurfaceFormatCount: *mut u32,
        pSurfaceFormats: *mut VkSurfaceFormatKHR,
    ) -> i32;
    pub(super) fn vkCreateSwapchainKHR(
        device: *mut c_void,
        pCreateInfo: *const VkSwapchainCreateInfoKHR,
        pAllocator: *const c_void,
        pSwapchain: *mut u64,
    ) -> i32;
    pub(super) fn vkDestroySwapchainKHR(
        device: *mut c_void,
        swapchain: u64,
        pAllocator: *const c_void,
    );
    pub(super) fn vkGetSwapchainImagesKHR(
        device: *mut c_void,
        swapchain: u64,
        pSwapchainImageCount: *mut u32,
        pSwapchainImages: *mut u64,
    ) -> i32;
    pub(super) fn vkAcquireNextImageKHR(
        device: *mut c_void,
        swapchain: u64,
        timeout: u64,
        semaphore: u64,
        fence: u64,
        pImageIndex: *mut u32,
    ) -> i32;
    pub(super) fn vkQueuePresentKHR(
        queue: *mut c_void,
        pPresentInfo: *const VkPresentInfoKHR,
    ) -> i32;
    pub(super) fn vkCreateFence(
        device: *mut c_void,
        pCreateInfo: *const VkFenceCreateInfo,
        pAllocator: *const c_void,
        pFence: *mut u64,
    ) -> i32;
    pub(super) fn vkDestroyFence(device: *mut c_void, fence: u64, pAllocator: *const c_void);
    pub(super) fn vkWaitForFences(
        device: *mut c_void,
        fenceCount: u32,
        pFences: *const u64,
        waitAll: u32,
        timeout: u64,
    ) -> i32;
    pub(super) fn vkResetFences(device: *mut c_void, fenceCount: u32, pFences: *const u64) -> i32;
    pub(super) fn vkCreateSemaphore(
        device: *mut c_void,
        pCreateInfo: *const VkSemaphoreCreateInfo,
        pAllocator: *const c_void,
        pSemaphore: *mut u64,
    ) -> i32;
    pub(super) fn vkDestroySemaphore(
        device: *mut c_void,
        semaphore: u64,
        pAllocator: *const c_void,
    );
    pub(super) fn vkResetCommandBuffer(commandBuffer: *mut c_void, flags: u32) -> i32;
    pub(super) fn vkCreateInstance(
        pCreateInfo: *const VkInstanceCreateInfo,
        pAllocator: *const c_void,
        pInstance: *mut *mut c_void,
    ) -> i32;
    pub(super) fn vkDestroyInstance(instance: *mut c_void, pAllocator: *const c_void);
    pub(super) fn vkEnumeratePhysicalDevices(
        instance: *mut c_void,
        pPhysicalDeviceCount: *mut u32,
        pPhysicalDevices: *mut *mut c_void,
    ) -> i32;
    pub(super) fn vkGetPhysicalDeviceQueueFamilyProperties(
        physicalDevice: *mut c_void,
        pQueueFamilyPropertyCount: *mut u32,
        pQueueFamilyProperties: *mut VkQueueFamilyProperties,
    );
    pub(super) fn vkGetPhysicalDeviceMemoryProperties(
        physicalDevice: *mut c_void,
        pMemoryProperties: *mut VkPhysicalDeviceMemoryProperties,
    );
    pub(super) fn vkGetPhysicalDeviceProperties(
        physicalDevice: *mut c_void,
        pProperties: *mut VkPhysicalDeviceProperties,
    );
    pub(super) fn vkGetPhysicalDeviceFormatProperties(
        physicalDevice: *mut c_void,
        format: i32,
        pFormatProperties: *mut VkFormatProperties,
    );
    pub(super) fn vkCreateDevice(
        physicalDevice: *mut c_void,
        pCreateInfo: *const VkDeviceCreateInfo,
        pAllocator: *const c_void,
        pDevice: *mut *mut c_void,
    ) -> i32;
    pub(super) fn vkDestroyDevice(device: *mut c_void, pAllocator: *const c_void);
    pub(super) fn vkGetDeviceQueue(
        device: *mut c_void,
        queueFamilyIndex: u32,
        queueIndex: u32,
        pQueue: *mut *mut c_void,
    );
    pub(super) fn vkDeviceWaitIdle(device: *mut c_void) -> i32;
    pub(super) fn vkQueueSubmit(
        queue: *mut c_void,
        submitCount: u32,
        pSubmits: *const VkSubmitInfo,
        fence: u64,
    ) -> i32;
    pub(super) fn vkQueueWaitIdle(queue: *mut c_void) -> i32;
    pub(super) fn vkCreateBuffer(
        device: *mut c_void,
        pCreateInfo: *const VkBufferCreateInfo,
        pAllocator: *const c_void,
        pBuffer: *mut u64,
    ) -> i32;
    pub(super) fn vkDestroyBuffer(device: *mut c_void, buffer: u64, pAllocator: *const c_void);
    pub(super) fn vkGetBufferMemoryRequirements(
        device: *mut c_void,
        buffer: u64,
        pMemoryRequirements: *mut VkMemoryRequirements,
    );
    pub(super) fn vkCreateImage(
        device: *mut c_void,
        pCreateInfo: *const VkImageCreateInfo,
        pAllocator: *const c_void,
        pImage: *mut u64,
    ) -> i32;
    pub(super) fn vkDestroyImage(device: *mut c_void, image: u64, pAllocator: *const c_void);
    pub(super) fn vkGetImageMemoryRequirements(
        device: *mut c_void,
        image: u64,
        pMemoryRequirements: *mut VkMemoryRequirements,
    );
    pub(super) fn vkAllocateMemory(
        device: *mut c_void,
        pAllocateInfo: *const VkMemoryAllocateInfo,
        pAllocator: *const c_void,
        pMemory: *mut u64,
    ) -> i32;
    pub(super) fn vkFreeMemory(device: *mut c_void, memory: u64, pAllocator: *const c_void);
    pub(super) fn vkBindBufferMemory(
        device: *mut c_void,
        buffer: u64,
        memory: u64,
        memoryOffset: u64,
    ) -> i32;
    pub(super) fn vkBindImageMemory(
        device: *mut c_void,
        image: u64,
        memory: u64,
        memoryOffset: u64,
    ) -> i32;
    pub(super) fn vkMapMemory(
        device: *mut c_void,
        memory: u64,
        offset: u64,
        size: u64,
        flags: u32,
        ppData: *mut *mut c_void,
    ) -> i32;
    pub(super) fn vkUnmapMemory(device: *mut c_void, memory: u64);
    pub(super) fn vkCreateImageView(
        device: *mut c_void,
        pCreateInfo: *const VkImageViewCreateInfo,
        pAllocator: *const c_void,
        pView: *mut u64,
    ) -> i32;
    pub(super) fn vkDestroyImageView(
        device: *mut c_void,
        imageView: u64,
        pAllocator: *const c_void,
    );
    pub(super) fn vkCreateRenderPass(
        device: *mut c_void,
        pCreateInfo: *const VkRenderPassCreateInfo,
        pAllocator: *const c_void,
        pRenderPass: *mut u64,
    ) -> i32;
    pub(super) fn vkDestroyRenderPass(
        device: *mut c_void,
        renderPass: u64,
        pAllocator: *const c_void,
    );
    pub(super) fn vkCreateFramebuffer(
        device: *mut c_void,
        pCreateInfo: *const VkFramebufferCreateInfo,
        pAllocator: *const c_void,
        pFramebuffer: *mut u64,
    ) -> i32;
    pub(super) fn vkDestroyFramebuffer(
        device: *mut c_void,
        framebuffer: u64,
        pAllocator: *const c_void,
    );
    pub(super) fn vkCreateShaderModule(
        device: *mut c_void,
        pCreateInfo: *const VkShaderModuleCreateInfo,
        pAllocator: *const c_void,
        pShaderModule: *mut u64,
    ) -> i32;
    pub(super) fn vkDestroyShaderModule(
        device: *mut c_void,
        shaderModule: u64,
        pAllocator: *const c_void,
    );
    pub(super) fn vkCreatePipelineLayout(
        device: *mut c_void,
        pCreateInfo: *const VkPipelineLayoutCreateInfo,
        pAllocator: *const c_void,
        pPipelineLayout: *mut u64,
    ) -> i32;
    pub(super) fn vkDestroyPipelineLayout(
        device: *mut c_void,
        pipelineLayout: u64,
        pAllocator: *const c_void,
    );
    pub(super) fn vkCreateGraphicsPipelines(
        device: *mut c_void,
        pipelineCache: u64,
        createInfoCount: u32,
        pCreateInfos: *const VkGraphicsPipelineCreateInfo,
        pAllocator: *const c_void,
        pPipelines: *mut u64,
    ) -> i32;
    pub(super) fn vkDestroyPipeline(device: *mut c_void, pipeline: u64, pAllocator: *const c_void);
    pub(super) fn vkCreateCommandPool(
        device: *mut c_void,
        pCreateInfo: *const VkCommandPoolCreateInfo,
        pAllocator: *const c_void,
        pCommandPool: *mut u64,
    ) -> i32;
    pub(super) fn vkDestroyCommandPool(
        device: *mut c_void,
        commandPool: u64,
        pAllocator: *const c_void,
    );
    pub(super) fn vkAllocateCommandBuffers(
        device: *mut c_void,
        pAllocateInfo: *const VkCommandBufferAllocateInfo,
        pCommandBuffers: *mut *mut c_void,
    ) -> i32;
    pub(super) fn vkBeginCommandBuffer(
        commandBuffer: *mut c_void,
        pBeginInfo: *const VkCommandBufferBeginInfo,
    ) -> i32;
    pub(super) fn vkEndCommandBuffer(commandBuffer: *mut c_void) -> i32;
    pub(super) fn vkCmdBeginRenderPass(
        commandBuffer: *mut c_void,
        pRenderPassBegin: *const VkRenderPassBeginInfo,
        contents: i32,
    );
    pub(super) fn vkCmdEndRenderPass(commandBuffer: *mut c_void);
    pub(super) fn vkCmdBindPipeline(
        commandBuffer: *mut c_void,
        pipelineBindPoint: i32,
        pipeline: u64,
    );
    pub(super) fn vkCmdBindVertexBuffers(
        commandBuffer: *mut c_void,
        firstBinding: u32,
        bindingCount: u32,
        pBuffers: *const u64,
        pOffsets: *const u64,
    );
    pub(super) fn vkCmdDraw(
        commandBuffer: *mut c_void,
        vertexCount: u32,
        instanceCount: u32,
        firstVertex: u32,
        firstInstance: u32,
    );
    pub(super) fn vkCmdCopyImageToBuffer(
        commandBuffer: *mut c_void,
        srcImage: u64,
        srcImageLayout: i32,
        dstBuffer: u64,
        regionCount: u32,
        pRegions: *const VkBufferImageCopy,
    );
    pub(super) fn vkCmdPipelineBarrier(
        commandBuffer: *mut c_void,
        srcStageMask: u32,
        dstStageMask: u32,
        dependencyFlags: u32,
        memoryBarrierCount: u32,
        pMemoryBarriers: *const c_void,
        bufferMemoryBarrierCount: u32,
        pBufferMemoryBarriers: *const VkBufferMemoryBarrier,
        imageMemoryBarrierCount: u32,
        pImageMemoryBarriers: *const c_void,
    );
}
