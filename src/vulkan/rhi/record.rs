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
use crate::gpu::{
    BufferUsage, Command, Encoder, Filter, Load, Memory, PassView, Span, TextureUsage, TransferCmd,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Key {
    Buffer(BufferId),
    /// (texture, mip, layer)
    Texture(TextureId, u32, u32),
}
impl Key {
    #[inline]
    fn hash(self) -> usize {
        let raw = match self {
            Key::Buffer(id) => u64::from(id.raw()),
            Key::Texture(id, mip, layer) => {
                (1u64 << 63)
                    | (u64::from(id.raw()) << 31)
                    | (u64::from(layer) << 6)
                    | u64::from(mip)
            }
        };
        let mut h = raw.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        h ^= h >> 30;
        h as usize
    }
}

/// The uses of one synchronization scope, backed by a stamp-cleared flat hash table.
pub(super) struct Scope {
    uses: Vec<(Key, Use)>,
    table: Vec<(u64, Key, u32)>,
    group_stamps: Vec<u64>,
    stamp: u64,
}
impl Default for Scope {
    fn default() -> Self {
        Self {
            uses: Vec::new(),
            table: Vec::new(),
            group_stamps: Vec::new(),
            stamp: 1,
        }
    }
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
    #[inline]
    pub(super) fn clear(&mut self) {
        self.uses.clear();
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            for slot in &mut self.table {
                slot.0 = 0;
            }
            self.group_stamps.fill(0);
            self.stamp = 1;
        }
    }

    #[inline]
    fn mark_group(&mut self, slot: usize) -> bool {
        if slot >= self.group_stamps.len() {
            self.group_stamps.resize(slot + 1, 0);
        }
        if self.group_stamps[slot] == self.stamp {
            return false;
        }
        self.group_stamps[slot] = self.stamp;
        true
    }

    #[inline]
    fn contains(&self, key: Key) -> bool {
        if self.table.is_empty() {
            return false;
        }
        let mask = self.table.len() - 1;
        let mut pos = key.hash() & mask;
        loop {
            let slot = self.table[pos];
            if slot.0 != self.stamp {
                return false;
            }
            if slot.1 == key {
                return true;
            }
            pos = (pos + 1) & mask;
        }
    }

    #[cold]
    fn grow(&mut self) {
        let new_cap = (self.table.len() * 2).max(64);
        let dummy = Key::Buffer(BufferId::from_raw(0));
        self.table.clear();
        self.table.resize(new_cap, (0, dummy, 0));
        let mask = new_cap - 1;
        let stamp = self.stamp;
        for (i, &(key, _)) in self.uses.iter().enumerate() {
            let mut pos = key.hash() & mask;
            while self.table[pos].0 == stamp {
                pos = (pos + 1) & mask;
            }
            self.table[pos] = (stamp, key, i as u32);
        }
    }

    pub fn add(&mut self, key: Key, u: Use) -> Result<()> {
        if (self.uses.len() + 1) * 2 > self.table.len() {
            self.grow();
        }
        let mask = self.table.len() - 1;
        let mut pos = key.hash() & mask;
        loop {
            let slot = self.table[pos];
            if slot.0 != self.stamp {
                let idx = self.uses.len() as u32;
                self.uses.push((key, u));
                self.table[pos] = (self.stamp, key, idx);
                return Ok(());
            }
            if slot.1 == key {
                let i = slot.2 as usize;
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
                return Ok(());
            }
            pos = (pos + 1) & mask;
        }
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

/// Tracking overlay and pending barriers of one command buffer, using flat stamp-indexed arenas.
pub(super) struct Plan {
    stamp: u64,
    tex_lookup: Vec<(u64, u32)>,
    tex_entries: Vec<(u32, u32, u32)>,
    tex_arena: Vec<State>,
    buf_lookup: Vec<(u64, u32)>,
    buf_entries: Vec<(u32, State)>,
    images: Vec<VkImageMemoryBarrier>,
    memory: Vec<VkBufferMemoryBarrier>,
    src: u32,
    dst: u32,
    /// Readback buffers written by this command buffer.
    readback: Vec<BufferId>,
}
impl Default for Plan {
    fn default() -> Self {
        Self {
            stamp: 1,
            tex_lookup: Vec::new(),
            tex_entries: Vec::new(),
            tex_arena: Vec::new(),
            buf_lookup: Vec::new(),
            buf_entries: Vec::new(),
            images: Vec::new(),
            memory: Vec::new(),
            src: 0,
            dst: 0,
            readback: Vec::new(),
        }
    }
}
impl Plan {
    pub(super) fn reset(&mut self) {
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.tex_lookup.fill((0, 0));
            self.buf_lookup.fill((0, 0));
            self.stamp = 1;
        }
        self.tex_entries.clear();
        self.tex_arena.clear();
        self.buf_entries.clear();
        self.images.clear();
        self.memory.clear();
        self.src = 0;
        self.dst = 0;
        self.readback.clear();
    }

    #[inline]
    fn buffer_state(&mut self, raw: u32, initial: State) -> &mut State {
        let slot = (raw & INDEX_MASK) as usize;
        if slot >= self.buf_lookup.len() {
            self.buf_lookup.resize(slot + 1, (0, 0));
        }
        let (st, idx) = self.buf_lookup[slot];
        if st == self.stamp && self.buf_entries[idx as usize].0 == raw {
            return &mut self.buf_entries[idx as usize].1;
        }
        let new_idx = self.buf_entries.len() as u32;
        self.buf_entries.push((raw, initial));
        self.buf_lookup[slot] = (self.stamp, new_idx);
        &mut self.buf_entries[new_idx as usize].1
    }

    #[inline]
    fn get_buffer_mut(&mut self, raw: u32) -> Option<&mut State> {
        let &(st, idx) = self.buf_lookup.get((raw & INDEX_MASK) as usize)?;
        if st == self.stamp && self.buf_entries[idx as usize].0 == raw {
            Some(&mut self.buf_entries[idx as usize].1)
        } else {
            None
        }
    }

    #[inline]
    fn texture_states(&mut self, raw: u32, initial: &[State]) -> &mut [State] {
        let slot = (raw & INDEX_MASK) as usize;
        if slot >= self.tex_lookup.len() {
            self.tex_lookup.resize(slot + 1, (0, 0));
        }
        let (st, idx) = self.tex_lookup[slot];
        if st == self.stamp && self.tex_entries[idx as usize].0 == raw {
            let (_, start, len) = self.tex_entries[idx as usize];
            return &mut self.tex_arena[start as usize..(start + len) as usize];
        }
        let start = self.tex_arena.len() as u32;
        let len = initial.len() as u32;
        self.tex_arena.extend_from_slice(initial);
        let new_idx = self.tex_entries.len() as u32;
        self.tex_entries.push((raw, start, len));
        self.tex_lookup[slot] = (self.stamp, new_idx);
        &mut self.tex_arena[start as usize..(start + len) as usize]
    }

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
        // Coalesce contiguous mip levels (and contiguous array layers once mip ranges match).
        if let Some(last) = self.images.last_mut()
            && last.image == image
            && last.subresourceRange.aspectMask == aspect
            && last.srcAccessMask == b.src_access
            && last.dstAccessMask == b.dst_access
            && last.oldLayout == b.old_layout
            && last.newLayout == b.new_layout
        {
            let r = &mut last.subresourceRange;
            if r.baseArrayLayer == layer
                && r.layerCount == 1
                && r.baseMipLevel + r.levelCount == mip
            {
                r.levelCount += 1;
                let len = self.images.len();
                if len >= 2 {
                    let cur = self.images[len - 1];
                    let prev = &mut self.images[len - 2];
                    if prev.image == cur.image
                        && prev.subresourceRange.aspectMask == cur.subresourceRange.aspectMask
                        && prev.srcAccessMask == cur.srcAccessMask
                        && prev.dstAccessMask == cur.dstAccessMask
                        && prev.oldLayout == cur.oldLayout
                        && prev.newLayout == cur.newLayout
                        && prev.subresourceRange.baseMipLevel == cur.subresourceRange.baseMipLevel
                        && prev.subresourceRange.levelCount == cur.subresourceRange.levelCount
                        && prev.subresourceRange.baseArrayLayer + prev.subresourceRange.layerCount
                            == cur.subresourceRange.baseArrayLayer
                    {
                        prev.subresourceRange.layerCount += cur.subresourceRange.layerCount;
                        self.images.pop();
                    }
                }
                return;
            }
            if r.baseMipLevel == mip
                && r.levelCount == 1
                && r.baseArrayLayer + r.layerCount == layer
            {
                r.layerCount += 1;
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
struct DrawState {
    pipeline: Option<PipelineId>,
    groups: [Option<BindGroupId>; MAX_BIND_GROUPS],
    /// (descriptor set, pipeline layout) actually bound per group.
    bound: [Option<(u64, u64)>; MAX_BIND_GROUPS],
    vertex: Option<(u64, u64)>,
    index: Option<(u64, u64)>,
    push: Option<Span>,
    pushed_span: Option<Span>,
    pushed_layout: u64,
    pushed_stages: u32,
    pushed_len: u32,
    pushed_bytes: [u8; 256],
}
impl Default for DrawState {
    fn default() -> Self {
        Self {
            pipeline: None,
            groups: [None; MAX_BIND_GROUPS],
            bound: [None; MAX_BIND_GROUPS],
            vertex: None,
            index: None,
            push: None,
            pushed_span: None,
            pushed_layout: 0,
            pushed_stages: 0,
            pushed_len: 0,
            pushed_bytes: [0; 256],
        }
    }
}

impl<'w> Gpu<'w> {
    /// Advances the overlay for every use in `scope` and queues the barriers they need.
    pub(super) fn apply(&self, plan: &mut Plan, scope: &Scope) -> Result<()> {
        for &(key, u) in &scope.uses {
            match key {
                Key::Buffer(id) => {
                    let b = self.buffer(id)?;
                    let state = plan.buffer_state(id.raw(), b.state);
                    if let Some(barrier) = state.apply(u) {
                        plan.buffer_barrier(b.handle, barrier);
                    }
                }
                Key::Texture(id, mip, layer) => {
                    let t = self.texture(id)?;
                    let states = plan.texture_states(id.raw(), &t.states);
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
        plan: &mut Plan,
        semaphores: Option<(u64, u64)>,
    ) -> Result<()> {
        for i in 0..plan.readback.len() {
            let id = plan.readback[i];
            let handle = self.buffer(id)?.handle;
            if let Some(state) = plan.get_buffer_mut(id.raw())
                && let Some(b) = state.host_read()
            {
                plan.buffer_barrier(handle, b);
            }
        }
        plan.readback.clear();
        self.flush(cmd, plan);
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
        for &(raw, start, len) in &plan.tex_entries {
            if let Some(t) = self.textures.get_mut(raw) {
                t.states
                    .copy_from_slice(&plan.tex_arena[start as usize..(start + len) as usize]);
                t.last_use = serial;
            }
        }
        for &(raw, state) in &plan.buf_entries {
            if let Some(b) = self.buffers.get_mut(raw) {
                b.state = state;
                b.last_use = serial;
            }
        }
        Ok(())
    }

    pub fn submit(&mut self, e: &Encoder) -> Result<()> {
        e.finish()?;
        self.flush_transfers(false)?;
        if !self.garbage.is_empty() {
            self.collect();
        }
        let cmd = self.begin(IMMEDIATE)?;
        let mut plan = core::mem::take(&mut self.plan);
        plan.reset();
        let res = (|| -> Result<()> {
            self.record(cmd, e, &mut plan, None)?;
            self.execute(IMMEDIATE, cmd, &mut plan, None)?;
            Ok(())
        })();
        self.plan = plan;
        res?;
        self.wait(IMMEDIATE)?;
        if !self.garbage.is_empty() {
            self.collect();
        }
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
            match commands[i] {
                Command::BeginPass {
                    colors,
                    depth,
                    end_cmd,
                } => {
                    let end = if end_cmd != u32::MAX {
                        end_cmd as usize
                    } else {
                        return Err("begin_pass without end_pass".into());
                    };
                    let desc = e.pass(colors, depth);
                    self.record_pass(cmd, e, desc, &commands[i + 1..end], plan, acquired)?;
                    i = end;
                }
                Command::Transfer(idx) => match e.transfers[idx as usize] {
                    TransferCmd::Dispatch {
                        pipeline,
                        groups,
                        push,
                        size,
                    } => self.record_dispatch(
                        cmd,
                        pipeline,
                        &e.groups[groups.range()],
                        &e.bytes[push.range()],
                        size,
                        plan,
                    )?,
                    TransferCmd::CopyTextureToBuffer {
                        src,
                        mip,
                        layer,
                        dst,
                        offset,
                    } => self.record_texture_copy(
                        cmd,
                        (src, mip, layer),
                        dst,
                        offset,
                        plan,
                        acquired,
                    )?,
                    TransferCmd::CopyBufferToBuffer {
                        src,
                        src_offset,
                        dst,
                        dst_offset,
                        size,
                    } => self.record_buffer_copy(
                        cmd,
                        (src, src_offset),
                        (dst, dst_offset),
                        size,
                        plan,
                    )?,
                    TransferCmd::Blit { src, dst, filter } => {
                        self.record_blit(cmd, src, dst, filter, plan, acquired)?
                    }
                },
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
        desc: PassView<'_>,
        body: &[Command],
        plan: &mut Plan,
        acquired: Option<TextureId>,
    ) -> Result<()> {
        if desc.colors.is_empty() && desc.depth.is_none() {
            return Err("begin_pass: a pass needs at least one attachment".into());
        }
        let max_colors = (self.limits.maxColorAttachments as usize).min(MAX_PASS_COLORS);
        if desc.colors.len() > max_colors {
            return Err(format!(
                "begin_pass: {} colour attachments exceed the device limit",
                desc.colors.len()
            ));
        }
        let mut key = PassKey::empty();
        let mut views = [0u64; MAX_PASS_COLORS + 1];
        let mut view_len = 0usize;
        // SAFETY: zeroed VkClearValue is valid (`0.0` float / `0.0` depth, `0` stencil).
        let mut clears = [unsafe { core::mem::zeroed::<VkClearValue>() }; MAX_PASS_COLORS + 1];
        let mut clear_len = 0usize;
        let mut size = None;

        let mut scope = core::mem::take(&mut self.scope);
        scope.clear();
        let scope_res = (|| -> Result<()> {
            let attachments = desc
                .colors
                .iter()
                .map(|a| (a, false))
                .chain(desc.depth.into_iter().map(|a| (a, true)));
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
                if scope.contains(key_attachment) {
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
                clears[clear_len] = if depth {
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
                };
                clear_len += 1;
                if depth {
                    key.depth = Some(ops);
                } else {
                    key.colors[key.color_len as usize] = ops;
                    key.color_len += 1;
                }
                let view_key = ViewKey {
                    ty: 1,
                    base_mip: a.mip,
                    mips: 1,
                    base_layer: a.layer,
                    layers: 1,
                };
                views[view_len] = self.view(a.texture, view_key)?;
                view_len += 1;
            }
            for c in body {
                match *c {
                    Command::SetBindGroup(_, g) => {
                        let group = self.group(g)?;
                        let slot = (g.raw() & INDEX_MASK) as usize;
                        if scope.mark_group(slot) {
                            self.add_bound(&mut scope, group)?;
                        }
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
            self.apply(plan, &scope)
        })();
        self.scope = scope;
        scope_res?;

        self.flush(cmd, plan);
        let (w, h) = size.expect("at least one attachment");
        let pass = self.render_pass(&key)?;
        let framebuffer = self.framebuffer(pass, &views[..view_len], w, h)?;
        let mut info = VkRenderPassBeginInfo::new();
        info.renderPass = pass;
        info.framebuffer = framebuffer;
        info.renderArea = rect(0, 0, w, h);
        info.clearValueCount = clear_len as u32;
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
        let pass_colors = key.colors();
        for c in body {
            // SAFETY (for every call below): cmd is recording inside the render pass begun by
            // record_pass; all referenced objects were validated as alive.
            match *c {
                Command::SetPipeline(id) => {
                    let p = self.pipeline(id)?;
                    let colors_match = p.colors.len() == pass_colors.len()
                        && p.colors
                            .iter()
                            .zip(pass_colors)
                            .all(|(&f, &(pf, _, _))| vk_format(f) == pf);
                    if p.compute
                        || !colors_match
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
                Command::SetBindGroup(g, id) => {
                    let slot = st
                        .groups
                        .get_mut(g as usize)
                        .ok_or_else(|| format!("set_bind_group: group {g} is out of range"))?;
                    *slot = Some(id);
                }
                Command::SetVertexBuffer(id, offset) => {
                    let b = self.buffer(id)?;
                    if offset >= b.desc.size {
                        return Err(format!(
                            "set_vertex_buffer: offset {offset} is past the end"
                        ));
                    }
                    if st.vertex != Some((b.handle, offset)) {
                        unsafe { vkCmdBindVertexBuffers(cmd, 0, 1, &b.handle, &offset) };
                        st.vertex = Some((b.handle, offset));
                    }
                }
                Command::SetIndexBuffer(id, offset) => {
                    let b = self.buffer(id)?;
                    if offset >= b.desc.size || offset % 4 != 0 {
                        return Err(format!(
                            "set_index_buffer: offset {offset} must be a multiple of 4 inside the buffer"
                        ));
                    }
                    if st.index != Some((b.handle, offset)) {
                        unsafe { vkCmdBindIndexBuffer(cmd, b.handle, offset, 1) };
                        st.index = Some((b.handle, offset));
                    }
                }
                Command::SetViewport([x, y, vw, vh]) => {
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
                Command::SetScissor([x, y, sw, sh]) => {
                    let (x, y) = (x.min(w), y.min(h));
                    let scissor = rect(x, y, sw.min(w - x), sh.min(h - y));
                    unsafe { vkCmdSetScissor(cmd, 0, 1, &scissor) };
                }
                Command::PushConstants(span) => st.push = Some(span),
                Command::Draw {
                    vertices,
                    instances,
                    first_vertex,
                    first_instance,
                } => {
                    self.bind_for_draw(cmd, e, &mut st, false)?;
                    unsafe { vkCmdDraw(cmd, vertices, instances, first_vertex, first_instance) };
                }
                Command::DrawIndexed {
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
        if p.vertex_input && st.vertex.is_none() {
            return Err(format!("{id:?} reads vertices but no vertex buffer is set"));
        }
        if indexed && st.index.is_none() {
            return Err("draw_indexed without set_index_buffer".into());
        }
        let mut run_sets = [0u64; MAX_BIND_GROUPS];
        let mut run_start = 0u32;
        let mut run_len = 0usize;
        for (g, layout) in p.groups.iter().enumerate() {
            if layout.entries.is_empty() {
                if run_len > 0 {
                    // SAFETY: inside the active render pass; every set in `run_sets` was validated.
                    unsafe {
                        vkCmdBindDescriptorSets(
                            cmd,
                            0,
                            p.layout,
                            run_start,
                            run_len as u32,
                            run_sets.as_ptr(),
                            0,
                            null(),
                        );
                    }
                    run_len = 0;
                }
                continue;
            }
            let group_id = st.groups[g]
                .ok_or_else(|| format!("{id:?} uses @group({g}) but no bind group is set"))?;
            let group = self.group(group_id)?;
            if group.layout_id != layout.id {
                return Err(format!(
                    "{group_id:?} was created for a different layout than @group({g}) of {id:?}"
                ));
            }
            if st.bound[g] != Some((group.set, p.layout)) {
                if run_len == 0 {
                    run_start = g as u32;
                }
                run_sets[run_len] = group.set;
                run_len += 1;
                st.bound[g] = Some((group.set, p.layout));
            } else if run_len > 0 {
                // SAFETY: inside the active render pass; every set in `run_sets` was validated.
                unsafe {
                    vkCmdBindDescriptorSets(
                        cmd,
                        0,
                        p.layout,
                        run_start,
                        run_len as u32,
                        run_sets.as_ptr(),
                        0,
                        null(),
                    );
                }
                run_len = 0;
            }
        }
        if run_len > 0 {
            // SAFETY: inside the active render pass; every set in `run_sets` was validated.
            unsafe {
                vkCmdBindDescriptorSets(
                    cmd,
                    0,
                    p.layout,
                    run_start,
                    run_len as u32,
                    run_sets.as_ptr(),
                    0,
                    null(),
                );
            }
        }
        if p.push_size > 0 {
            let span = st.push.ok_or_else(|| {
                format!(
                    "{id:?} uses {} bytes of push constants but none are set",
                    p.push_size
                )
            })?;
            if span.len() < p.push_size as usize {
                return Err(format!(
                    "{id:?} uses {} bytes of push constants but only {} are set",
                    p.push_size,
                    span.len()
                ));
            }
            let push_len = p.push_size as usize;
            let bytes = &e.bytes[span.start as usize..span.start as usize + push_len];
            let same = st.pushed_layout == p.layout
                && st.pushed_stages == p.push_stages
                && st.pushed_len == p.push_size
                && (st.pushed_span == Some(span)
                    || (push_len <= st.pushed_bytes.len()
                        && &st.pushed_bytes[..push_len] == bytes));
            if !same {
                // SAFETY: `bytes` holds `p.push_size` bytes of the encoder's push-constant buffer.
                unsafe {
                    vkCmdPushConstants(
                        cmd,
                        p.layout,
                        p.push_stages,
                        0,
                        p.push_size,
                        bytes.as_ptr().cast(),
                    )
                };
                st.pushed_layout = p.layout;
                st.pushed_stages = p.push_stages;
                st.pushed_len = p.push_size;
                st.pushed_span = Some(span);
                if push_len <= st.pushed_bytes.len() {
                    st.pushed_bytes[..push_len].copy_from_slice(bytes);
                }
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
        let mut chosen = [None; MAX_BIND_GROUPS];
        for (i, &(g, group)) in groups.iter().enumerate() {
            if groups[..i].iter().any(|(other, _)| *other == g) {
                return Err(format!("dispatch: @group({g}) is given twice"));
            }
            if (g as usize) < p.groups.len() {
                chosen[g as usize] = Some(group);
            }
        }
        let mut sets = [(0u32, 0u64); MAX_BIND_GROUPS];
        let mut set_len = 0usize;
        let mut scope = core::mem::take(&mut self.scope);
        scope.clear();
        let (handle, layout, push_size, push_stages) = {
            let res = (|| -> Result<(u64, u64, u32, u32)> {
                let p = self.pipeline(id)?;
                for (g, gl) in p.groups.iter().enumerate() {
                    if gl.entries.is_empty() {
                        continue;
                    }
                    let group_id =
                        chosen[g].ok_or_else(|| format!("dispatch: {id:?} needs @group({g})"))?;
                    let group = self.group(group_id)?;
                    if group.layout_id != gl.id {
                        return Err(format!(
                            "{group_id:?} was created for a different layout than @group({g}) of {id:?}"
                        ));
                    }
                    self.add_bound(&mut scope, group)?;
                    sets[set_len] = (g as u32, group.set);
                    set_len += 1;
                }
                let out = (p.handle, p.layout, p.push_size, p.push_stages);
                self.apply(plan, &scope)?;
                Ok(out)
            })();
            self.scope = scope;
            res?
        };
        self.flush(cmd, plan);
        // SAFETY: cmd is recording outside a render pass; the pipeline, layout and sets are
        // alive and compatible (checked above); push holds at least push_size bytes.
        unsafe {
            vkCmdBindPipeline(cmd, 1, handle);
            for &(g, set) in &sets[..set_len] {
                vkCmdBindDescriptorSets(cmd, 1, layout, g, 1, &set, 0, null());
            }
            if push_size > 0 {
                vkCmdPushConstants(cmd, layout, push_stages, 0, push_size, push.as_ptr().cast());
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
        let mut scope = core::mem::take(&mut self.scope);
        scope.clear();
        let res = (|| -> Result<()> {
            scope.add(
                Key::Texture(src, mip, layer),
                Use::new(track::TRANSFER_SRC, stage::TRANSFER, access::TRANSFER_READ),
            )?;
            scope.add(
                Key::Buffer(dst),
                Use::buffer(stage::TRANSFER, access::TRANSFER_WRITE),
            )?;
            self.apply(plan, &scope)
        })();
        self.scope = scope;
        res?;
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
        let mut scope = core::mem::take(&mut self.scope);
        scope.clear();
        let res = (|| -> Result<()> {
            scope.add(
                Key::Buffer(src),
                Use::buffer(stage::TRANSFER, access::TRANSFER_READ),
            )?;
            scope.add(
                Key::Buffer(dst),
                Use::buffer(stage::TRANSFER, access::TRANSFER_WRITE),
            )?;
            self.apply(plan, &scope)
        })();
        self.scope = scope;
        res?;
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
        let mut scope = core::mem::take(&mut self.scope);
        scope.clear();
        let res = (|| -> Result<()> {
            scope.add(
                Key::Texture(src, src_mip, src_layer),
                Use::new(track::TRANSFER_SRC, stage::TRANSFER, access::TRANSFER_READ),
            )?;
            scope.add(
                Key::Texture(dst, dst_mip, dst_layer),
                Use::new(track::TRANSFER_DST, stage::TRANSFER, access::TRANSFER_WRITE).discarding(),
            )?;
            self.apply(plan, &scope)
        })();
        self.scope = scope;
        res?;
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
