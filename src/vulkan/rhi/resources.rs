//! Buffers, textures, views and samplers; synchronous uploads, mip generation and readback.
use super::record::{Key, Plan, Scope};
use super::track::{access, stage};
use super::*;
use crate::gpu::{Address, BufferUsage, Compare, Filter, Memory, SamplerDesc, TextureUsage};

const DEVICE_LOCAL: u32 = 0x1;
const HOST_VISIBLE_COHERENT: u32 = 0x2 | 0x4;
const HOST_CACHED: u32 = 0x8;
/// VkFormatFeatureFlagBits.
pub(super) const FEATURE_SAMPLED: u32 = 0x1;
pub(super) const FEATURE_STORAGE: u32 = 0x2;
pub(super) const FEATURE_COLOR: u32 = 0x80;
pub(super) const FEATURE_BLEND: u32 = 0x100;
pub(super) const FEATURE_DEPTH: u32 = 0x200;
pub(super) const FEATURE_BLIT_SRC: u32 = 0x400;
pub(super) const FEATURE_BLIT_DST: u32 = 0x800;
pub(super) const FEATURE_LINEAR: u32 = 0x1000;
/// Stages that may sample a texture uploaded by `write_texture`.
const ANY_SHADER: u32 = stage::VERTEX_SHADER | stage::FRAGMENT_SHADER | stage::COMPUTE;

pub(super) fn vk_format(f: Format) -> i32 {
    match f {
        Format::Rgba8Unorm => 37,
        Format::Rgba8Srgb => 43,
        Format::Bgra8Unorm => 44,
        Format::Bgra8Srgb => 50,
        Format::Rgba16Float => 97,
        Format::Rg16Float => 83,
        Format::R16Float => 76,
        Format::R8Unorm => 9,
        Format::R32Float => 100,
        Format::Rgba32Float => 109,
        Format::Depth32Float => 126,
    }
}
pub(super) fn vk_compare(c: Compare) -> i32 {
    match c {
        Compare::Never => 0,
        Compare::Less => 1,
        Compare::Equal => 2,
        Compare::LessEqual => 3,
        Compare::Greater => 4,
        Compare::NotEqual => 5,
        Compare::GreaterEqual => 6,
        Compare::Always => 7,
    }
}
pub(super) fn vk_filter(f: Filter) -> i32 {
    match f {
        Filter::Nearest => 0,
        Filter::Linear => 1,
    }
}

impl<'w> Gpu<'w> {
    pub(super) fn format_features(&self, format: i32) -> u32 {
        // SAFETY: a pure query on the physical device.
        unsafe {
            let mut props: VkFormatProperties = core::mem::zeroed();
            vkGetPhysicalDeviceFormatProperties(self.physical, format, &mut props);
            props.optimalTilingFeatures
        }
    }

    fn allocate(&self, req: VkMemoryRequirements, required: u32, preferred: u32) -> Result<u64> {
        let types = &self.memory.memoryTypes[..self.memory.memoryTypeCount as usize];
        let find = |flags: u32| {
            (0..types.len()).find(|&i| {
                req.memoryTypeBits & (1 << i) != 0 && types[i].propertyFlags & flags == flags
            })
        };
        let index = find(required | preferred)
            .or_else(|| find(required))
            .ok_or_else(|| format!("no memory type with property flags {required:#x}"))?;
        let mut info = VkMemoryAllocateInfo::new();
        info.allocationSize = req.size;
        info.memoryTypeIndex = index as u32;
        // SAFETY: the allocate info outlives the call; the caller owns the memory.
        create("vkAllocateMemory", |o| unsafe {
            vkAllocateMemory(self.device, &info, null(), o)
        })
    }

    /// Creates a buffer bound to its own allocation and maps host-visible memory.
    fn raw_buffer(&self, size: u64, usage: u32, memory: Memory) -> Result<(u64, u64, *mut u8)> {
        let mut ci = VkBufferCreateInfo::new();
        ci.size = size;
        ci.usage = usage;
        let dev = self.device;
        let (required, preferred) = match memory {
            Memory::Device => (0, DEVICE_LOCAL),
            Memory::Upload => (HOST_VISIBLE_COHERENT, 0),
            Memory::Readback => (HOST_VISIBLE_COHERENT, HOST_CACHED),
        };
        // SAFETY: create infos outlive each call. On failure every object created so far is
        // destroyed before returning; on success the caller owns buffer and memory.
        unsafe {
            let buffer = create("vkCreateBuffer", |o| vkCreateBuffer(dev, &ci, null(), o))?;
            let mut req = core::mem::zeroed();
            vkGetBufferMemoryRequirements(dev, buffer, &mut req);
            let mem = match self.allocate(req, required, preferred) {
                Ok(m) => m,
                Err(e) => {
                    vkDestroyBuffer(dev, buffer, null());
                    return Err(e);
                }
            };
            let mut mapped = null_mut();
            let mut rc = vkBindBufferMemory(dev, buffer, mem, 0);
            if rc == 0 && memory != Memory::Device {
                rc = vkMapMemory(dev, mem, 0, u64::MAX, 0, &mut mapped);
            }
            if rc != 0 {
                vkDestroyBuffer(dev, buffer, null());
                vkFreeMemory(dev, mem, null());
                return Err(format!("bind or map buffer memory: VkResult {rc}"));
            }
            Ok((buffer, mem, mapped.cast()))
        }
    }

    pub fn create_buffer(&mut self, d: &BufferDesc) -> Result<BufferId> {
        if d.size == 0 {
            return Err("create_buffer: size must be positive".into());
        }
        if d.usage.is_empty() {
            return Err("create_buffer: usage is empty".into());
        }
        let mut usage = 0;
        for (flag, bit) in [
            (BufferUsage::VERTEX, 0x80),
            (BufferUsage::INDEX, 0x40),
            (BufferUsage::UNIFORM, 0x10),
            (BufferUsage::STORAGE, 0x20),
            (BufferUsage::COPY_SRC, 0x1),
            (BufferUsage::COPY_DST, 0x2),
        ] {
            if d.usage.contains(flag) {
                usage |= bit;
            }
        }
        let (handle, memory, mapped) = self.raw_buffer(d.size, usage, d.memory)?;
        let buffer = Buffer {
            handle,
            memory,
            desc: *d,
            mapped,
            state: State::default(),
            last_use: 0,
        };
        match self.buffers.insert(buffer) {
            Ok(raw) => Ok(BufferId::from_raw(raw)),
            Err(e) => {
                self.free(Garbage::Buffer(handle, memory));
                Err(e)
            }
        }
    }

    pub fn write_buffer(&mut self, id: BufferId, offset: u64, data: &[u8]) -> Result<()> {
        let b = self.buffer(id)?;
        if offset
            .checked_add(data.len() as u64)
            .is_none_or(|end| end > b.desc.size)
        {
            return Err(format!(
                "write_buffer: {} bytes at offset {offset} exceed the {}-byte buffer",
                data.len(),
                b.desc.size
            ));
        }
        if data.is_empty() {
            return Ok(());
        }
        if !b.mapped.is_null() {
            self.wait_serial(b.last_use)?;
            // SAFETY: the persistent mapping covers the whole buffer, the range was checked
            // above, and every submission that used the buffer has completed.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    data.as_ptr(),
                    b.mapped.add(offset as usize),
                    data.len(),
                );
            }
            return Ok(());
        }
        if !b.desc.usage.contains(BufferUsage::COPY_DST) {
            return Err("write_buffer: Memory::Device buffers need BufferUsage::COPY_DST".into());
        }
        let handle = b.handle;
        let staging = self.stage(data)?;
        self.immediate(|gpu, cmd, plan| {
            let mut scope = Scope::default();
            scope.add(
                Key::Buffer(id),
                Use::buffer(stage::TRANSFER, access::TRANSFER_WRITE),
            )?;
            gpu.apply(plan, &scope)?;
            gpu.flush(cmd, plan);
            let region = VkBufferCopy {
                srcOffset: 0,
                dstOffset: offset,
                size: data.len() as u64,
            };
            // SAFETY: both buffers are alive and large enough; cmd is recording.
            unsafe { vkCmdCopyBuffer(cmd, staging, handle, 1, &region) };
            Ok(())
        })
    }

    pub fn read_buffer(&self, id: BufferId) -> Result<Vec<u8>> {
        let b = self.buffer(id)?;
        if b.desc.memory != Memory::Readback {
            return Err("read_buffer: only Memory::Readback buffers can be read".into());
        }
        self.wait_serial(b.last_use)?;
        // SAFETY: the mapping covers `size` bytes; the GPU no longer writes the buffer and its
        // writes were made visible to the host by a barrier in the submission that wrote it.
        Ok(unsafe { core::slice::from_raw_parts(b.mapped, b.desc.size as usize) }.to_vec())
    }

    /// Copies `data` into the shared staging buffer (growing it) and returns that buffer.
    fn stage(&mut self, data: &[u8]) -> Result<u64> {
        self.wait(IMMEDIATE)?;
        let size = data.len() as u64;
        if self.staging.as_ref().is_none_or(|s| s.size < size) {
            if let Some(old) = self.staging.take() {
                self.free(Garbage::Buffer(old.buffer, old.memory));
            }
            let capacity = size.max(1 << 16).next_power_of_two();
            let (buffer, memory, mapped) = self.raw_buffer(capacity, 0x1, Memory::Upload)?;
            self.staging = Some(Staging {
                buffer,
                memory,
                mapped,
                size: capacity,
            });
        }
        let s = self.staging.as_ref().expect("staging buffer created above");
        // SAFETY: the mapping holds at least `size` bytes and the only submission that reads
        // the staging buffer (the synchronous one) is idle.
        unsafe { core::ptr::copy_nonoverlapping(data.as_ptr(), s.mapped, data.len()) };
        Ok(s.buffer)
    }

    /// Records one synchronous submission, submits it and waits for completion.
    pub(super) fn immediate(
        &mut self,
        record: impl FnOnce(&mut Self, Handle, &mut Plan) -> Result<()>,
    ) -> Result<()> {
        let cmd = self.begin(IMMEDIATE)?;
        let mut plan = Plan::default();
        record(self, cmd, &mut plan)?;
        self.execute(IMMEDIATE, cmd, plan, None)?;
        self.wait(IMMEDIATE)?;
        self.collect();
        Ok(())
    }

    pub fn create_texture(&mut self, d: &TextureDesc) -> Result<TextureId> {
        let max = self.limits.maxImageDimension2D;
        if d.width == 0 || d.height == 0 || d.width > max || d.height > max {
            return Err(format!(
                "create_texture: {}x{} is outside 1..={max}",
                d.width, d.height
            ));
        }
        if d.layers == 0 || d.layers > self.limits.maxImageArrayLayers {
            return Err(format!("create_texture: invalid layer count {}", d.layers));
        }
        let chain = 32 - d.width.max(d.height).leading_zeros();
        if d.mips == 0 || d.mips > chain {
            return Err(format!(
                "create_texture: {} mips requested; a {}x{} texture has at most {chain}",
                d.mips, d.width, d.height
            ));
        }
        if d.usage.is_empty() {
            return Err("create_texture: usage is empty".into());
        }
        let depth = d.format.is_depth();
        let format = vk_format(d.format);
        let features = self.format_features(format);
        let mut usage = 0;
        for (flag, bit, feature) in [
            (TextureUsage::SAMPLED, 0x4, FEATURE_SAMPLED),
            (TextureUsage::COLOR_TARGET, 0x10, FEATURE_COLOR),
            (TextureUsage::DEPTH_TARGET, 0x20, FEATURE_DEPTH),
            (TextureUsage::STORAGE, 0x8, FEATURE_STORAGE),
            (TextureUsage::COPY_SRC, 0x1, 0),
            (TextureUsage::COPY_DST, 0x2, 0),
        ] {
            if d.usage.contains(flag) {
                if features & feature != feature {
                    return Err(format!(
                        "create_texture: {:?} does not support {flag:?} on {}",
                        d.format, self.name
                    ));
                }
                usage |= bit;
            }
        }
        if depth && d.usage.contains(TextureUsage::COLOR_TARGET)
            || !depth && d.usage.contains(TextureUsage::DEPTH_TARGET)
        {
            return Err(format!(
                "create_texture: {:?} cannot be a {}",
                d.format,
                if depth {
                    "colour target"
                } else {
                    "depth target"
                }
            ));
        }
        let mut ci = VkImageCreateInfo::new();
        if d.layers >= 6 && d.layers % 6 == 0 && d.width == d.height {
            ci.flags = 0x10; // CUBE_COMPATIBLE: lets 6 layers be bound as a cube
        }
        ci.imageType = 1;
        ci.format = format;
        ci.extent = VkExtent3D {
            width: d.width,
            height: d.height,
            depth: 1,
        };
        ci.mipLevels = d.mips;
        ci.arrayLayers = d.layers;
        ci.samples = 1;
        ci.usage = usage;
        let dev = self.device;
        // SAFETY: create infos outlive each call; partial objects are destroyed on failure.
        let (image, memory) = unsafe {
            let image = create("vkCreateImage", |o| vkCreateImage(dev, &ci, null(), o))?;
            let mut req = core::mem::zeroed();
            vkGetImageMemoryRequirements(dev, image, &mut req);
            let memory = match self.allocate(req, 0, DEVICE_LOCAL) {
                Ok(m) => m,
                Err(e) => {
                    vkDestroyImage(dev, image, null());
                    return Err(e);
                }
            };
            if let Err(e) = check(
                vkBindImageMemory(dev, image, memory, 0),
                "vkBindImageMemory",
            ) {
                vkDestroyImage(dev, image, null());
                vkFreeMemory(dev, memory, null());
                return Err(e);
            }
            (image, memory)
        };
        let texture = Texture {
            image,
            memory,
            desc: *d,
            format,
            aspect: if depth { 2 } else { 1 },
            states: vec![State::default(); (d.mips * d.layers) as usize],
            views: Vec::new(),
            swapchain: false,
            last_use: 0,
        };
        match self.textures.insert(texture) {
            Ok(raw) => Ok(TextureId::from_raw(raw)),
            Err(e) => {
                self.free(Garbage::Image(image, memory, Vec::new()));
                Err(e)
            }
        }
    }

    pub fn texture_desc(&self, id: TextureId) -> Result<TextureDesc> {
        Ok(self.texture(id)?.desc)
    }

    /// A cached view of `id`. Views live until the texture is destroyed.
    pub(super) fn view(&mut self, id: TextureId, key: ViewKey) -> Result<u64> {
        let dev = self.device;
        let t = self
            .textures
            .get_mut(id.raw())
            .ok_or_else(|| format!("{id:?} was destroyed"))?;
        if let Some((_, v)) = t.views.iter().find(|(k, _)| *k == key) {
            return Ok(*v);
        }
        let mut vi = VkImageViewCreateInfo::new();
        vi.image = t.image;
        vi.viewType = key.ty;
        vi.format = t.format;
        vi.subresourceRange = VkImageSubresourceRange {
            aspectMask: t.aspect,
            baseMipLevel: key.base_mip,
            levelCount: key.mips,
            baseArrayLayer: key.base_layer,
            layerCount: key.layers,
        };
        // SAFETY: the image is alive and the range was validated by the caller.
        let view = create("vkCreateImageView", |o| unsafe {
            vkCreateImageView(dev, &vi, null(), o)
        })?;
        t.views.push((key, view));
        Ok(view)
    }

    pub fn write_texture(
        &mut self,
        id: TextureId,
        mip: u32,
        layer: u32,
        data: &[u8],
    ) -> Result<()> {
        let t = self.texture(id)?;
        if t.swapchain || !t.desc.usage.contains(TextureUsage::COPY_DST) {
            return Err("write_texture: the texture needs TextureUsage::COPY_DST".into());
        }
        if mip >= t.desc.mips || layer >= t.desc.layers {
            return Err(format!(
                "write_texture: mip {mip} layer {layer} is outside the texture"
            ));
        }
        let (w, h) = t.desc.mip_size(mip);
        let expected = u64::from(w) * u64::from(h) * u64::from(t.desc.format.texel_size());
        if data.len() as u64 != expected {
            return Err(format!(
                "write_texture: a {w}x{h} {:?} subresource takes {expected} bytes, got {}",
                t.desc.format,
                data.len()
            ));
        }
        let (image, aspect) = (t.image, t.aspect);
        let sampled = t.desc.usage.contains(TextureUsage::SAMPLED);
        let staging = self.stage(data)?;
        self.immediate(|gpu, cmd, plan| {
            let mut scope = Scope::default();
            let copy = Use::new(track::TRANSFER_DST, stage::TRANSFER, access::TRANSFER_WRITE);
            scope.add(Key::Texture(id, mip, layer), copy.discarding())?;
            gpu.apply(plan, &scope)?;
            gpu.flush(cmd, plan);
            let region = VkBufferImageCopy {
                bufferOffset: 0,
                bufferRowLength: 0,
                bufferImageHeight: 0,
                imageSubresource: VkImageSubresourceLayers {
                    aspectMask: aspect,
                    mipLevel: mip,
                    baseArrayLayer: layer,
                    layerCount: 1,
                },
                imageOffset: VkOffset3D { x: 0, y: 0, z: 0 },
                imageExtent: VkExtent3D {
                    width: w,
                    height: h,
                    depth: 1,
                },
            };
            // SAFETY: the staging buffer holds `expected` bytes; the image subresource is in
            // TRANSFER_DST_OPTIMAL after the barrier above.
            unsafe {
                vkCmdCopyBufferToImage(cmd, staging, image, track::TRANSFER_DST, 1, &region);
            }
            if sampled {
                let mut scope = Scope::default();
                let read = Use::new(track::SHADER_READ_ONLY, ANY_SHADER, access::SHADER_READ);
                scope.add(Key::Texture(id, mip, layer), read)?;
                gpu.apply(plan, &scope)?;
                gpu.flush(cmd, plan);
            }
            Ok(())
        })
    }

    pub fn generate_mips(&mut self, id: TextureId) -> Result<()> {
        let t = self.texture(id)?;
        let d = t.desc;
        if t.swapchain
            || !d
                .usage
                .contains(TextureUsage::COPY_SRC | TextureUsage::COPY_DST)
        {
            return Err("generate_mips: the texture needs COPY_SRC | COPY_DST usage".into());
        }
        let needed = FEATURE_BLIT_SRC | FEATURE_BLIT_DST | FEATURE_LINEAR;
        if d.format.is_depth() || self.format_features(t.format) & needed != needed {
            return Err(format!(
                "generate_mips: {:?} cannot be blitted with linear filtering",
                d.format
            ));
        }
        let (image, aspect) = (t.image, t.aspect);
        self.immediate(|gpu, cmd, plan| {
            for mip in 1..d.mips {
                let mut scope = Scope::default();
                let src = Use::new(track::TRANSFER_SRC, stage::TRANSFER, access::TRANSFER_READ);
                let dst = Use::new(track::TRANSFER_DST, stage::TRANSFER, access::TRANSFER_WRITE);
                scope.add_texture(id, (mip - 1, mip), (0, d.layers), src)?;
                scope.add_texture(id, (mip, mip + 1), (0, d.layers), dst.discarding())?;
                gpu.apply(plan, &scope)?;
                gpu.flush(cmd, plan);
                let blit = blit_region(&d, aspect, (mip - 1, 0, d.layers), &d, (mip, 0));
                // SAFETY: both subresource ranges were transitioned above; the image is alive.
                unsafe {
                    vkCmdBlitImage(
                        cmd,
                        image,
                        track::TRANSFER_SRC,
                        image,
                        track::TRANSFER_DST,
                        1,
                        &blit,
                        1,
                    );
                }
            }
            if d.usage.contains(TextureUsage::SAMPLED) {
                let mut scope = Scope::default();
                let read = Use::new(track::SHADER_READ_ONLY, ANY_SHADER, access::SHADER_READ);
                scope.add_texture(id, (0, d.mips), (0, d.layers), read)?;
                gpu.apply(plan, &scope)?;
                gpu.flush(cmd, plan);
            }
            Ok(())
        })
    }

    pub fn create_sampler(&mut self, d: &SamplerDesc) -> Result<SamplerId> {
        if d.max_lod.is_nan() || d.max_lod < 0.0 || d.anisotropy.is_nan() {
            return Err("create_sampler: max_lod must be >= 0 and anisotropy a number".into());
        }
        let anisotropic = self.features.anisotropy && d.anisotropy > 1.0;
        let max_anisotropy = d
            .anisotropy
            .clamp(1.0, self.limits.maxSamplerAnisotropy.max(1.0));
        let address = match d.address {
            Address::Repeat => 0,
            Address::Mirror => 1,
            Address::Clamp => 2,
        };
        let key = [
            vk_filter(d.filter) as u32,
            vk_filter(d.mip_filter) as u32,
            address,
            if anisotropic {
                max_anisotropy.to_bits()
            } else {
                0
            },
            d.compare.map_or(u32::MAX, |c| vk_compare(c) as u32),
            d.max_lod.to_bits(),
            0,
            0,
        ];
        if let Some(&id) = self.sampler_cache.get(&key) {
            return Ok(id);
        }
        let mut ci = VkSamplerCreateInfo::new();
        ci.magFilter = vk_filter(d.filter);
        ci.minFilter = vk_filter(d.filter);
        ci.mipmapMode = vk_filter(d.mip_filter);
        ci.addressModeU = address as i32;
        ci.addressModeV = address as i32;
        ci.addressModeW = address as i32;
        ci.anisotropyEnable = anisotropic as u32;
        ci.maxAnisotropy = if anisotropic { max_anisotropy } else { 1.0 };
        ci.compareEnable = d.compare.is_some() as u32;
        ci.compareOp = d.compare.map_or(0, vk_compare);
        ci.maxLod = d.max_lod;
        // SAFETY: the create info outlives the call; the sampler is owned by the pool.
        let handle = create("vkCreateSampler", |o| unsafe {
            vkCreateSampler(self.device, &ci, null(), o)
        })?;
        let raw = self
            .samplers
            .insert(Sampler {
                handle,
                comparison: d.compare.is_some(),
            })
            .inspect_err(|_| unsafe { vkDestroySampler(self.device, handle, null()) })?;
        let id = SamplerId::from_raw(raw);
        self.sampler_cache.insert(key, id);
        Ok(id)
    }

    pub fn destroy_buffer(&mut self, id: BufferId) {
        if let Some(b) = self.buffers.remove(id.raw()) {
            self.discard(Garbage::Buffer(b.handle, b.memory));
        }
    }
    pub fn destroy_texture(&mut self, id: TextureId) {
        if self.textures.get(id.raw()).is_none_or(|t| t.swapchain) {
            return;
        }
        if let Some(t) = self.textures.remove(id.raw()) {
            let views: Vec<u64> = t.views.iter().map(|(_, v)| *v).collect();
            self.forget_views(&views);
            self.discard(Garbage::Image(t.image, t.memory, views));
        }
    }
    pub fn destroy_bind_group(&mut self, id: BindGroupId) {
        if let Some(g) = self.groups.remove(id.raw()) {
            self.discard(Garbage::Set(g.pool, g.set, g.counts));
        }
    }
}

/// A blit of every layer `layers` of `src` mip onto the same layers of `dst` mip, scaled to fit.
pub(super) fn blit_region(
    src: &TextureDesc,
    aspect: u32,
    (src_mip, base_layer, layers): (u32, u32, u32),
    dst: &TextureDesc,
    (dst_mip, dst_layer): (u32, u32),
) -> VkImageBlit {
    let corner = |(w, h): (u32, u32)| VkOffset3D {
        x: w as i32,
        y: h as i32,
        z: 1,
    };
    let origin = VkOffset3D { x: 0, y: 0, z: 0 };
    VkImageBlit {
        srcSubresource: VkImageSubresourceLayers {
            aspectMask: aspect,
            mipLevel: src_mip,
            baseArrayLayer: base_layer,
            layerCount: layers,
        },
        srcOffsets: [origin, corner(src.mip_size(src_mip))],
        dstSubresource: VkImageSubresourceLayers {
            aspectMask: aspect,
            mipLevel: dst_mip,
            baseArrayLayer: dst_layer,
            layerCount: layers,
        },
        dstOffsets: [origin, corner(dst.mip_size(dst_mip))],
    }
}
