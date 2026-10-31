//! Replays a backend-neutral [`Encoder`] into a Vulkan command buffer, deriving barriers.
//!
//! Each pass, dispatch, copy and blit is a *scope*. Before recording a scope, every resource it
//! uses is gathered into a [`Scope`] (merging repeated uses of one subresource and rejecting
//! incompatible ones), the per-subresource [`State`]s are advanced on a [`Plan`] overlay, and the
//! resulting image and buffer barriers are emitted as one `vkCmdPipelineBarrier`. The overlay
//! is committed to the persistent states only after the command buffer was submitted, so a
//! failed recording leaves the tracking untouched.
use super::resources::{
    FEATURE_BLIT_DST, FEATURE_BLIT_SRC, FEATURE_LINEAR, blit_region, vk_filter, vk_format,
};
use super::track::{Barrier, access, stage};
use super::*;
use crate::gpu::{BufferUsage, Command, Encoder, Filter, Load, Memory, PassDesc, TextureUsage};
use core::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Key {
    Buffer(BufferId),
    /// (texture, mip, layer)
    Texture(TextureId, u32, u32),
}

/// The uses of one synchronization scope.
#[derive(Default)]
pub(super) struct Scope {
    uses: Vec<(Key, Use)>,
    index: HashMap<Key, usize>,
}
fn describe(layout: i32) -> &'static str {
    match layout {
        track::COLOR_ATTACHMENT => "a colour attachment",
        track::DEPTH_ATTACHMENT => "a depth attachment",
        track::SHADER_READ_ONLY => "a sampled texture",
        track::GENERAL => "a storage texture",
        track::TRANSFER_SRC => "a copy source",
        track::TRANSFER_DST => "a copy destination",
        _ => "another use",
    }
}
impl Scope {
    pub fn add(&mut self, key: Key, u: Use) -> Result<()> {
        match self.index.get(&key) {
            Some(&i) => {
                let previous = self.uses[i].1;
                if !self.uses[i].1.merge(u) {
                    let Key::Texture(id, mip, layer) = key else {
                        unreachable!("buffers have no layouts to disagree on")
                    };
                    return Err(format!(
                        "{id:?} mip {mip} layer {layer} is used as {} and as {} in one pass",
                        describe(previous.layout),
                        describe(u.layout)
                    ));
                }
            }
            None => {
                self.index.insert(key, self.uses.len());
                self.uses.push((key, u));
            }
        }
        Ok(())
    }
    pub fn add_texture(
        &mut self,
        id: TextureId,
        mips: (u32, u32),
        layers: (u32, u32),
        u: Use,
    ) -> Result<()> {
        for layer in layers.0..layers.1 {
            for mip in mips.0..mips.1 {
                self.add(Key::Texture(id, mip, layer), u)?;
            }
        }
        Ok(())
    }
}

/// Tracking overlay and pending barriers of one command buffer.
#[derive(Default)]
pub(super) struct Plan {
    textures: HashMap<u32, Vec<State>>,
    buffers: HashMap<u32, State>,
    images: Vec<VkImageMemoryBarrier>,
    memory: Vec<VkBufferMemoryBarrier>,
    src: u32,
    dst: u32,
    /// Readback buffers written by this command buffer.
    readback: Vec<BufferId>,
}
impl Plan {
    fn buffer_barrier(&mut self, buffer: u64, b: Barrier) {
        let mut m = VkBufferMemoryBarrier::new();
        m.srcAccessMask = b.src_access;
        m.dstAccessMask = b.dst_access;
        m.srcQueueFamilyIndex = u32::MAX;
        m.dstQueueFamilyIndex = u32::MAX;
        m.buffer = buffer;
        m.size = u64::MAX; // VK_WHOLE_SIZE
        self.memory.push(m);
        self.src |= b.src_stages;
        self.dst |= b.dst_stages;
    }
    fn image_barrier(&mut self, image: u64, aspect: u32, mip: u32, layer: u32, b: Barrier) {
        self.src |= b.src_stages;
        self.dst |= b.dst_stages;
        // Extend the previous barrier when this is the next mip of the same layer.
        if let Some(last) = self.images.last_mut() {
            let r = &mut last.subresourceRange;
            if last.image == image
                && r.baseArrayLayer == layer
                && r.layerCount == 1
                && r.baseMipLevel + r.levelCount == mip
                && last.srcAccessMask == b.src_access
                && last.dstAccessMask == b.dst_access
                && last.oldLayout == b.old_layout
                && last.newLayout == b.new_layout
            {
                r.levelCount += 1;
                return;
            }
        }
        let mut m = VkImageMemoryBarrier::new();
        m.srcAccessMask = b.src_access;
        m.dstAccessMask = b.dst_access;
        m.oldLayout = b.old_layout;
        m.newLayout = b.new_layout;
        m.srcQueueFamilyIndex = u32::MAX;
        m.dstQueueFamilyIndex = u32::MAX;
        m.image = image;
        m.subresourceRange = VkImageSubresourceRange {
            aspectMask: aspect,
            baseMipLevel: mip,
            levelCount: 1,
            baseArrayLayer: layer,
            layerCount: 1,
        };
        self.images.push(m);
    }
}

/// Bindings currently recorded inside a render pass.
#[derive(Default)]
struct DrawState {
    pipeline: Option<PipelineId>,
    groups: [Option<BindGroupId>; MAX_BIND_GROUPS],
    /// (descriptor set, pipeline layout) actually bound per group.
    bound: [Option<(u64, u64)>; MAX_BIND_GROUPS],
    vertex: bool,
    index: bool,
    push: Option<Range<usize>>,
    pushed: Option<(Range<usize>, u64)>,
}

impl<'w> Gpu<'w> {
    /// Advances the overlay for every use in `scope` and queues the barriers they need.
    pub(super) fn apply(&self, plan: &mut Plan, scope: &Scope) -> Result<()> {
        for &(key, u) in &scope.uses {
            match key {
                Key::Buffer(id) => {
                    let b = self.buffer(id)?;
                    let state = plan.buffers.entry(id.raw()).or_insert(b.state);
                    if let Some(barrier) = state.apply(u) {
                        plan.buffer_barrier(b.handle, barrier);
                    }
                }
                Key::Texture(id, mip, layer) => {
                    let t = self.texture(id)?;
                    let states = plan
                        .textures
                        .entry(id.raw())
                        .or_insert_with(|| t.states.clone());
                    let i = (layer * t.desc.mips + mip) as usize;
                    if let Some(barrier) = states[i].apply(u) {
                        plan.image_barrier(t.image, t.aspect, mip, layer, barrier);
                    }
                }
            }
        }
        Ok(())
    }

    /// Records the queued barriers as one pipeline barrier.
    pub(super) fn flush(&self, cmd: Handle, plan: &mut Plan) {
        if plan.images.is_empty() && plan.memory.is_empty() {
            return;
        }
        let src = if plan.src == 0 { stage::TOP } else { plan.src };
        // SAFETY: cmd is recording outside a render pass; the barrier arrays outlive the call
        // and reference live resources.
        unsafe {
            vkCmdPipelineBarrier(
                cmd,
                src,
                plan.dst,
                0,
                0,
                null(),
                plan.memory.len() as u32,
                plan.memory.as_ptr(),
                plan.images.len() as u32,
                plan.images.as_ptr().cast(),
            );
        }
        plan.images.clear();
        plan.memory.clear();
        plan.src = 0;
        plan.dst = 0;
    }

    /// Waits for submission `which`, then resets and begins its command buffer.
    pub(super) fn begin(&mut self, which: usize) -> Result<Handle> {
        self.wait(which)?;
        let cmd = self.subs[which].cmd;
        let mut begin = VkCommandBufferBeginInfo::new();
        begin.flags = 1; // ONE_TIME_SUBMIT
        // SAFETY: the command buffer's previous submission has completed (waited above).
        unsafe {
            check(vkResetCommandBuffer(cmd, 0), "vkResetCommandBuffer")?;
            check(vkBeginCommandBuffer(cmd, &begin), "vkBeginCommandBuffer")?;
        }
        Ok(cmd)
    }

    /// Makes readback writes host-visible, ends and submits `cmd`, and commits the tracking.
    /// `semaphores` = (wait on acquire, signal for present).
    pub(super) fn execute(
        &mut self,
        which: usize,
        cmd: Handle,
        mut plan: Plan,
        semaphores: Option<(u64, u64)>,
    ) -> Result<()> {
        for id in core::mem::take(&mut plan.readback) {
            let handle = self.buffer(id)?.handle;
            if let Some(state) = plan.buffers.get_mut(&id.raw()) {
                if let Some(b) = state.host_read() {
                    plan.buffer_barrier(handle, b);
                }
            }
        }
        self.flush(cmd, &mut plan);
        let sub = &self.subs[which];
        let wait_stages = stage::COLOR_OUTPUT | stage::TRANSFER;
        let mut si = VkSubmitInfo::new();
        si.commandBufferCount = 1;
        si.pCommandBuffers = &cmd;
        if let Some((wait, signal)) = &semaphores {
            si.waitSemaphoreCount = 1;
            si.pWaitSemaphores = wait;
            si.pWaitDstStageMask = &wait_stages;
            si.signalSemaphoreCount = 1;
            si.pSignalSemaphores = signal;
        }
        // SAFETY: the fence is unsignalled-or-idle (its last submission was waited for in
        // `begin`), and everything the command buffer references stays alive until the fence
        // signals: destruction is deferred through the garbage list.
        unsafe {
            check(vkEndCommandBuffer(cmd), "vkEndCommandBuffer")?;
            check(vkResetFences(self.device, 1, &sub.fence), "vkResetFences")?;
            check(
                vkQueueSubmit(self.queue, 1, &si, sub.fence),
                "vkQueueSubmit",
            )?;
        }
        self.serial += 1;
        sub.pending.set(Some(self.serial));
        let serial = self.serial;
        for (raw, states) in plan.textures {
            if let Some(t) = self.textures.get_mut(raw) {
                t.states = states;
                t.last_use = serial;
            }
        }
        for (raw, state) in plan.buffers {
            if let Some(b) = self.buffers.get_mut(raw) {
                b.state = state;
                b.last_use = serial;
            }
        }
        Ok(())
    }

    pub fn submit(&mut self, e: Encoder) -> Result<()> {
        e.finish()?;
        self.collect();
        let cmd = self.begin(IMMEDIATE)?;
        let mut plan = Plan::default();
        self.record(cmd, &e, &mut plan, None)?;
        self.execute(IMMEDIATE, cmd, plan, None)?;
        self.wait(IMMEDIATE)?;
        self.collect();
        Ok(())
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

    fn add_bound(&self, scope: &mut Scope, group: &BindGroup) -> Result<()> {
        for b in &group.bound {
            match *b {
                Bound::Buffer { id, usage } => {
                    self.buffer(id)?;
                    scope.add(Key::Buffer(id), usage)?;
                }
                Bound::Texture {
                    id,
                    mips,
                    layers,
                    usage,
                } => {
                    self.texture(id)?;
                    scope.add_texture(id, mips, layers, usage)?;
                }
                Bound::Sampler(id) => {
                    self.samplers
                        .get(id.raw())
                        .ok_or_else(|| format!("{id:?} does not belong to this device"))?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn record(
        &mut self,
        cmd: Handle,
        e: &Encoder,
        plan: &mut Plan,
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
                    self.record_pass(cmd, e, desc, &commands[i + 1..end], plan, acquired)?;
                    i = end;
                }
                Command::Dispatch {
                    pipeline,
                    groups,
                    push,
                    size,
                } => self.record_dispatch(
                    cmd,
                    *pipeline,
                    &e.groups[groups.clone()],
                    &e.bytes[push.clone()],
                    *size,
                    plan,
                )?,
                &Command::CopyTextureToBuffer {
                    src,
                    mip,
                    layer,
                    dst,
                    offset,
                } => {
                    self.record_texture_copy(cmd, (src, mip, layer), dst, offset, plan, acquired)?
                }
                &Command::CopyBufferToBuffer {
                    src,
                    src_offset,
                    dst,
                    dst_offset,
                    size,
                } => {
                    self.record_buffer_copy(cmd, (src, src_offset), (dst, dst_offset), size, plan)?
                }
                &Command::Blit { src, dst, filter } => {
                    self.record_blit(cmd, src, dst, filter, plan, acquired)?
                }
                other => return Err(format!("{other:?} recorded outside a pass")),
            }
            i += 1;
        }
        Ok(())
    }

    fn record_pass(
        &mut self,
        cmd: Handle,
        e: &Encoder,
        desc: &PassDesc,
        body: &[Command],
        plan: &mut Plan,
        acquired: Option<TextureId>,
    ) -> Result<()> {
        if desc.colors.is_empty() && desc.depth.is_none() {
            return Err("begin_pass: a pass needs at least one attachment".into());
        }
        if desc.colors.len() > self.limits.maxColorAttachments as usize {
            return Err(format!(
                "begin_pass: {} colour attachments exceed the device limit",
                desc.colors.len()
            ));
        }
        let mut scope = Scope::default();
        let mut key = PassKey {
            colors: Vec::new(),
            depth: None,
        };
        let mut views = Vec::new();
        let mut clears = Vec::new();
        let mut size = None;
        let attachments = desc
            .colors
            .iter()
            .map(|a| (a, false))
            .chain(desc.depth.iter().map(|a| (a, true)));
        for (a, depth) in attachments {
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
            let ops = (t.format, load_op(a.load), if a.store { 0 } else { 1 });
            let key_attachment = Key::Texture(a.texture, a.mip, a.layer);
            if scope.index.contains_key(&key_attachment) {
                return Err("begin_pass: the same subresource is attached twice".into());
            }
            let usage = if depth {
                Use::new(
                    track::DEPTH_ATTACHMENT,
                    stage::EARLY_TESTS | stage::LATE_TESTS,
                    access::DEPTH_READ | access::DEPTH_WRITE,
                )
            } else {
                Use::new(
                    track::COLOR_ATTACHMENT,
                    stage::COLOR_OUTPUT,
                    access::COLOR_READ | access::COLOR_WRITE,
                )
            };
            let usage = if a.load == Load::Load {
                usage
            } else {
                usage.discarding()
            };
            scope.add(key_attachment, usage)?;
            let clear = match a.load {
                Load::Clear(v) => v,
                _ => [0.0; 4],
            };
            clears.push(if depth {
                VkClearValue {
                    depthStencil: VkClearDepthStencilValue {
                        depth: clear[0],
                        stencil: 0,
                    },
                }
            } else {
                VkClearValue {
                    color: VkClearColorValue { float32: clear },
                }
            });
            if depth {
                key.depth = Some(ops);
            } else {
                key.colors.push(ops);
            }
            let view_key = ViewKey {
                ty: 1,
                base_mip: a.mip,
                mips: 1,
                base_layer: a.layer,
                layers: 1,
            };
            views.push(self.view(a.texture, view_key)?);
        }
        for c in body {
            match *c {
                Command::SetBindGroup(_, g) => {
                    let group = self.group(g)?;
                    self.add_bound(&mut scope, group)?;
                }
                Command::SetVertexBuffer(b, _) => {
                    if !self.buffer(b)?.desc.usage.contains(BufferUsage::VERTEX) {
                        return Err(format!("set_vertex_buffer: {b:?} lacks VERTEX usage"));
                    }
                    scope.add(
                        Key::Buffer(b),
                        Use::buffer(stage::VERTEX_INPUT, access::VERTEX_READ),
                    )?;
                }
                Command::SetIndexBuffer(b, _) => {
                    if !self.buffer(b)?.desc.usage.contains(BufferUsage::INDEX) {
                        return Err(format!("set_index_buffer: {b:?} lacks INDEX usage"));
                    }
                    scope.add(
                        Key::Buffer(b),
                        Use::buffer(stage::VERTEX_INPUT, access::INDEX_READ),
                    )?;
                }
                _ => {}
            }
        }
        self.apply(plan, &scope)?;
        self.flush(cmd, plan);
        let (w, h) = size.expect("at least one attachment");
        let pass = self.render_pass(&key)?;
        let framebuffer = self.framebuffer(pass, &views, w, h)?;
        let mut info = VkRenderPassBeginInfo::new();
        info.renderPass = pass;
        info.framebuffer = framebuffer;
        info.renderArea = rect(0, 0, w, h);
        info.clearValueCount = clears.len() as u32;
        info.pClearValues = clears.as_ptr();
        let viewport = VkViewport {
            x: 0.0,
            y: 0.0,
            width: w as f32,
            height: h as f32,
            minDepth: 0.0,
            maxDepth: 1.0,
        };
        // SAFETY: the pass, framebuffer and clear values are alive; cmd is recording outside
        // a render pass.
        unsafe {
            vkCmdBeginRenderPass(cmd, &info, 0);
            vkCmdSetViewport(cmd, 0, 1, &viewport);
            vkCmdSetScissor(cmd, 0, 1, &rect(0, 0, w, h));
        }
        self.replay_pass(cmd, e, body, &key, (w, h))?;
        // SAFETY: the render pass begun above is still active.
        unsafe { vkCmdEndRenderPass(cmd) };
        Ok(())
    }

    fn replay_pass(
        &self,
        cmd: Handle,
        e: &Encoder,
        body: &[Command],
        key: &PassKey,
        (w, h): (u32, u32),
    ) -> Result<()> {
        let mut st = DrawState::default();
        for c in body {
            // SAFETY (for every call below): cmd is recording inside the render pass begun by
            // record_pass; all referenced objects were validated as alive.
            match c {
                &Command::SetPipeline(id) => {
                    let p = self.pipeline(id)?;
                    let colors: Vec<i32> = p.colors.iter().map(|f| vk_format(*f)).collect();
                    let pass_colors: Vec<i32> = key.colors.iter().map(|c| c.0).collect();
                    if p.compute
                        || colors != pass_colors
                        || p.depth.map(vk_format) != key.depth.map(|d| d.0)
                    {
                        return Err(format!(
                            "set_pipeline: {id:?} renders {:?} + depth {:?}, which does not match \
                             this pass",
                            p.colors, p.depth
                        ));
                    }
                    if st.pipeline != Some(id) {
                        unsafe { vkCmdBindPipeline(cmd, 0, p.handle) };
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
                    unsafe { vkCmdBindVertexBuffers(cmd, 0, 1, &b.handle, &offset) };
                    st.vertex = true;
                }
                &Command::SetIndexBuffer(id, offset) => {
                    let b = self.buffer(id)?;
                    if offset >= b.desc.size || offset % 4 != 0 {
                        return Err(format!(
                            "set_index_buffer: offset {offset} must be a multiple of 4 inside the buffer"
                        ));
                    }
                    unsafe { vkCmdBindIndexBuffer(cmd, b.handle, offset, 1) };
                    st.index = true;
                }
                &Command::SetViewport([x, y, vw, vh]) => {
                    let viewport = VkViewport {
                        x,
                        y,
                        width: vw,
                        height: vh,
                        minDepth: 0.0,
                        maxDepth: 1.0,
                    };
                    unsafe { vkCmdSetViewport(cmd, 0, 1, &viewport) };
                }
                &Command::SetScissor([x, y, sw, sh]) => {
                    let (x, y) = (x.min(w), y.min(h));
                    let scissor = rect(x, y, sw.min(w - x), sh.min(h - y));
                    unsafe { vkCmdSetScissor(cmd, 0, 1, &scissor) };
                }
                Command::PushConstants(range) => st.push = Some(range.clone()),
                &Command::Draw {
                    vertices,
                    instances,
                    first_vertex,
                    first_instance,
                } => {
                    self.bind_for_draw(cmd, e, &mut st, false)?;
                    unsafe { vkCmdDraw(cmd, vertices, instances, first_vertex, first_instance) };
                }
                &Command::DrawIndexed {
                    indices,
                    instances,
                    first_index,
                    base_vertex,
                    first_instance,
                } => {
                    self.bind_for_draw(cmd, e, &mut st, true)?;
                    unsafe {
                        vkCmdDrawIndexed(
                            cmd,
                            indices,
                            instances,
                            first_index,
                            base_vertex,
                            first_instance,
                        )
                    };
                }
                other => return Err(format!("{other:?} is not allowed inside a pass")),
            }
        }
        Ok(())
    }

    /// Binds whatever descriptor sets and push constants the current pipeline still needs.
    fn bind_for_draw(
        &self,
        cmd: Handle,
        e: &Encoder,
        st: &mut DrawState,
        indexed: bool,
    ) -> Result<()> {
        let id = st.pipeline.ok_or("draw without set_pipeline")?;
        let p = self.pipeline(id)?;
        if p.vertex_input && !st.vertex {
            return Err(format!("{id:?} reads vertices but no vertex buffer is set"));
        }
        if indexed && !st.index {
            return Err("draw_indexed without set_index_buffer".into());
        }
        for (g, layout) in p.groups.iter().enumerate() {
            if layout.entries.is_empty() {
                continue;
            }
            let group_id = st.groups[g]
                .ok_or_else(|| format!("{id:?} uses @group({g}) but no bind group is set"))?;
            let group = self.group(group_id)?;
            if group.layout != *layout {
                return Err(format!(
                    "{group_id:?} was created for a different layout than @group({g}) of {id:?}"
                ));
            }
            if st.bound[g] != Some((group.set, p.layout)) {
                // SAFETY: inside the active render pass; the set matches the layout (checked).
                unsafe {
                    vkCmdBindDescriptorSets(cmd, 0, p.layout, g as u32, 1, &group.set, 0, null())
                };
                st.bound[g] = Some((group.set, p.layout));
            }
        }
        if p.push_size > 0 {
            let range = st.push.clone().ok_or_else(|| {
                format!(
                    "{id:?} uses {} bytes of push constants but none are set",
                    p.push_size
                )
            })?;
            if range.len() < p.push_size as usize {
                return Err(format!(
                    "{id:?} uses {} bytes of push constants but only {} are set",
                    p.push_size,
                    range.len()
                ));
            }
            if st.pushed != Some((range.clone(), p.layout)) {
                // SAFETY: the range holds at least push_size bytes of the encoder's data.
                unsafe {
                    vkCmdPushConstants(
                        cmd,
                        p.layout,
                        p.push_stages,
                        0,
                        p.push_size,
                        e.bytes[range.start..].as_ptr().cast(),
                    )
                };
                st.pushed = Some((range, p.layout));
            }
        }
        Ok(())
    }

    fn record_dispatch(
        &mut self,
        cmd: Handle,
        id: PipelineId,
        groups: &[(u32, BindGroupId)],
        push: &[u8],
        size: [u32; 3],
        plan: &mut Plan,
    ) -> Result<()> {
        let p = self.pipeline(id)?;
        if !p.compute {
            return Err(format!("dispatch: {id:?} is not a compute pipeline"));
        }
        let max = self.limits.maxComputeWorkGroupCount;
        if size.iter().zip(max).any(|(n, m)| *n > m) {
            return Err(format!(
                "dispatch: {size:?} exceeds the device limit {max:?}"
            ));
        }
        if push.len() < p.push_size as usize {
            return Err(format!(
                "dispatch: {id:?} uses {} bytes of push constants but {} were given",
                p.push_size,
                push.len()
            ));
        }
        // Groups the pipeline does not use are ignored, like unused bindings.
        let mut chosen = vec![None; p.groups.len()];
        for (i, &(g, group)) in groups.iter().enumerate() {
            if groups[..i].iter().any(|(other, _)| *other == g) {
                return Err(format!("dispatch: @group({g}) is given twice"));
            }
            if let Some(slot) = chosen.get_mut(g as usize) {
                *slot = Some(group);
            }
        }
        let mut scope = Scope::default();
        let mut sets = Vec::new();
        for (g, layout) in p.groups.iter().enumerate() {
            if layout.entries.is_empty() {
                continue;
            }
            let group_id =
                chosen[g].ok_or_else(|| format!("dispatch: {id:?} needs @group({g})"))?;
            let group = self.group(group_id)?;
            if group.layout != *layout {
                return Err(format!(
                    "{group_id:?} was created for a different layout than @group({g}) of {id:?}"
                ));
            }
            self.add_bound(&mut scope, group)?;
            sets.push((g as u32, group.set));
        }
        self.apply(plan, &scope)?;
        self.flush(cmd, plan);
        // SAFETY: cmd is recording outside a render pass; the pipeline, layout and sets are
        // alive and compatible (checked above); push holds at least push_size bytes.
        unsafe {
            vkCmdBindPipeline(cmd, 1, p.handle);
            for (g, set) in sets {
                vkCmdBindDescriptorSets(cmd, 1, p.layout, g, 1, &set, 0, null());
            }
            if p.push_size > 0 {
                vkCmdPushConstants(
                    cmd,
                    p.layout,
                    p.push_stages,
                    0,
                    p.push_size,
                    push.as_ptr().cast(),
                );
            }
            vkCmdDispatch(cmd, size[0], size[1], size[2]);
        }
        Ok(())
    }

    fn record_texture_copy(
        &mut self,
        cmd: Handle,
        (src, mip, layer): (TextureId, u32, u32),
        dst: BufferId,
        offset: u64,
        plan: &mut Plan,
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
        let region = VkBufferImageCopy {
            bufferOffset: offset,
            bufferRowLength: 0,
            bufferImageHeight: 0,
            imageSubresource: VkImageSubresourceLayers {
                aspectMask: t.aspect,
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
        let (image, buffer, readback) = (t.image, b.handle, b.desc.memory == Memory::Readback);
        let mut scope = Scope::default();
        scope.add(
            Key::Texture(src, mip, layer),
            Use::new(track::TRANSFER_SRC, stage::TRANSFER, access::TRANSFER_READ),
        )?;
        scope.add(
            Key::Buffer(dst),
            Use::buffer(stage::TRANSFER, access::TRANSFER_WRITE),
        )?;
        self.apply(plan, &scope)?;
        self.flush(cmd, plan);
        // SAFETY: the subresource is in TRANSFER_SRC_OPTIMAL and the region fits the buffer.
        unsafe { vkCmdCopyImageToBuffer(cmd, image, track::TRANSFER_SRC, buffer, 1, &region) };
        if readback && !plan.readback.contains(&dst) {
            plan.readback.push(dst);
        }
        Ok(())
    }

    fn record_buffer_copy(
        &mut self,
        cmd: Handle,
        (src, src_offset): (BufferId, u64),
        (dst, dst_offset): (BufferId, u64),
        size: u64,
        plan: &mut Plan,
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
        let (from, to, readback) = (s.handle, d.handle, d.desc.memory == Memory::Readback);
        let mut scope = Scope::default();
        scope.add(
            Key::Buffer(src),
            Use::buffer(stage::TRANSFER, access::TRANSFER_READ),
        )?;
        scope.add(
            Key::Buffer(dst),
            Use::buffer(stage::TRANSFER, access::TRANSFER_WRITE),
        )?;
        self.apply(plan, &scope)?;
        self.flush(cmd, plan);
        let region = VkBufferCopy {
            srcOffset: src_offset,
            dstOffset: dst_offset,
            size,
        };
        // SAFETY: both ranges were bounds-checked; the barrier above orders the copy.
        unsafe { vkCmdCopyBuffer(cmd, from, to, 1, &region) };
        if readback && !plan.readback.contains(&dst) {
            plan.readback.push(dst);
        }
        Ok(())
    }

    fn record_blit(
        &mut self,
        cmd: Handle,
        (src, src_mip, src_layer): (TextureId, u32, u32),
        (dst, dst_mip, dst_layer): (TextureId, u32, u32),
        filter: Filter,
        plan: &mut Plan,
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
        let mut need_src = FEATURE_BLIT_SRC;
        if filter == Filter::Linear {
            need_src |= FEATURE_LINEAR;
        }
        if self.format_features(s.format) & need_src != need_src
            || self.format_features(d.format) & FEATURE_BLIT_DST == 0
        {
            return Err(format!(
                "blit: {:?} -> {:?} with {filter:?} filtering is unsupported",
                s.desc.format, d.desc.format
            ));
        }
        if (s.desc.format.is_depth() || d.desc.format.is_depth())
            && (s.desc.format != d.desc.format || filter != Filter::Nearest)
        {
            return Err("blit: depth blits need identical formats and Nearest filtering".into());
        }
        let region = blit_region(
            &s.desc,
            s.aspect,
            (src_mip, src_layer, 1),
            &d.desc,
            (dst_mip, dst_layer),
        );
        let (from, to) = (s.image, d.image);
        let mut scope = Scope::default();
        scope.add(
            Key::Texture(src, src_mip, src_layer),
            Use::new(track::TRANSFER_SRC, stage::TRANSFER, access::TRANSFER_READ),
        )?;
        scope.add(
            Key::Texture(dst, dst_mip, dst_layer),
            Use::new(track::TRANSFER_DST, stage::TRANSFER, access::TRANSFER_WRITE).discarding(),
        )?;
        self.apply(plan, &scope)?;
        self.flush(cmd, plan);
        // SAFETY: both subresources are in their transfer layouts; regions are in range.
        unsafe {
            vkCmdBlitImage(
                cmd,
                from,
                track::TRANSFER_SRC,
                to,
                track::TRANSFER_DST,
                1,
                &region,
                vk_filter(filter),
            )
        };
        Ok(())
    }
}

fn load_op(load: Load) -> i32 {
    match load {
        Load::Load => 0,
        Load::Clear(_) => 1,
        Load::DontCare => 2,
    }
}
fn rect(x: u32, y: u32, w: u32, h: u32) -> VkRect2D {
    VkRect2D {
        offset: VkOffset2D {
            x: x as i32,
            y: y as i32,
        },
        extent: VkExtent2D {
            width: w,
            height: h,
        },
    }
}
