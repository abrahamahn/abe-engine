//! Replays a backend-neutral [`Encoder`] into a Metal command buffer: one render or compute
//! encoder per pass or dispatch and one blit encoder per copy. Metal orders the encoders of
//! one queue and tracks hazards on its resources, so no barriers are recorded; the replay
//! validates what the Vulkan backend validates and resolves bind groups against the current
//! pipeline's binding maps.
use super::ffi::{self, MTLClearColor, MTLOrigin, MTLScissorRect, MTLSize, MTLViewport};
use super::*;
use crate::gpu::{
    BindGroupId, BufferUsage, Command, Encoder, Filter, Load, PassDesc, TextureUsage,
};
use abe_shader::MslIndex;
use core::ops::Range;

/// What the replay has bound on the current encoder, to skip redundant calls.
#[derive(Default)]
struct Bindings {
    buffers: HashMap<(u8, u32), (Id, u64)>,
    textures: HashMap<(u8, u32), Id>,
    samplers: HashMap<(u8, u32), Id>,
    bytes: HashMap<(u8, u32), Vec<u8>>,
}

/// State of the render pass being replayed.
#[derive(Default)]
struct DrawState {
    pipeline: Option<PipelineId>,
    groups: [Option<BindGroupId>; 8],
    vertex: bool,
    index: Option<(Id, u64)>,
    push: Option<Range<usize>>,
    bound: Bindings,
}

fn stage_code(k: StageKind) -> u8 {
    match k {
        StageKind::Vertex => 0,
        StageKind::Fragment => 1,
        StageKind::Compute => 2,
    }
}

/// `set{Vertex,Fragment,}Buffer:offset:atIndex:` and friends on a render or compute encoder.
struct Target {
    enc: Id,
    kind: StageKind,
}

impl Target {
    unsafe fn buffer(&self, b: Id, offset: u64, index: u32) {
        let (o, i) = (offset as NSUInteger, index as NSUInteger);
        // SAFETY: (all methods) the encoder is live and recording; objects are alive.
        unsafe {
            match self.kind {
                StageKind::Vertex => {
                    send!(self.enc, "setVertexBuffer:offset:atIndex:", b => Id, o => NSUInteger, i => NSUInteger; ())
                }
                StageKind::Fragment => {
                    send!(self.enc, "setFragmentBuffer:offset:atIndex:", b => Id, o => NSUInteger, i => NSUInteger; ())
                }
                StageKind::Compute => {
                    send!(self.enc, "setBuffer:offset:atIndex:", b => Id, o => NSUInteger, i => NSUInteger; ())
                }
            }
        }
    }
    unsafe fn bytes(&self, data: &[u8], index: u32) {
        let (p, n, i) = (
            data.as_ptr() as *const core::ffi::c_void,
            data.len() as NSUInteger,
            index as NSUInteger,
        );
        // SAFETY: Metal copies the bytes during the call.
        unsafe {
            match self.kind {
                StageKind::Vertex => {
                    send!(self.enc, "setVertexBytes:length:atIndex:", p => *const core::ffi::c_void, n => NSUInteger, i => NSUInteger; ())
                }
                StageKind::Fragment => {
                    send!(self.enc, "setFragmentBytes:length:atIndex:", p => *const core::ffi::c_void, n => NSUInteger, i => NSUInteger; ())
                }
                StageKind::Compute => {
                    send!(self.enc, "setBytes:length:atIndex:", p => *const core::ffi::c_void, n => NSUInteger, i => NSUInteger; ())
                }
            }
        }
    }
    unsafe fn texture(&self, t: Id, index: u32) {
        let i = index as NSUInteger;
        // SAFETY: as above.
        unsafe {
            match self.kind {
                StageKind::Vertex => {
                    send!(self.enc, "setVertexTexture:atIndex:", t => Id, i => NSUInteger; ())
                }
                StageKind::Fragment => {
                    send!(self.enc, "setFragmentTexture:atIndex:", t => Id, i => NSUInteger; ())
                }
                StageKind::Compute => {
                    send!(self.enc, "setTexture:atIndex:", t => Id, i => NSUInteger; ())
                }
            }
        }
    }
    unsafe fn sampler(&self, s: Id, index: u32) {
        let i = index as NSUInteger;
        // SAFETY: as above.
        unsafe {
            match self.kind {
                StageKind::Vertex => {
                    send!(self.enc, "setVertexSamplerState:atIndex:", s => Id, i => NSUInteger; ())
                }
                StageKind::Fragment => {
                    send!(self.enc, "setFragmentSamplerState:atIndex:", s => Id, i => NSUInteger; ())
                }
                StageKind::Compute => {
                    send!(self.enc, "setSamplerState:atIndex:", s => Id, i => NSUInteger; ())
                }
            }
        }
    }
}

impl<'w> Gpu<'w> {
    pub fn submit(&mut self, e: Encoder) -> Result<()> {
        e.finish()?;
        autoreleasepool(|| {
            self.collect()?;
            let cb = self.command_buffer()?;
            self.record(cb.id(), &e, None)?;
            let serial = self.commit(cb);
            self.mark_used(&e, serial);
            self.wait_serial(serial)
        })
    }

    /// A texture the current submission may use: swapchain textures only while acquired.
    fn usable(&self, id: TextureId, acquired: Option<TextureId>) -> Result<&Texture> {
        let t = self.texture(id)?;
        if t.swapchain && acquired != Some(id) {
            return Err(format!(
                "{id:?} is a swapchain texture; only present() may use it, after acquire() \
                 returned it"
            ));
        }
        Ok(t)
    }

    /// Records that the buffers an encoder uses are busy until submission `serial`.
    pub(super) fn mark_used(&mut self, e: &Encoder, serial: u64) {
        let mut buffers = Vec::new();
        let mut groups = Vec::new();
        for c in &e.commands {
            match c {
                Command::SetBindGroup(_, g) => groups.push(*g),
                Command::SetVertexBuffer(b, _) | Command::SetIndexBuffer(b, _) => buffers.push(*b),
                Command::CopyTextureToBuffer { dst, .. } => buffers.push(*dst),
                Command::CopyBufferToBuffer { src, dst, .. } => {
                    buffers.push(*src);
                    buffers.push(*dst);
                }
                _ => {}
            }
        }
        groups.extend(e.groups.iter().map(|(_, g)| *g));
        for g in groups {
            if let Some(group) = self.groups.get(g.raw()) {
                for (_, b) in &group.bound {
                    if let Bound::Buffer { id, .. } = b {
                        buffers.push(*id);
                    }
                }
            }
        }
        for b in buffers {
            if let Some(buf) = self.buffers.get_mut(b.raw()) {
                buf.last_use = serial;
            }
        }
    }

    pub(super) fn record(
        &mut self,
        cb: Id,
        e: &Encoder,
        acquired: Option<TextureId>,
    ) -> Result<()> {
        e.finish()?;
        let commands = &e.commands;
        let mut i = 0;
        while i < commands.len() {
            match &commands[i] {
                Command::BeginPass(desc) => {
                    let end = commands[i..]
                        .iter()
                        .position(|c| matches!(c, Command::EndPass))
                        .map(|n| i + n)
                        .ok_or("begin_pass without end_pass")?;
                    self.record_pass(cb, e, desc, &commands[i + 1..end], acquired)?;
                    i = end;
                }
                Command::Dispatch {
                    pipeline,
                    groups,
                    push,
                    size,
                } => self.record_dispatch(
                    cb,
                    *pipeline,
                    &e.groups[groups.clone()],
                    &e.bytes[push.clone()],
                    *size,
                )?,
                &Command::CopyTextureToBuffer {
                    src,
                    mip,
                    layer,
                    dst,
                    offset,
                } => self.record_texture_copy(cb, (src, mip, layer), dst, offset, acquired)?,
                &Command::CopyBufferToBuffer {
                    src,
                    src_offset,
                    dst,
                    dst_offset,
                    size,
                } => self.record_buffer_copy(cb, (src, src_offset), (dst, dst_offset), size)?,
                &Command::Blit { src, dst, filter } => {
                    self.record_blit(cb, src, dst, filter, acquired)?
                }
                other => return Err(format!("{other:?} recorded outside a pass")),
            }
            i += 1;
        }
        Ok(())
    }

    fn record_pass(
        &mut self,
        cb: Id,
        e: &Encoder,
        desc: &PassDesc,
        body: &[Command],
        acquired: Option<TextureId>,
    ) -> Result<()> {
        if desc.colors.is_empty() && desc.depth.is_none() {
            return Err("begin_pass: a pass needs at least one attachment".into());
        }
        if desc.colors.len() > 8 {
            return Err(format!(
                "begin_pass: {} colour attachments exceed the device limit",
                desc.colors.len()
            ));
        }
        let descriptor =
            Self::new_object(class!("MTLRenderPassDescriptor"), "MTLRenderPassDescriptor")?;
        let rp = descriptor.id();
        let mut size = None;
        let mut attached: Vec<(TextureId, u32, u32)> = Vec::new();
        // SAFETY: (whole function) descriptors and encoders are live; textures were looked up
        // in the pools and stay alive for the command buffer.
        unsafe {
            let colors = send!(rp, "colorAttachments"; Id);
            let attachments = desc
                .colors
                .iter()
                .enumerate()
                .map(|(i, a)| (a, Some(i)))
                .chain(desc.depth.iter().map(|a| (a, None)));
            for (a, index) in attachments {
                let depth = index.is_none();
                let t = self.usable(a.texture, acquired)?;
                let what = if depth { "depth" } else { "colour" };
                let need = if depth {
                    TextureUsage::DEPTH_TARGET
                } else {
                    TextureUsage::COLOR_TARGET
                };
                if depth != t.desc.format.is_depth() || !t.desc.usage.contains(need) {
                    return Err(format!(
                        "begin_pass: {:?} ({:?}, {:?}) cannot be a {what} attachment",
                        a.texture, t.desc.format, t.desc.usage
                    ));
                }
                if a.mip >= t.desc.mips || a.layer >= t.desc.layers {
                    return Err(format!(
                        "begin_pass: mip {} layer {} is outside {:?}",
                        a.mip, a.layer, a.texture
                    ));
                }
                let extent = t.desc.mip_size(a.mip);
                if *size.get_or_insert(extent) != extent {
                    return Err("begin_pass: attachments differ in size".into());
                }
                let sub = (a.texture, a.mip, a.layer);
                if attached.contains(&sub) {
                    return Err("begin_pass: the same subresource is attached twice".into());
                }
                attached.push(sub);
                let target = match index {
                    Some(i) => at_index(colors, i),
                    None => send!(rp, "depthAttachment"; Id),
                };
                send!(target, "setTexture:", t.obj.id() => Id; ());
                send!(target, "setLevel:", a.mip as NSUInteger => NSUInteger; ());
                send!(target, "setSlice:", a.layer as NSUInteger => NSUInteger; ());
                let load = match a.load {
                    Load::Clear(_) => ffi::load::CLEAR,
                    Load::Load => ffi::load::LOAD,
                    Load::DontCare => ffi::load::DONT_CARE,
                };
                send!(target, "setLoadAction:", load as NSUInteger => NSUInteger; ());
                let store = if a.store {
                    ffi::store::STORE
                } else {
                    ffi::store::DONT_CARE
                };
                send!(target, "setStoreAction:", store as NSUInteger => NSUInteger; ());
                if let Load::Clear(v) = a.load {
                    if depth {
                        send!(target, "setClearDepth:", f64::from(v[0]) => f64; ());
                    } else {
                        let c = MTLClearColor {
                            red: f64::from(v[0]),
                            green: f64::from(v[1]),
                            blue: f64::from(v[2]),
                            alpha: f64::from(v[3]),
                        };
                        send!(target, "setClearColor:", c => MTLClearColor; ());
                    }
                }
            }
            // A subresource cannot be an attachment and sampled in the same pass.
            for c in body {
                match *c {
                    Command::SetBindGroup(_, g) => {
                        let group = self
                            .groups
                            .get(g.raw())
                            .ok_or_else(|| format!("{g:?} was destroyed"))?;
                        for (_, b) in &group.bound {
                            if let Bound::Texture {
                                id, mips, layers, ..
                            } = b
                            {
                                self.texture(*id)?;
                                if let Some((_, mip, layer)) = attached.iter().find(|(t, m, l)| {
                                    t == id
                                        && (mips.0..mips.1).contains(m)
                                        && (layers.0..layers.1).contains(l)
                                }) {
                                    return Err(format!(
                                        "{id:?} mip {mip} layer {layer} is used as an attachment \
                                         and as a sampled texture in one pass"
                                    ));
                                }
                            }
                        }
                    }
                    Command::SetVertexBuffer(b, _) => {
                        if !self.buffer(b)?.desc.usage.contains(BufferUsage::VERTEX) {
                            return Err(format!("set_vertex_buffer: {b:?} lacks VERTEX usage"));
                        }
                    }
                    Command::SetIndexBuffer(b, _) => {
                        if !self.buffer(b)?.desc.usage.contains(BufferUsage::INDEX) {
                            return Err(format!("set_index_buffer: {b:?} lacks INDEX usage"));
                        }
                    }
                    _ => {}
                }
            }
            let (w, h) = size.expect("at least one attachment");
            let enc = send!(cb, "renderCommandEncoderWithDescriptor:", rp => Id; Id);
            if enc.is_null() {
                return Err("renderCommandEncoderWithDescriptor returned nil".into());
            }
            let full = MTLViewport {
                origin_x: 0.0,
                origin_y: 0.0,
                width: f64::from(w),
                height: f64::from(h),
                znear: 0.0,
                zfar: 1.0,
            };
            send!(enc, "setViewport:", full => MTLViewport; ());
            let result = self.replay_pass(enc, e, body, desc, (w, h));
            send!(enc, "endEncoding"; ());
            result
        }
    }

    fn replay_pass(
        &self,
        enc: Id,
        e: &Encoder,
        body: &[Command],
        desc: &PassDesc,
        (w, h): (u32, u32),
    ) -> Result<()> {
        let pass_colors: Vec<Format> = desc
            .colors
            .iter()
            .map(|a| self.texture(a.texture).map(|t| t.desc.format))
            .collect::<Result<_>>()?;
        let pass_depth = desc
            .depth
            .map(|a| self.texture(a.texture).map(|t| t.desc.format))
            .transpose()?;
        let mut st = DrawState::default();
        for c in body {
            // SAFETY (for every call below): the encoder is recording inside the pass begun by
            // record_pass; all referenced objects were validated as alive.
            unsafe {
                match c {
                    &Command::SetPipeline(id) => {
                        let p = self.pipeline(id)?;
                        if p.compute || p.colors != pass_colors || p.depth != pass_depth {
                            return Err(format!(
                                "set_pipeline: {id:?} renders {:?} + depth {:?}, which does not \
                                 match this pass",
                                p.colors, p.depth
                            ));
                        }
                        if st.pipeline != Some(id) {
                            let r = p.raster.as_ref().ok_or("render pipeline without state")?;
                            send!(enc, "setRenderPipelineState:", p.state.id() => Id; ());
                            send!(enc, "setDepthStencilState:", r.depth_state.id() => Id; ());
                            send!(enc, "setCullMode:", r.cull as NSUInteger => NSUInteger; ());
                            send!(enc, "setFrontFacingWinding:", r.winding as NSUInteger => NSUInteger; ());
                            send!(
                                enc,
                                "setDepthBias:slopeScale:clamp:",
                                r.depth_bias.0 => f32,
                                r.depth_bias.1 => f32,
                                0.0f32 => f32;
                                ()
                            );
                            if self.depth_clamp {
                                send!(enc, "setDepthClipMode:", r.clip as NSUInteger => NSUInteger; ());
                            }
                            st.pipeline = Some(id);
                        }
                    }
                    &Command::SetBindGroup(g, id) => {
                        let slot = st
                            .groups
                            .get_mut(g as usize)
                            .ok_or_else(|| format!("set_bind_group: group {g} is out of range"))?;
                        *slot = Some(id);
                    }
                    &Command::SetVertexBuffer(id, offset) => {
                        let b = self.buffer(id)?;
                        if offset >= b.desc.size {
                            return Err(format!(
                                "set_vertex_buffer: offset {offset} is past the end"
                            ));
                        }
                        send!(
                            enc,
                            "setVertexBuffer:offset:atIndex:",
                            b.obj.id() => Id,
                            offset as NSUInteger => NSUInteger,
                            abe_shader::VERTEX_BUFFER as NSUInteger => NSUInteger;
                            ()
                        );
                        st.vertex = true;
                    }
                    &Command::SetIndexBuffer(id, offset) => {
                        let b = self.buffer(id)?;
                        if offset >= b.desc.size || offset % 4 != 0 {
                            return Err(format!(
                                "set_index_buffer: offset {offset} must be a multiple of 4 inside the buffer"
                            ));
                        }
                        st.index = Some((b.obj.id(), offset));
                    }
                    &Command::SetViewport([x, y, vw, vh]) => {
                        let v = MTLViewport {
                            origin_x: f64::from(x),
                            origin_y: f64::from(y),
                            width: f64::from(vw),
                            height: f64::from(vh),
                            znear: 0.0,
                            zfar: 1.0,
                        };
                        send!(enc, "setViewport:", v => MTLViewport; ());
                    }
                    &Command::SetScissor([x, y, sw, sh]) => {
                        let (x, y) = (x.min(w), y.min(h));
                        let r = MTLScissorRect {
                            x: x as usize,
                            y: y as usize,
                            width: sw.min(w - x) as usize,
                            height: sh.min(h - y) as usize,
                        };
                        send!(enc, "setScissorRect:", r => MTLScissorRect; ());
                    }
                    Command::PushConstants(range) => st.push = Some(range.clone()),
                    &Command::Draw {
                        vertices,
                        instances,
                        first_vertex,
                        first_instance,
                    } => {
                        let primitive = self.bind_for_draw(enc, e, &mut st, false)?;
                        if vertices > 0 && instances > 0 {
                            send!(
                                enc,
                                "drawPrimitives:vertexStart:vertexCount:instanceCount:baseInstance:",
                                primitive as NSUInteger => NSUInteger,
                                first_vertex as NSUInteger => NSUInteger,
                                vertices as NSUInteger => NSUInteger,
                                instances as NSUInteger => NSUInteger,
                                first_instance as NSUInteger => NSUInteger;
                                ()
                            );
                        }
                    }
                    &Command::DrawIndexed {
                        indices,
                        instances,
                        first_index,
                        base_vertex,
                        first_instance,
                    } => {
                        let primitive = self.bind_for_draw(enc, e, &mut st, true)?;
                        let (buffer, offset) = st.index.expect("checked by bind_for_draw");
                        if indices > 0 && instances > 0 {
                            send!(
                                enc,
                                "drawIndexedPrimitives:indexCount:indexType:indexBuffer:indexBufferOffset:instanceCount:baseVertex:baseInstance:",
                                primitive as NSUInteger => NSUInteger,
                                indices as NSUInteger => NSUInteger,
                                ffi::INDEX_UINT32 as NSUInteger => NSUInteger,
                                buffer => Id,
                                (offset + u64::from(first_index) * 4) as NSUInteger => NSUInteger,
                                instances as NSUInteger => NSUInteger,
                                base_vertex as isize => isize,
                                first_instance as NSUInteger => NSUInteger;
                                ()
                            );
                        }
                    }
                    other => return Err(format!("{other:?} is not allowed inside a pass")),
                }
            }
        }
        Ok(())
    }

    /// Binds what the current pipeline needs from the bind groups and push constants and
    /// returns its primitive type.
    fn bind_for_draw(
        &self,
        enc: Id,
        e: &Encoder,
        st: &mut DrawState,
        indexed: bool,
    ) -> Result<u64> {
        let id = st.pipeline.ok_or("draw without set_pipeline")?;
        let p = self.pipeline(id)?;
        if p.vertex_input && !st.vertex {
            return Err(format!("{id:?} reads vertices but no vertex buffer is set"));
        }
        if indexed && st.index.is_none() {
            return Err("draw_indexed without set_index_buffer".into());
        }
        let push = match (&st.push, p.push_size) {
            (_, 0) => None,
            (Some(r), n) if r.len() >= n as usize => Some(&e.bytes[r.start..r.start + n as usize]),
            (Some(r), n) => {
                return Err(format!(
                    "{id:?} uses {n} bytes of push constants but only {} are set",
                    r.len()
                ));
            }
            (None, n) => {
                return Err(format!(
                    "{id:?} uses {n} bytes of push constants but none are set"
                ));
            }
        };
        let groups: Vec<Option<BindGroupId>> = st.groups.to_vec();
        self.bind_groups(p, id, &groups, enc, push, &mut st.bound)?;
        Ok(p.raster
            .as_ref()
            .map_or(ffi::primitive::TRIANGLE, |r| r.primitive))
    }

    /// Resolves bind groups against every stage of `p` and binds them, plus push constants
    /// and buffer sizes.
    fn bind_groups(
        &self,
        p: &Pipeline,
        id: PipelineId,
        groups: &[Option<BindGroupId>],
        enc: Id,
        push: Option<&[u8]>,
        bound: &mut Bindings,
    ) -> Result<()> {
        let mut resolved: Vec<Option<&BindGroup>> = vec![None; p.groups.len()];
        for (g, layout) in p.groups.iter().enumerate() {
            if layout.is_empty() {
                continue;
            }
            let group_id = groups
                .get(g)
                .copied()
                .flatten()
                .ok_or_else(|| format!("{id:?} uses @group({g}) but no bind group is set"))?;
            let group = self
                .groups
                .get(group_id.raw())
                .ok_or_else(|| format!("{group_id:?} was destroyed"))?;
            if group.layout != *layout {
                return Err(format!(
                    "{group_id:?} was created for a different layout than @group({g}) of {id:?}"
                ));
            }
            resolved[g] = Some(group);
        }
        let find = |group: u32, binding: u32| -> Result<&Bound> {
            resolved
                .get(group as usize)
                .copied()
                .flatten()
                .and_then(|g| g.bound.iter().find(|(b, _)| *b == binding))
                .map(|(_, b)| b)
                .ok_or_else(|| format!("@group({group}) @binding({binding}) is not bound"))
        };
        for stage in &p.stages {
            let target = Target {
                enc,
                kind: stage.kind,
            };
            let code = stage_code(stage.kind);
            for slot in &stage.slots {
                // SAFETY: the encoder is recording; every object was checked alive.
                unsafe {
                    match (slot.index, find(slot.group, slot.binding)?) {
                        (MslIndex::Buffer(i), Bound::Buffer { id, offset, .. }) => {
                            let b = self.buffer(*id)?.obj.id();
                            if bound.buffers.get(&(code, i)) != Some(&(b, *offset)) {
                                target.buffer(b, *offset, i);
                                bound.buffers.insert((code, i), (b, *offset));
                            }
                        }
                        (MslIndex::Texture(i), Bound::Texture { id, obj, .. }) => {
                            self.texture(*id)?;
                            if bound.textures.get(&(code, i)) != Some(obj) {
                                target.texture(*obj, i);
                                bound.textures.insert((code, i), *obj);
                            }
                        }
                        (MslIndex::Sampler(i), Bound::Sampler(s)) => {
                            let s = self
                                .samplers
                                .get(s.raw())
                                .ok_or_else(|| format!("{s:?} does not belong to this device"))?
                                .obj
                                .id();
                            if bound.samplers.get(&(code, i)) != Some(&s) {
                                target.sampler(s, i);
                                bound.samplers.insert((code, i), s);
                            }
                        }
                        (index, b) => {
                            return Err(format!(
                                "@group({}) @binding({}): {b:?} cannot bind to {index:?}",
                                slot.group, slot.binding
                            ));
                        }
                    }
                }
            }
            let mut send_bytes = |data: Vec<u8>, index: u32| {
                if bound.bytes.get(&(code, index)) != Some(&data) {
                    // SAFETY: the encoder is recording; Metal copies the bytes.
                    unsafe { target.bytes(&data, index) };
                    bound.bytes.insert((code, index), data);
                }
            };
            if stage.push
                && let Some(data) = push
            {
                send_bytes(data.to_vec(), abe_shader::PUSH_CONSTANT_BUFFER);
            }
            if !stage.sized.is_empty() {
                let mut sizes = Vec::with_capacity(stage.sized.len() * 4);
                for &(group, binding) in &stage.sized {
                    let size = match find(group, binding)? {
                        Bound::Buffer { size, .. } => u32::try_from(*size).unwrap_or(u32::MAX),
                        _ => 0,
                    };
                    sizes.extend_from_slice(&size.to_le_bytes());
                }
                send_bytes(sizes, abe_shader::SIZES_BUFFER);
            }
        }
        Ok(())
    }

    fn record_dispatch(
        &mut self,
        cb: Id,
        id: PipelineId,
        groups: &[(u32, BindGroupId)],
        push: &[u8],
        size: [u32; 3],
    ) -> Result<()> {
        let p = self.pipeline(id)?;
        if !p.compute {
            return Err(format!("dispatch: {id:?} is not a compute pipeline"));
        }
        if size.iter().any(|n| *n > 65535) {
            return Err(format!("dispatch: {size:?} exceeds the device limit"));
        }
        if push.len() < p.push_size as usize {
            return Err(format!(
                "dispatch: {id:?} uses {} bytes of push constants but {} were given",
                p.push_size,
                push.len()
            ));
        }
        let mut chosen = [None; 8];
        for (i, &(g, group)) in groups.iter().enumerate() {
            if groups[..i].iter().any(|(other, _)| *other == g) {
                return Err(format!("dispatch: @group({g}) is given twice"));
            }
            if let Some(slot) = chosen.get_mut(g as usize) {
                *slot = Some(group);
            }
        }
        // SAFETY: a compute encoder on the recording command buffer, ended below.
        unsafe {
            let enc = send!(cb, "computeCommandEncoder"; Id);
            if enc.is_null() {
                return Err("computeCommandEncoder returned nil".into());
            }
            send!(enc, "setComputePipelineState:", p.state.id() => Id; ());
            let push = (p.push_size > 0).then(|| &push[..p.push_size as usize]);
            let result = self.bind_groups(p, id, &chosen, enc, push, &mut Bindings::default());
            if result.is_ok() && size.iter().all(|n| *n > 0) {
                let groups = MTLSize {
                    width: size[0] as usize,
                    height: size[1] as usize,
                    depth: size[2] as usize,
                };
                let threads = MTLSize {
                    width: p.threads[0] as usize,
                    height: p.threads[1] as usize,
                    depth: p.threads[2] as usize,
                };
                send!(
                    enc,
                    "dispatchThreadgroups:threadsPerThreadgroup:",
                    groups => MTLSize,
                    threads => MTLSize;
                    ()
                );
            }
            send!(enc, "endEncoding"; ());
            result
        }
    }

    fn record_texture_copy(
        &mut self,
        cb: Id,
        (src, mip, layer): (TextureId, u32, u32),
        dst: BufferId,
        offset: u64,
        acquired: Option<TextureId>,
    ) -> Result<()> {
        let t = self.usable(src, acquired)?;
        let b = self.buffer(dst)?;
        if !t.desc.usage.contains(TextureUsage::COPY_SRC) {
            return Err(format!(
                "copy_texture_to_buffer: {src:?} lacks COPY_SRC usage"
            ));
        }
        if !b.desc.usage.contains(BufferUsage::COPY_DST) {
            return Err(format!(
                "copy_texture_to_buffer: {dst:?} lacks COPY_DST usage"
            ));
        }
        if mip >= t.desc.mips || layer >= t.desc.layers {
            return Err(format!(
                "copy_texture_to_buffer: mip {mip} layer {layer} is outside {src:?}"
            ));
        }
        let (w, h) = t.desc.mip_size(mip);
        let texel = u64::from(t.desc.format.texel_size());
        let bytes = u64::from(w) * u64::from(h) * texel;
        if offset % texel != 0
            || offset
                .checked_add(bytes)
                .is_none_or(|end| end > b.desc.size)
        {
            return Err(format!(
                "copy_texture_to_buffer: {bytes} bytes at offset {offset} do not fit {dst:?} \
                 (offsets must be multiples of the texel size)"
            ));
        }
        // SAFETY: a blit encoder copying one whole subresource into a large enough buffer.
        // One image is copied, so the image stride is 0 (as in write_texture).
        unsafe {
            let blit = send!(cb, "blitCommandEncoder"; Id);
            send!(
                blit,
                "copyFromTexture:sourceSlice:sourceLevel:sourceOrigin:sourceSize:toBuffer:destinationOffset:destinationBytesPerRow:destinationBytesPerImage:",
                t.obj.id() => Id,
                layer as NSUInteger => NSUInteger,
                mip as NSUInteger => NSUInteger,
                MTLOrigin::default() => MTLOrigin,
                MTLSize { width: w as usize, height: h as usize, depth: 1 } => MTLSize,
                b.obj.id() => Id,
                offset as NSUInteger => NSUInteger,
                (u64::from(w) * texel) as NSUInteger => NSUInteger,
                0usize => NSUInteger;
                ()
            );
            send!(blit, "endEncoding"; ());
        }
        Ok(())
    }

    fn record_buffer_copy(
        &mut self,
        cb: Id,
        (src, src_offset): (BufferId, u64),
        (dst, dst_offset): (BufferId, u64),
        size: u64,
    ) -> Result<()> {
        let s = self.buffer(src)?;
        let d = self.buffer(dst)?;
        if !s.desc.usage.contains(BufferUsage::COPY_SRC) {
            return Err(format!(
                "copy_buffer_to_buffer: {src:?} lacks COPY_SRC usage"
            ));
        }
        if !d.desc.usage.contains(BufferUsage::COPY_DST) {
            return Err(format!(
                "copy_buffer_to_buffer: {dst:?} lacks COPY_DST usage"
            ));
        }
        let fits = |offset: u64, len: u64| offset.checked_add(size).is_some_and(|e| e <= len);
        if !fits(src_offset, s.desc.size) || !fits(dst_offset, d.desc.size) {
            return Err("copy_buffer_to_buffer: range out of bounds".into());
        }
        if src == dst && src_offset < dst_offset + size && dst_offset < src_offset + size {
            return Err("copy_buffer_to_buffer: overlapping ranges in one buffer".into());
        }
        if size == 0 {
            return Ok(());
        }
        // SAFETY: both ranges were bounds-checked.
        unsafe {
            let blit = send!(cb, "blitCommandEncoder"; Id);
            send!(
                blit,
                "copyFromBuffer:sourceOffset:toBuffer:destinationOffset:size:",
                s.obj.id() => Id,
                src_offset as NSUInteger => NSUInteger,
                d.obj.id() => Id,
                dst_offset as NSUInteger => NSUInteger,
                size as NSUInteger => NSUInteger;
                ()
            );
            send!(blit, "endEncoding"; ());
        }
        Ok(())
    }

    fn record_blit(
        &mut self,
        cb: Id,
        (src, src_mip, src_layer): (TextureId, u32, u32),
        (dst, dst_mip, dst_layer): (TextureId, u32, u32),
        filter: Filter,
        acquired: Option<TextureId>,
    ) -> Result<()> {
        let s = self.usable(src, acquired)?;
        let d = self.usable(dst, acquired)?;
        if !s.desc.usage.contains(TextureUsage::COPY_SRC)
            || !d.desc.usage.contains(TextureUsage::COPY_DST)
        {
            return Err("blit: the source needs COPY_SRC and the destination COPY_DST".into());
        }
        if src_mip >= s.desc.mips
            || src_layer >= s.desc.layers
            || dst_mip >= d.desc.mips
            || dst_layer >= d.desc.layers
        {
            return Err("blit: subresource out of range".into());
        }
        if (src, src_mip, src_layer) == (dst, dst_mip, dst_layer) {
            return Err("blit: source and destination are the same subresource".into());
        }
        let (sd, dd) = (s.desc, d.desc);
        if (sd.format.is_depth() || dd.format.is_depth())
            && (sd.format != dd.format || filter != Filter::Nearest)
        {
            return Err("blit: depth blits need identical formats and Nearest filtering".into());
        }
        let same_size = sd.mip_size(src_mip) == dd.mip_size(dst_mip);
        if same_size && sd.format == dd.format {
            let (from, to) = (s.obj.id(), d.obj.id());
            let (w, h) = sd.mip_size(src_mip);
            // SAFETY: a same-size, same-format copy between two live subresources.
            unsafe {
                let blit = send!(cb, "blitCommandEncoder"; Id);
                send!(
                    blit,
                    "copyFromTexture:sourceSlice:sourceLevel:sourceOrigin:sourceSize:toTexture:destinationSlice:destinationLevel:destinationOrigin:",
                    from => Id,
                    src_layer as NSUInteger => NSUInteger,
                    src_mip as NSUInteger => NSUInteger,
                    MTLOrigin::default() => MTLOrigin,
                    MTLSize { width: w as usize, height: h as usize, depth: 1 } => MTLSize,
                    to => Id,
                    dst_layer as NSUInteger => NSUInteger,
                    dst_mip as NSUInteger => NSUInteger,
                    MTLOrigin::default() => MTLOrigin;
                    ()
                );
                send!(blit, "endEncoding"; ());
            }
            return Ok(());
        }
        if sd.format.is_depth() {
            return Err("blit: Metal cannot scale depth textures; blit at the same size".into());
        }
        // Scale or convert by drawing the source over the destination.
        let (pipeline, sampler) = self.blit_pipeline(dd.format, filter)?;
        let view = self.view(
            src,
            ViewKey {
                ty: ffi::texture_type::D2,
                base_mip: src_mip,
                mips: 1,
                base_layer: src_layer,
                layers: 1,
            },
        )?;
        let p = self.pipeline(pipeline)?;
        let state = p.state.id();
        let dst_obj = self.texture(dst)?.obj.id();
        let sampler = self
            .samplers
            .get(sampler.raw())
            .ok_or("blit sampler missing")?
            .obj
            .id();
        let fragment = p
            .stages
            .iter()
            .find(|m| m.kind == StageKind::Fragment)
            .ok_or("blit pipeline without a fragment stage")?
            .clone();
        let descriptor =
            Self::new_object(class!("MTLRenderPassDescriptor"), "MTLRenderPassDescriptor")?;
        let rp = descriptor.id();
        // SAFETY: a render pass over one live subresource drawing one triangle.
        unsafe {
            let a = at_index(send!(rp, "colorAttachments"; Id), 0);
            send!(a, "setTexture:", dst_obj => Id; ());
            send!(a, "setLevel:", dst_mip as NSUInteger => NSUInteger; ());
            send!(a, "setSlice:", dst_layer as NSUInteger => NSUInteger; ());
            send!(a, "setLoadAction:", ffi::load::DONT_CARE as NSUInteger => NSUInteger; ());
            send!(a, "setStoreAction:", ffi::store::STORE as NSUInteger => NSUInteger; ());
            let enc = send!(cb, "renderCommandEncoderWithDescriptor:", rp => Id; Id);
            if enc.is_null() {
                return Err("renderCommandEncoderWithDescriptor returned nil".into());
            }
            send!(enc, "setRenderPipelineState:", state => Id; ());
            let target = Target {
                enc,
                kind: StageKind::Fragment,
            };
            for slot in &fragment.slots {
                match slot.index {
                    MslIndex::Texture(i) => target.texture(view, i),
                    MslIndex::Sampler(i) => target.sampler(sampler, i),
                    MslIndex::Buffer(_) => {}
                }
            }
            send!(
                enc,
                "drawPrimitives:vertexStart:vertexCount:",
                ffi::primitive::TRIANGLE as NSUInteger => NSUInteger,
                0usize => NSUInteger,
                3usize => NSUInteger;
                ()
            );
            send!(enc, "endEncoding"; ());
        }
        Ok(())
    }
}

/// `objectAtIndexedSubscript:` of a descriptor array.
unsafe fn at_index(array: Id, i: usize) -> Id {
    // SAFETY: Metal descriptor arrays create entries on demand.
    unsafe { send!(array, "objectAtIndexedSubscript:", i => NSUInteger; Id) }
}
