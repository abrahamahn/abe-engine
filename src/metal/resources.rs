//! Buffers, textures, views and samplers; synchronous uploads, mip generation and readback.
use super::ffi::{self, MTLOrigin, MTLSize};
use super::*;
use crate::apple::objc::NSRange;
use crate::gpu::{Address, BufferUsage, Compare, Filter, Memory, SamplerDesc, TextureUsage};

pub(super) fn pixel_format(f: Format) -> u64 {
    use ffi::pixel::*;
    match f {
        Format::Rgba8Unorm => RGBA8_UNORM,
        Format::Rgba8Srgb => RGBA8_UNORM_SRGB,
        Format::Bgra8Unorm => BGRA8_UNORM,
        Format::Bgra8Srgb => BGRA8_UNORM_SRGB,
        Format::Rgba16Float => RGBA16_FLOAT,
        Format::Rg16Float => RG16_FLOAT,
        Format::R16Float => R16_FLOAT,
        Format::R8Unorm => R8_UNORM,
        Format::R32Float => R32_FLOAT,
        Format::Rgba32Float => RGBA32_FLOAT,
        Format::Depth32Float => DEPTH32_FLOAT,
    }
}

pub(super) fn compare_function(c: Compare) -> u64 {
    use ffi::compare::*;
    match c {
        Compare::Never => NEVER,
        Compare::Less => LESS,
        Compare::Equal => EQUAL,
        Compare::LessEqual => LESS_EQUAL,
        Compare::Greater => GREATER,
        Compare::NotEqual => NOT_EQUAL,
        Compare::GreaterEqual => GREATER_EQUAL,
        Compare::Always => ALWAYS,
    }
}

impl<'w> Gpu<'w> {
    pub fn create_buffer(&mut self, d: &BufferDesc) -> Result<BufferId> {
        if d.size == 0 {
            return Err("create_buffer: size must be positive".into());
        }
        if d.usage.is_empty() {
            return Err("create_buffer: usage is empty".into());
        }
        let options = match d.memory {
            Memory::Device => ffi::resource::STORAGE_PRIVATE,
            Memory::Upload => ffi::resource::STORAGE_SHARED | ffi::resource::CPU_WRITE_COMBINED,
            Memory::Readback => ffi::resource::STORAGE_SHARED,
        };
        autoreleasepool(|| {
            // SAFETY: newBufferWithLength:options: returns an owned buffer or nil; `contents`
            // of a shared buffer stays valid for the buffer's lifetime.
            let (obj, contents) = unsafe {
                let obj = Obj::owned(send!(
                    self.device.id(),
                    "newBufferWithLength:options:",
                    d.size as NSUInteger => NSUInteger,
                    options as NSUInteger => NSUInteger;
                    Id
                ))
                .ok_or_else(|| format!("create_buffer: cannot allocate {} bytes", d.size))?;
                let contents = if d.memory == Memory::Device {
                    core::ptr::null_mut()
                } else {
                    send!(obj.id(), "contents"; *mut core::ffi::c_void).cast::<u8>()
                };
                (obj, contents)
            };
            let raw = self.buffers.insert(Buffer {
                obj,
                desc: *d,
                contents,
                last_use: 0,
            })?;
            Ok(BufferId::from_raw(raw))
        })
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
        if !b.contents.is_null() {
            let (contents, last_use) = (b.contents, b.last_use);
            autoreleasepool(|| self.wait_serial(last_use))?;
            // SAFETY: the shared mapping covers the whole buffer, the range was checked above
            // and every submission that used the buffer has completed.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    data.as_ptr(),
                    contents.add(offset as usize),
                    data.len(),
                );
            }
            return Ok(());
        }
        if !b.desc.usage.contains(BufferUsage::COPY_DST) {
            return Err("write_buffer: Memory::Device buffers need BufferUsage::COPY_DST".into());
        }
        let dst = b.obj.clone();
        autoreleasepool(|| {
            let staging = self.stage(data)?;
            self.immediate(|_, cb| {
                // SAFETY: both buffers are alive and large enough (checked above).
                unsafe {
                    let blit = send!(cb, "blitCommandEncoder"; Id);
                    send!(
                        blit,
                        "copyFromBuffer:sourceOffset:toBuffer:destinationOffset:size:",
                        staging => Id,
                        0usize => NSUInteger,
                        dst.id() => Id,
                        offset as NSUInteger => NSUInteger,
                        data.len() => NSUInteger;
                        ()
                    );
                    send!(blit, "endEncoding"; ());
                }
                Ok(())
            })?;
            self.touch_buffer(id);
            Ok(())
        })
    }

    pub fn read_buffer(&self, id: BufferId) -> Result<Vec<u8>> {
        let b = self.buffer(id)?;
        if b.desc.memory != Memory::Readback {
            return Err("read_buffer: only Memory::Readback buffers can be read".into());
        }
        // `&self`: wait without forgetting the command buffers (the next mutable call will).
        autoreleasepool(|| -> Result<()> {
            for (s, cb) in &self.in_flight {
                if *s <= b.last_use {
                    // SAFETY: waiting on and querying a committed command buffer.
                    unsafe {
                        send!(cb.id(), "waitUntilCompleted"; ());
                        if send!(cb.id(), "status"; NSUInteger) as u64 == ffi::status::ERROR {
                            return Err(ns_error(
                                "GPU command buffer failed",
                                send!(cb.id(), "error"; Id),
                            ));
                        }
                    }
                }
            }
            Ok(())
        })?;
        // SAFETY: the shared mapping covers `size` bytes and the GPU no longer writes it;
        // shared memory is coherent once the command buffer completed.
        Ok(unsafe { core::slice::from_raw_parts(b.contents, b.desc.size as usize) }.to_vec())
    }

    fn touch_buffer(&mut self, id: BufferId) {
        let serial = self.serial;
        if let Some(b) = self.buffers.get_mut(id.raw()) {
            b.last_use = serial;
        }
    }

    /// Copies `data` into the shared staging buffer (growing it) and returns that buffer.
    /// Only synchronous submissions read it, and each waits before returning.
    fn stage(&mut self, data: &[u8]) -> Result<Id> {
        let size = data.len() as u64;
        if self.staging.as_ref().is_none_or(|(_, s)| *s < size) {
            let capacity = size.max(1 << 16).next_power_of_two();
            // SAFETY: an owned shared buffer.
            let obj = unsafe {
                Obj::owned(send!(
                    self.device.id(),
                    "newBufferWithLength:options:",
                    capacity as NSUInteger => NSUInteger,
                    (ffi::resource::STORAGE_SHARED | ffi::resource::CPU_WRITE_COMBINED) as NSUInteger => NSUInteger;
                    Id
                ))
            }
            .ok_or("cannot allocate a staging buffer")?;
            self.staging = Some((obj, capacity));
        }
        let (obj, _) = self.staging.as_ref().expect("staging buffer created above");
        // SAFETY: the buffer holds at least `size` bytes and no submission is using it.
        unsafe {
            let contents = send!(obj.id(), "contents"; *mut core::ffi::c_void).cast::<u8>();
            core::ptr::copy_nonoverlapping(data.as_ptr(), contents, data.len());
        }
        Ok(obj.id())
    }

    /// Records one command buffer with `record`, commits it and waits for it.
    pub(super) fn immediate(
        &mut self,
        record: impl FnOnce(&mut Self, Id) -> Result<()>,
    ) -> Result<()> {
        let cb = self.command_buffer()?;
        record(self, cb.id())?;
        let serial = self.commit(cb);
        self.wait_serial(serial)
    }

    pub fn create_texture(&mut self, d: &TextureDesc) -> Result<TextureId> {
        let max = self.limits().max_texture_2d;
        if d.width == 0 || d.height == 0 || d.width > max || d.height > max {
            return Err(format!(
                "create_texture: {}x{} is outside 1..={max}",
                d.width, d.height
            ));
        }
        if d.layers == 0 || d.layers > 2048 {
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
        if d.usage.contains(TextureUsage::STORAGE)
            && matches!(
                d.format,
                Format::Rgba8Srgb | Format::Bgra8Srgb | Format::Depth32Float
            )
        {
            return Err(format!(
                "create_texture: {:?} cannot be a storage texture",
                d.format
            ));
        }
        let mut usage = 0;
        if d.usage.contains(TextureUsage::SAMPLED) || d.usage.contains(TextureUsage::COPY_SRC) {
            // Copy sources are also sampled by scaled blits.
            usage |= ffi::usage::SHADER_READ;
        }
        if d.usage.contains(TextureUsage::STORAGE) {
            usage |= ffi::usage::SHADER_WRITE;
        }
        if d.usage.contains(TextureUsage::COLOR_TARGET)
            || d.usage.contains(TextureUsage::DEPTH_TARGET)
            || (!depth && d.usage.contains(TextureUsage::COPY_DST))
        {
            // Colour copy destinations are rendered to by scaled blits.
            usage |= ffi::usage::RENDER_TARGET;
        }
        let ty = if d.layers > 1 {
            ffi::texture_type::D2_ARRAY
        } else {
            ffi::texture_type::D2
        };
        let format = pixel_format(d.format);
        autoreleasepool(|| {
            let desc = Self::new_object(class!("MTLTextureDescriptor"), "MTLTextureDescriptor")?;
            // SAFETY: setters on a texture descriptor, then an owned texture or nil.
            let obj = unsafe {
                let t = desc.id();
                send!(t, "setTextureType:", ty as NSUInteger => NSUInteger; ());
                send!(t, "setPixelFormat:", format as NSUInteger => NSUInteger; ());
                send!(t, "setWidth:", d.width as NSUInteger => NSUInteger; ());
                send!(t, "setHeight:", d.height as NSUInteger => NSUInteger; ());
                send!(t, "setMipmapLevelCount:", d.mips as NSUInteger => NSUInteger; ());
                send!(t, "setArrayLength:", d.layers as NSUInteger => NSUInteger; ());
                send!(t, "setUsage:", usage as NSUInteger => NSUInteger; ());
                send!(t, "setStorageMode:", ffi::STORAGE_MODE_PRIVATE as NSUInteger => NSUInteger; ());
                Obj::owned(send!(self.device.id(), "newTextureWithDescriptor:", t => Id; Id))
            }
            .ok_or_else(|| {
                format!(
                    "create_texture: {}x{} {:?} ({} layers, {} mips) is not supported on {}",
                    d.width, d.height, d.format, d.layers, d.mips, self.name
                )
            })?;
            let raw = self.textures.insert(Texture {
                obj,
                desc: *d,
                pixel_format: format,
                ty,
                views: Vec::new(),
                swapchain: false,
            })?;
            Ok(TextureId::from_raw(raw))
        })
    }

    pub fn texture_desc(&self, id: TextureId) -> Result<TextureDesc> {
        Ok(self.texture(id)?.desc)
    }

    /// The texture itself when `key` covers it with its own type, otherwise a cached view.
    pub(super) fn view(&mut self, id: TextureId, key: ViewKey) -> Result<Id> {
        let t = self
            .textures
            .get_mut(id.raw())
            .ok_or_else(|| format!("{id:?} was destroyed"))?;
        if key.ty == t.ty
            && key.base_mip == 0
            && key.mips == t.desc.mips
            && key.base_layer == 0
            && key.layers == t.desc.layers
        {
            return Ok(t.obj.id());
        }
        if let Some((_, v)) = t.views.iter().find(|(k, _)| *k == key) {
            return Ok(v.id());
        }
        // SAFETY: a view of a live texture with its own pixel format; the ranges were
        // validated by the caller. Returns an owned view or nil.
        let v = unsafe {
            Obj::owned(send!(
                t.obj.id(),
                "newTextureViewWithPixelFormat:textureType:levels:slices:",
                t.pixel_format as NSUInteger => NSUInteger,
                key.ty as NSUInteger => NSUInteger,
                NSRange { location: key.base_mip as usize, length: key.mips as usize } => NSRange,
                NSRange { location: key.base_layer as usize, length: key.layers as usize } => NSRange;
                Id
            ))
        }
        .ok_or_else(|| format!("{id:?}: cannot create a texture view {key:?}"))?;
        let raw = v.id();
        t.views.push((key, v));
        Ok(raw)
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
        let texel = u64::from(t.desc.format.texel_size());
        let expected = u64::from(w) * u64::from(h) * texel;
        if data.len() as u64 != expected {
            return Err(format!(
                "write_texture: a {w}x{h} {:?} subresource takes {expected} bytes, got {}",
                t.desc.format,
                data.len()
            ));
        }
        let dst = t.obj.clone();
        autoreleasepool(|| {
            let staging = self.stage(data)?;
            self.immediate(|_, cb| {
                // SAFETY: the staging buffer holds the tightly packed rows of one subresource.
                unsafe {
                    let blit = send!(cb, "blitCommandEncoder"; Id);
                    send!(
                        blit,
                        "copyFromBuffer:sourceOffset:sourceBytesPerRow:sourceBytesPerImage:sourceSize:toTexture:destinationSlice:destinationLevel:destinationOrigin:",
                        staging => Id,
                        0usize => NSUInteger,
                        (u64::from(w) * texel) as NSUInteger => NSUInteger,
                        expected as NSUInteger => NSUInteger,
                        MTLSize { width: w as usize, height: h as usize, depth: 1 } => MTLSize,
                        dst.id() => Id,
                        layer as NSUInteger => NSUInteger,
                        mip as NSUInteger => NSUInteger,
                        MTLOrigin::default() => MTLOrigin;
                        ()
                    );
                    send!(blit, "endEncoding"; ());
                }
                Ok(())
            })
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
        if d.format.is_depth() || matches!(d.format, Format::R32Float | Format::Rgba32Float) {
            return Err(format!(
                "generate_mips: {:?} cannot be filtered on every Apple GPU",
                d.format
            ));
        }
        if d.mips < 2 {
            return Ok(());
        }
        let obj = t.obj.clone();
        autoreleasepool(|| {
            self.immediate(|_, cb| {
                // SAFETY: a colour-renderable, filterable texture with a full mip chain.
                unsafe {
                    let blit = send!(cb, "blitCommandEncoder"; Id);
                    send!(blit, "generateMipmapsForTexture:", obj.id() => Id; ());
                    send!(blit, "endEncoding"; ());
                }
                Ok(())
            })
        })
    }

    pub fn create_sampler(&mut self, d: &SamplerDesc) -> Result<SamplerId> {
        if d.max_lod.is_nan() || d.max_lod < 0.0 || d.anisotropy.is_nan() {
            return Err("create_sampler: max_lod must be >= 0 and anisotropy a number".into());
        }
        let filter = |f: Filter| match f {
            Filter::Nearest => ffi::sampler::NEAREST,
            Filter::Linear => ffi::sampler::LINEAR,
        };
        let mip = match d.mip_filter {
            Filter::Nearest => ffi::sampler::MIP_NEAREST,
            Filter::Linear => ffi::sampler::MIP_LINEAR,
        };
        let address = match d.address {
            Address::Repeat => ffi::sampler::REPEAT,
            Address::Mirror => ffi::sampler::MIRROR_REPEAT,
            Address::Clamp => ffi::sampler::CLAMP_TO_EDGE,
        };
        let anisotropy = d.anisotropy.clamp(1.0, 16.0).round() as u64;
        let key = [
            filter(d.filter),
            mip,
            address,
            anisotropy,
            d.compare.map_or(u64::MAX, compare_function),
            u64::from(d.max_lod.to_bits()),
            0,
            0,
        ];
        if let Some(&id) = self.sampler_cache.get(&key) {
            return Ok(id);
        }
        autoreleasepool(|| {
            let desc = Self::new_object(class!("MTLSamplerDescriptor"), "MTLSamplerDescriptor")?;
            // SAFETY: setters on a sampler descriptor, then an owned sampler state or nil.
            let obj = unsafe {
                let s = desc.id();
                send!(s, "setMinFilter:", filter(d.filter) as NSUInteger => NSUInteger; ());
                send!(s, "setMagFilter:", filter(d.filter) as NSUInteger => NSUInteger; ());
                send!(s, "setMipFilter:", mip as NSUInteger => NSUInteger; ());
                send!(s, "setSAddressMode:", address as NSUInteger => NSUInteger; ());
                send!(s, "setTAddressMode:", address as NSUInteger => NSUInteger; ());
                send!(s, "setRAddressMode:", address as NSUInteger => NSUInteger; ());
                send!(s, "setMaxAnisotropy:", anisotropy as NSUInteger => NSUInteger; ());
                if let Some(c) = d.compare {
                    send!(s, "setCompareFunction:", compare_function(c) as NSUInteger => NSUInteger; ());
                }
                send!(s, "setLodMaxClamp:", d.max_lod => f32; ());
                Obj::owned(send!(self.device.id(), "newSamplerStateWithDescriptor:", s => Id; Id))
            }
            .ok_or("create_sampler: newSamplerStateWithDescriptor returned nil")?;
            let raw = self.samplers.insert(Sampler {
                obj,
                comparison: d.compare.is_some(),
            })?;
            let id = SamplerId::from_raw(raw);
            self.sampler_cache.insert(key, id);
            Ok(id)
        })
    }

    /// Command buffers retain what they use, so the engine's reference can go at once.
    pub fn destroy_buffer(&mut self, id: BufferId) {
        autoreleasepool(|| drop(self.buffers.remove(id.raw())));
    }
    pub fn destroy_texture(&mut self, id: TextureId) {
        if self.textures.get(id.raw()).is_none_or(|t| t.swapchain) {
            return;
        }
        autoreleasepool(|| drop(self.textures.remove(id.raw())));
    }
    pub fn destroy_bind_group(&mut self, id: crate::gpu::BindGroupId) {
        self.groups.remove(id.raw());
    }
}
