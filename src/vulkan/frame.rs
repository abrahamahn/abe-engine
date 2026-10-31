use super::*;
use core::mem::{size_of, size_of_val, zeroed};
use core::ptr::copy_nonoverlapping;

// Spec names next to the small Vulkan 1.0 subset used by this backend.
const RGBA8: i32 = 37; // VK_FORMAT_R8G8B8A8_UNORM
const VEC4: i32 = 109; // VK_FORMAT_R32G32B32A32_SFLOAT
const COLOR_LAYOUT: i32 = 2;
const DEPTH_LAYOUT: i32 = 3;
const TRANSFER_SRC_LAYOUT: i32 = 6;
const COLOR_OUTPUT: u32 = 0x400;
const TRANSFER: u32 = 0x1000;
const HOST: u32 = 0x4000;
const COLOR_WRITE: u32 = 0x100;
const TRANSFER_READ: u32 = 0x800;
const TRANSFER_WRITE: u32 = 0x1000;
const HOST_READ: u32 = 0x2000;

pub(super) struct Frame<'a> {
    gpu: &'a VulkanRenderer,
    buffers: Vec<u64>,
    images: Vec<u64>,
    views: Vec<u64>,
    memory: Vec<u64>,
    shaders: Vec<u64>,
    pass: u64,
    framebuffer: u64,
    layout: u64,
    pipeline: u64,
    pool: u64,
    in_flight: bool,
}
impl<'a> Frame<'a> {
    pub(super) fn new(gpu: &'a VulkanRenderer) -> Self {
        Self {
            gpu,
            buffers: vec![],
            images: vec![],
            views: vec![],
            memory: vec![],
            shaders: vec![],
            pass: 0,
            framebuffer: 0,
            layout: 0,
            pipeline: 0,
            pool: 0,
            in_flight: false,
        }
    }
    fn allocate(&mut self, req: VkMemoryRequirements, flags: u32) -> Result<u64> {
        let mem = &self.gpu.memory;
        let index = (0..mem.memoryTypeCount)
            .find(|i| {
                req.memoryTypeBits & (1 << i) != 0
                    && mem.memoryTypes[*i as usize].propertyFlags & flags == flags
            })
            .ok_or_else(|| format!("no compatible memory type for flags {flags:#x}"))?;
        let mut info = VkMemoryAllocateInfo::new();
        info.allocationSize = req.size;
        info.memoryTypeIndex = index;
        let value = create("vkAllocateMemory", |o| unsafe {
            vkAllocateMemory(self.gpu.device, &info, null(), o)
        })?;
        self.memory.push(value);
        Ok(value)
    }
    fn buffer(&mut self, bytes: u64, usage: u32) -> Result<(u64, u64)> {
        let mut ci = VkBufferCreateInfo::new();
        ci.size = bytes;
        ci.usage = usage;
        let dev = self.gpu.device;
        unsafe {
            let buf = create("vkCreateBuffer", |o| vkCreateBuffer(dev, &ci, null(), o))?;
            self.buffers.push(buf);
            let mut req = zeroed();
            vkGetBufferMemoryRequirements(dev, buf, &mut req);
            let memory = self.allocate(req, 2 | 4)?; // HOST_VISIBLE | HOST_COHERENT
            check(
                vkBindBufferMemory(dev, buf, memory, 0),
                "vkBindBufferMemory",
            )?;
            Ok((buf, memory))
        }
    }
    fn image(
        &mut self,
        width: u32,
        height: u32,
        format: i32,
        usage: u32,
        aspect: u32,
    ) -> Result<(u64, u64)> {
        let mut ci = VkImageCreateInfo::new();
        ci.imageType = 1; // 2D
        ci.format = format;
        ci.extent = VkExtent3D {
            width,
            height,
            depth: 1,
        };
        ci.mipLevels = 1;
        ci.arrayLayers = 1;
        ci.samples = 1;
        ci.usage = usage;
        let dev = self.gpu.device;
        unsafe {
            let image = create("vkCreateImage", |o| vkCreateImage(dev, &ci, null(), o))?;
            self.images.push(image);
            let mut req = zeroed();
            vkGetImageMemoryRequirements(dev, image, &mut req);
            let mem = self.allocate(req, 0)?;
            check(vkBindImageMemory(dev, image, mem, 0), "vkBindImageMemory")?;
            let mut vi = VkImageViewCreateInfo::new();
            vi.image = image;
            vi.viewType = 1;
            vi.format = format;
            vi.subresourceRange = VkImageSubresourceRange {
                aspectMask: aspect,
                baseMipLevel: 0,
                levelCount: 1,
                baseArrayLayer: 0,
                layerCount: 1,
            };
            let view = create("vkCreateImageView", |o| {
                vkCreateImageView(dev, &vi, null(), o)
            })?;
            self.views.push(view);
            Ok((image, view))
        }
    }
    fn targets(&mut self, width: u32, height: u32) -> Result<u64> {
        let (color, cv) = self.image(width, height, RGBA8, 0x10 | 1, 1)?; // COLOR_ATTACHMENT | TRANSFER_SRC
        let (_, dv) = self.image(width, height, self.gpu.depth_format, 0x20, 2)?; // DEPTH_STENCIL_ATTACHMENT
        let attachments = [
            VkAttachmentDescription {
                flags: 0,
                format: RGBA8,
                samples: 1,
                loadOp: 1,
                storeOp: 0,
                stencilLoadOp: 2,
                stencilStoreOp: 1,
                initialLayout: 0,
                finalLayout: TRANSFER_SRC_LAYOUT,
            },
            VkAttachmentDescription {
                flags: 0,
                format: self.gpu.depth_format,
                samples: 1,
                loadOp: 1,
                storeOp: 1,
                stencilLoadOp: 2,
                stencilStoreOp: 1,
                initialLayout: 0,
                finalLayout: DEPTH_LAYOUT,
            },
        ];
        let color_ref = VkAttachmentReference {
            attachment: 0,
            layout: COLOR_LAYOUT,
        };
        let depth_ref = VkAttachmentReference {
            attachment: 1,
            layout: DEPTH_LAYOUT,
        };
        let mut sub: VkSubpassDescription = unsafe { zeroed() };
        sub.colorAttachmentCount = 1;
        sub.pColorAttachments = &color_ref;
        sub.pDepthStencilAttachment = &depth_ref;
        let dep = VkSubpassDependency {
            srcSubpass: 0,
            dstSubpass: u32::MAX,
            srcStageMask: COLOR_OUTPUT,
            dstStageMask: TRANSFER,
            srcAccessMask: COLOR_WRITE,
            dstAccessMask: TRANSFER_READ,
            dependencyFlags: 0,
        };
        let mut pi = VkRenderPassCreateInfo::new();
        pi.attachmentCount = 2;
        pi.pAttachments = attachments.as_ptr();
        pi.subpassCount = 1;
        pi.pSubpasses = &sub;
        pi.dependencyCount = 1;
        pi.pDependencies = &dep;
        self.pass = create("vkCreateRenderPass", |o| unsafe {
            vkCreateRenderPass(self.gpu.device, &pi, null(), o)
        })?;
        let views = [cv, dv];
        let mut fi = VkFramebufferCreateInfo::new();
        fi.renderPass = self.pass;
        fi.attachmentCount = 2;
        fi.pAttachments = views.as_ptr();
        fi.width = width;
        fi.height = height;
        fi.layers = 1;
        self.framebuffer = create("vkCreateFramebuffer", |o| unsafe {
            vkCreateFramebuffer(self.gpu.device, &fi, null(), o)
        })?;
        Ok(color)
    }
    fn shader(&mut self, words: &[u32], stage: i32) -> Result<VkPipelineShaderStageCreateInfo> {
        let mut ci = VkShaderModuleCreateInfo::new();
        ci.codeSize = size_of_val(words);
        ci.pCode = words.as_ptr();
        let module = create("vkCreateShaderModule", |o| unsafe {
            vkCreateShaderModule(self.gpu.device, &ci, null(), o)
        })?;
        self.shaders.push(module);
        let mut si = VkPipelineShaderStageCreateInfo::new();
        si.stage = stage;
        si.module = module;
        si.pName = c"main".as_ptr();
        Ok(si)
    }
    fn create_pipeline(&mut self, width: u32, height: u32) -> Result<()> {
        let stages = [
            self.shader(&shaders::vertex(), 1)?,
            self.shader(&shaders::fragment(), 0x10)?,
        ];
        let binding = VkVertexInputBindingDescription {
            binding: 0,
            stride: size_of::<Vertex>() as u32,
            inputRate: 0,
        };
        let attrs = [
            VkVertexInputAttributeDescription {
                location: 0,
                binding: 0,
                format: VEC4,
                offset: 0,
            },
            VkVertexInputAttributeDescription {
                location: 1,
                binding: 0,
                format: VEC4,
                offset: core::mem::offset_of!(Vertex, rgba) as u32,
            },
        ];
        let mut vi = VkPipelineVertexInputStateCreateInfo::new();
        vi.vertexBindingDescriptionCount = 1;
        vi.pVertexBindingDescriptions = &binding;
        vi.vertexAttributeDescriptionCount = 2;
        vi.pVertexAttributeDescriptions = attrs.as_ptr();
        let mut ia = VkPipelineInputAssemblyStateCreateInfo::new();
        ia.topology = 3; // TRIANGLE_LIST
        let viewport = VkViewport {
            x: 0.0,
            y: 0.0,
            width: width as f32,
            height: height as f32,
            minDepth: 0.0,
            maxDepth: 1.0,
        };
        let rect = area(width, height);
        let mut vp = VkPipelineViewportStateCreateInfo::new();
        vp.viewportCount = 1;
        vp.pViewports = &viewport;
        vp.scissorCount = 1;
        vp.pScissors = &rect;
        let mut rs = VkPipelineRasterizationStateCreateInfo::new();
        rs.lineWidth = 1.0;
        let mut ms = VkPipelineMultisampleStateCreateInfo::new();
        ms.rasterizationSamples = 1;
        let mut ds = VkPipelineDepthStencilStateCreateInfo::new();
        ds.depthTestEnable = 1;
        ds.depthWriteEnable = 1;
        ds.depthCompareOp = 1; // LESS
        let mut color: VkPipelineColorBlendAttachmentState = unsafe { zeroed() };
        color.colorWriteMask = 15;
        let mut cb = VkPipelineColorBlendStateCreateInfo::new();
        cb.attachmentCount = 1;
        cb.pAttachments = &color;
        self.layout = create("vkCreatePipelineLayout", |o| unsafe {
            vkCreatePipelineLayout(
                self.gpu.device,
                &VkPipelineLayoutCreateInfo::new(),
                null(),
                o,
            )
        })?;
        let mut pi = VkGraphicsPipelineCreateInfo::new();
        pi.stageCount = 2;
        pi.pStages = stages.as_ptr();
        pi.pVertexInputState = &vi;
        pi.pInputAssemblyState = &ia;
        pi.pViewportState = &vp;
        pi.pRasterizationState = &rs;
        pi.pMultisampleState = &ms;
        pi.pDepthStencilState = &ds;
        pi.pColorBlendState = &cb;
        pi.renderPass = self.pass;
        pi.layout = self.layout;
        self.pipeline = create("vkCreateGraphicsPipelines", |o| unsafe {
            vkCreateGraphicsPipelines(self.gpu.device, 0, 1, &pi, null(), o)
        })?;
        Ok(())
    }
    fn commands(&mut self) -> Result<Handle> {
        let mut ci = VkCommandPoolCreateInfo::new();
        ci.queueFamilyIndex = self.gpu.family;
        self.pool = create("vkCreateCommandPool", |o| unsafe {
            vkCreateCommandPool(self.gpu.device, &ci, null(), o)
        })?;
        let mut ai = VkCommandBufferAllocateInfo::new();
        ai.commandPool = self.pool;
        ai.commandBufferCount = 1;
        create("vkAllocateCommandBuffers", |o| unsafe {
            vkAllocateCommandBuffers(self.gpu.device, &ai, o)
        })
    }
    pub(super) fn draw(
        mut self,
        width: u32,
        height: u32,
        clear: [f32; 4],
        vertices: &[Vertex],
    ) -> Result<Vec<u8>> {
        let bytes = u64::from(width) * u64::from(height) * 4;
        let image = self.targets(width, height)?;
        self.create_pipeline(width, height)?;
        let dev = self.gpu.device;
        let (out, out_mem) = self.buffer(bytes, 2)?; // TRANSFER_DST
        let mut vb = 0;
        // SAFETY: sizes match each allocation, all pointer data outlives its synchronous
        // call, and Frame owns every Vulkan object until submission and readback complete.
        unsafe {
            if !vertices.is_empty() {
                let size = size_of_val(vertices);
                let (buffer, memory) = self.buffer(size as u64, 0x80)?; // VERTEX_BUFFER
                vb = buffer;
                let ptr = create("map vertex buffer", |o| {
                    vkMapMemory(dev, memory, 0, size as u64, 0, o)
                })?;
                copy_nonoverlapping(vertices.as_ptr().cast::<u8>(), ptr.cast::<u8>(), size);
                vkUnmapMemory(dev, memory);
            }
            let cmd = self.commands()?;
            let mut begin = VkCommandBufferBeginInfo::new();
            begin.flags = 1;
            check(vkBeginCommandBuffer(cmd, &begin), "vkBeginCommandBuffer")?;
            let clears = [
                VkClearValue {
                    color: VkClearColorValue { float32: clear },
                },
                VkClearValue {
                    depthStencil: VkClearDepthStencilValue {
                        depth: 1.0,
                        stencil: 0,
                    },
                },
            ];
            let mut info = VkRenderPassBeginInfo::new();
            info.renderPass = self.pass;
            info.framebuffer = self.framebuffer;
            info.renderArea = area(width, height);
            info.clearValueCount = 2;
            info.pClearValues = clears.as_ptr();
            vkCmdBeginRenderPass(cmd, &info, 0);
            if vb != 0 {
                vkCmdBindPipeline(cmd, 0, self.pipeline);
                vkCmdBindVertexBuffers(cmd, 0, 1, &vb, &0);
                vkCmdDraw(cmd, vertices.len() as u32, 1, 0, 0);
            }
            vkCmdEndRenderPass(cmd);
            let region = VkBufferImageCopy {
                bufferOffset: 0,
                bufferRowLength: 0,
                bufferImageHeight: 0,
                imageSubresource: VkImageSubresourceLayers {
                    aspectMask: 1,
                    mipLevel: 0,
                    baseArrayLayer: 0,
                    layerCount: 1,
                },
                imageOffset: VkOffset3D { x: 0, y: 0, z: 0 },
                imageExtent: VkExtent3D {
                    width,
                    height,
                    depth: 1,
                },
            };
            vkCmdCopyImageToBuffer(cmd, image, TRANSFER_SRC_LAYOUT, out, 1, &region);
            let mut barrier = VkBufferMemoryBarrier::new();
            barrier.srcAccessMask = TRANSFER_WRITE;
            barrier.dstAccessMask = HOST_READ;
            barrier.srcQueueFamilyIndex = u32::MAX;
            barrier.dstQueueFamilyIndex = u32::MAX;
            barrier.buffer = out;
            barrier.size = bytes;
            vkCmdPipelineBarrier(cmd, TRANSFER, HOST, 0, 0, null(), 1, &barrier, 0, null());
            check(vkEndCommandBuffer(cmd), "vkEndCommandBuffer")?;
            let mut submission = VkSubmitInfo::new();
            submission.commandBufferCount = 1;
            submission.pCommandBuffers = &cmd;
            check(
                vkQueueSubmit(self.gpu.queue, 1, &submission, 0),
                "vkQueueSubmit",
            )?;
            self.in_flight = true;
            check(vkQueueWaitIdle(self.gpu.queue), "wait for frame")?;
            self.in_flight = false;
            let mut pixels = vec![0; bytes as usize];
            let src = create("map readback buffer", |o| {
                vkMapMemory(dev, out_mem, 0, bytes, 0, o)
            })?;
            copy_nonoverlapping(src.cast::<u8>(), pixels.as_mut_ptr(), pixels.len());
            vkUnmapMemory(dev, out_mem);
            Ok(pixels)
        }
    }
}
fn area(width: u32, height: u32) -> VkRect2D {
    VkRect2D {
        offset: VkOffset2D { x: 0, y: 0 },
        extent: VkExtent2D { width, height },
    }
}
impl Drop for Frame<'_> {
    fn drop(&mut self) {
        let d = self.gpu.device;
        unsafe {
            if self.in_flight {
                vkDeviceWaitIdle(d);
            }
            vkDestroyCommandPool(d, self.pool, null());
            vkDestroyFramebuffer(d, self.framebuffer, null());
            vkDestroyPipeline(d, self.pipeline, null());
            vkDestroyPipelineLayout(d, self.layout, null());
            vkDestroyRenderPass(d, self.pass, null());
            for h in &self.shaders {
                vkDestroyShaderModule(d, *h, null());
            }
            for h in &self.views {
                vkDestroyImageView(d, *h, null());
            }
            for h in &self.images {
                vkDestroyImage(d, *h, null());
            }
            for h in &self.buffers {
                vkDestroyBuffer(d, *h, null());
            }
            for h in &self.memory {
                vkFreeMemory(d, *h, null());
            }
        }
    }
}
