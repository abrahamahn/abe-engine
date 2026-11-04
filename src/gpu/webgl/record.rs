//! Replays a backend-neutral [`Encoder`] into the command stream `web/ganbate.js` executes on
//! WebGL 2: `u32` words, one opcode followed by its operands. Recording checks what GL would
//! otherwise report as errors (formats, usages, attachment/binding feedback loops, missing
//! bindings), resolves each draw's uniform blocks, texture units, storage textures and hidden
//! uniforms against the bound groups, and emits only the GL bindings that changed since the
//! start of the submission. Push constants go to a ring of uniform blocks, shared when equal.
//!
//! Opcodes and operands (floats are their bits):
//! * `BEGIN_PASS` colours, has_depth, then per colour (texture, mip, layer, load, store, r, g,
//!   b, a) and for depth (texture, mip, layer, load, store, clear). load: 0 load, 1 clear,
//!   2 don't care; store: 0 discard, 1 store.
//! * `END_PASS`
//! * `SET_PIPELINE` pipeline: its program and fixed-function state.
//! * `VIEWPORT` x, y, w, h · `SCISSOR` x, y, w, h
//! * `BIND_UBO` point, buffer, offset, size · `PUSH_RANGE` offset, size (at [`PUSH_POINT`])
//! * `BIND_UNIT` unit, texture, sampler (0: the host's nearest sampler), base mip, mip count
//! * `BIND_STORAGE` unit, buffer (its data texture)
//! * `SET_HIDDEN` index, value (a hidden uniform of the current pipeline's program)
//! * `BIND_VAO` vertex array
//! * `DRAW` vertices, instances, first vertex · `DRAW_INDEXED` indices, instances, first index
//! * `COPY_BUFFER` src, src offset, dst, dst offset, size
//! * `COPY_TEXTURE` texture, mip, layer, width, height, buffer, offset
//! * `BLIT` src texture, mip, layer, dst texture, mip, layer, filter (0 nearest, 1 linear)
use super::ffi::SURFACE;
use super::*;
use crate::gpu::{Command, Encoder, Load, PassDesc};
use core::ops::Range;

const BEGIN_PASS: u32 = 1;
const END_PASS: u32 = 2;
const SET_PIPELINE: u32 = 3;
const VIEWPORT: u32 = 4;
const SCISSOR: u32 = 5;
const BIND_UBO: u32 = 6;
const PUSH_RANGE: u32 = 7;
const BIND_UNIT: u32 = 8;
const BIND_STORAGE: u32 = 9;
const SET_HIDDEN: u32 = 10;
const BIND_VAO: u32 = 11;
const DRAW: u32 = 12;
const DRAW_INDEXED: u32 = 13;
const COPY_BUFFER: u32 = 14;
const COPY_TEXTURE: u32 = 15;
const BLIT: u32 = 16;

/// Marks a texture unit holding a storage buffer's data texture in [`Host::units`].
const STORAGE_UNIT: u32 = 1 << 31;

/// Push-constant blocks of one submission.
#[derive(Default)]
pub(super) struct Ring {
    pub bytes: Vec<u8>,
    offsets: HashMap<Vec<u8>, u32>,
    slot: u32,
}
impl Ring {
    fn new(align: u32) -> Self {
        Self {
            slot: PUSH_SLOT.next_multiple_of(align.max(1)),
            ..Self::default()
        }
    }
    /// The offset of a block holding `data`, zero-padded to the slot size.
    fn place(&mut self, data: &[u8]) -> u32 {
        if let Some(&o) = self.offsets.get(data) {
            return o;
        }
        let offset = self.bytes.len() as u32;
        self.bytes.extend_from_slice(data);
        self.bytes.resize((offset + self.slot) as usize, 0);
        self.offsets.insert(data.to_vec(), offset);
        offset
    }
}

/// The GL bindings this submission's stream has made so far.
#[derive(Default)]
struct Host {
    pipeline: Option<u32>,
    /// Uniform block binding point → (buffer, offset, size); the ring is buffer 0.
    points: HashMap<u32, (u32, u32, u32)>,
    /// Texture unit → (texture or `STORAGE_UNIT | buffer`, sampler).
    units: HashMap<u32, (u32, u32)>,
    /// Texture → (base mip, mip count) last applied.
    levels: HashMap<u32, (u32, u32)>,
    vao: Option<u32>,
    /// (program, hidden index) → value.
    hidden: HashMap<(u32, u32), u32>,
}

/// Bind state of one render pass.
#[derive(Default)]
struct DrawState {
    pipeline: Option<PipelineId>,
    groups: [Option<BindGroupId>; MAX_GROUPS],
    vertex: Option<(BufferId, u64)>,
    index: Option<(BufferId, u64)>,
    push: Option<Range<usize>>,
    checked: Vec<BindGroupId>,
}

/// One draw's parameters.
struct DrawCall {
    indexed: bool,
    base_vertex: i32,
    first_instance: u32,
}

/// Whether two (first, end) ranges overlap.
fn overlap(a: (u32, u32), b: (u32, u32)) -> bool {
    a.0 < b.1 && b.0 < a.1
}

impl Gpu<'_> {
    /// A texture the current submission may use: the canvas texture only in `present`.
    fn usable(&self, id: TextureId, acquired: Option<TextureId>) -> Result<&Texture> {
        let t = self.texture(id)?;
        if t.surface && acquired != Some(id) {
            return Err(format!(
                "{id:?} is the canvas texture; only present() may use it, after acquire()"
            ));
        }
        Ok(t)
    }

    /// Checks that every resource a bind group references is alive.
    fn check_bound(&self, group: &BindGroup) -> Result<()> {
        for (_, b) in &group.entries {
            match *b {
                Bound::Buffer { id, .. } => {
                    self.buffer(id)?;
                }
                Bound::Texture { id, .. } => {
                    self.texture(id)?;
                }
                Bound::Sampler(id) => {
                    self.sampler(id)?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn record(
        &self,
        e: &Encoder,
        acquired: Option<TextureId>,
    ) -> Result<(Vec<u32>, Vec<u8>)> {
        e.finish()?;
        let mut out = Vec::with_capacity(e.commands.len() * 6);
        let mut ring = Ring::new(self.info.uniform_align);
        let mut host = Host::default();
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
                    let body = &commands[i + 1..end];
                    self.record_pass(e, desc, body, &mut out, &mut ring, &mut host, acquired)?;
                    i = end;
                }
                Command::Dispatch { .. } => {
                    return Err("dispatch: WebGL 2 has no compute shaders".into());
                }
                &Command::CopyTextureToBuffer {
                    src,
                    mip,
                    layer,
                    dst,
                    offset,
                } => {
                    self.record_texture_copy((src, mip, layer), dst, offset, &mut out, acquired)?
                }
                &Command::CopyBufferToBuffer {
                    src,
                    src_offset,
                    dst,
                    dst_offset,
                    size,
                } => {
                    self.record_buffer_copy((src, src_offset), (dst, dst_offset), size, &mut out)?
                }
                &Command::Blit { src, dst, filter } => {
                    self.record_blit(src, dst, filter, &mut out, acquired)?;
                    // The host's blit program changes the program, units and vertex array.
                    host = Host::default();
                }
                other => return Err(format!("{other:?} recorded outside a pass")),
            }
            i += 1;
        }
        Ok((out, ring.bytes))
    }

    #[allow(clippy::too_many_arguments)]
    fn record_pass(
        &self,
        e: &Encoder,
        desc: &PassDesc,
        body: &[Command],
        out: &mut Vec<u32>,
        ring: &mut Ring,
        host: &mut Host,
        acquired: Option<TextureId>,
    ) -> Result<()> {
        if desc.colors.is_empty() && desc.depth.is_none() {
            return Err("begin_pass: a pass needs at least one attachment".into());
        }
        if desc.colors.len() > self.info.max_draw_buffers as usize {
            return Err(format!(
                "begin_pass: {} colour attachments exceed the device limit",
                desc.colors.len()
            ));
        }
        let mut size = None;
        let mut attached: Vec<(TextureId, u32, u32)> = Vec::new();
        let mut colors = Vec::new();
        let mut depth_format = None;
        let mut words = vec![
            BEGIN_PASS,
            desc.colors.len() as u32,
            u32::from(desc.depth.is_some()),
        ];
        let attachments = desc
            .colors
            .iter()
            .map(|a| (a, false))
            .chain(desc.depth.iter().map(|a| (a, true)));
        for (a, depth) in attachments {
            let t = self.usable(a.texture, acquired)?;
            let need = if depth {
                TextureUsage::DEPTH_TARGET
            } else {
                TextureUsage::COLOR_TARGET
            };
            if depth != t.desc.format.is_depth() || !t.desc.usage.contains(need) {
                return Err(format!(
                    "begin_pass: {:?} ({:?}, {:?}) cannot be a {} attachment",
                    a.texture,
                    t.desc.format,
                    t.desc.usage,
                    if depth { "depth" } else { "colour" }
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
            let (load, clear) = match a.load {
                Load::Load => (0, [0.0; 4]),
                Load::Clear(v) => (1, v),
                Load::DontCare => (2, [0.0; 4]),
            };
            words.extend([t.js, a.mip, a.layer, load, u32::from(a.store)]);
            if depth {
                words.push(clear[0].clamp(0.0, 1.0).to_bits());
                depth_format = Some(t.desc.format);
            } else {
                words.extend(clear.map(f32::to_bits));
                colors.push(t.desc.format);
            }
        }
        out.extend(words);
        let (w, h) = size.expect("at least one attachment");
        let mut st = DrawState::default();
        for c in body {
            match c {
                &Command::SetPipeline(id) => {
                    let p = self.pipeline(id)?;
                    if p.colors != colors || p.depth != depth_format {
                        return Err(format!(
                            "set_pipeline: {id:?} renders {:?} + depth {:?}, which does not match \
                             this pass",
                            p.colors, p.depth
                        ));
                    }
                    st.pipeline = Some(id);
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
                    if !b.desc.usage.contains(BufferUsage::VERTEX) {
                        return Err(format!("set_vertex_buffer: {id:?} lacks VERTEX usage"));
                    }
                    if offset >= b.desc.size || offset % 4 != 0 {
                        return Err(format!(
                            "set_vertex_buffer: offset {offset} must be a multiple of 4 inside the buffer"
                        ));
                    }
                    st.vertex = Some((id, offset));
                }
                &Command::SetIndexBuffer(id, offset) => {
                    let b = self.buffer(id)?;
                    if !b.desc.usage.contains(BufferUsage::INDEX) {
                        return Err(format!("set_index_buffer: {id:?} lacks INDEX usage"));
                    }
                    if offset >= b.desc.size || offset % 4 != 0 {
                        return Err(format!(
                            "set_index_buffer: offset {offset} must be a multiple of 4 inside the buffer"
                        ));
                    }
                    st.index = Some((id, offset));
                }
                &Command::SetViewport(v) => {
                    out.push(VIEWPORT);
                    out.extend(v.map(f32::to_bits));
                }
                &Command::SetScissor([x, y, sw, sh]) => {
                    let (x, y) = (x.min(w), y.min(h));
                    out.extend([SCISSOR, x, y, sw.min(w - x), sh.min(h - y)]);
                }
                Command::PushConstants(range) => st.push = Some(range.clone()),
                &Command::Draw {
                    vertices,
                    instances,
                    first_vertex,
                    first_instance,
                } => {
                    let call = DrawCall {
                        indexed: false,
                        base_vertex: 0,
                        first_instance,
                    };
                    self.bind_for_draw(e, &mut st, &call, &attached, out, ring, host)?;
                    out.extend([DRAW, vertices, instances, first_vertex]);
                }
                &Command::DrawIndexed {
                    indices,
                    instances,
                    first_index,
                    base_vertex,
                    first_instance,
                } => {
                    let call = DrawCall {
                        indexed: true,
                        base_vertex,
                        first_instance,
                    };
                    self.bind_for_draw(e, &mut st, &call, &attached, out, ring, host)?;
                    let (ib, offset) = st.index.expect("checked by bind_for_draw");
                    let first = offset / 4 + u64::from(first_index);
                    let count = self.buffer(ib)?.desc.size / 4;
                    if first + u64::from(indices) > count {
                        return Err(format!(
                            "draw_indexed: indices {first}..{} are outside {ib:?}",
                            first + u64::from(indices)
                        ));
                    }
                    out.extend([DRAW_INDEXED, indices, instances, first as u32]);
                }
                other => return Err(format!("{other:?} is not allowed inside a pass")),
            }
        }
        out.push(END_PASS);
        Ok(())
    }

    /// The resource bound at `(group, binding)`.
    fn entry(&self, st: &DrawState, (group, binding): (u32, u32)) -> Result<Bound> {
        let gid = st
            .groups
            .get(group as usize)
            .copied()
            .flatten()
            .ok_or_else(|| format!("the pipeline uses @group({group}) but no bind group is set"))?;
        self.group(gid)?
            .entries
            .iter()
            .find(|(b, _)| *b == binding)
            .map(|(_, r)| *r)
            .ok_or_else(|| format!("{gid:?} has no @binding({binding})"))
    }

    /// Emits what the current pipeline needs for one draw: the pipeline, uniform blocks, push
    /// constants, texture units, storage textures, hidden uniforms and vertex array.
    #[allow(clippy::too_many_arguments)]
    fn bind_for_draw(
        &self,
        e: &Encoder,
        st: &mut DrawState,
        call: &DrawCall,
        attached: &[(TextureId, u32, u32)],
        out: &mut Vec<u32>,
        ring: &mut Ring,
        host: &mut Host,
    ) -> Result<()> {
        let id = st.pipeline.ok_or("draw without set_pipeline")?;
        let p = self.pipeline(id)?;
        if p.vertex_input && st.vertex.is_none() {
            return Err(format!("{id:?} reads vertices but no vertex buffer is set"));
        }
        if call.indexed && st.index.is_none() {
            return Err("draw_indexed without set_index_buffer".into());
        }
        for (slot, layout) in p.groups.iter().enumerate() {
            if layout.is_empty() {
                continue;
            }
            let gid = st.groups[slot]
                .ok_or_else(|| format!("{id:?} uses @group({slot}) but no bind group is set"))?;
            let g = self.group(gid)?;
            if g.layout.as_ref() != Some(layout) {
                return Err(format!(
                    "{gid:?} was created for a different layout than @group({slot}) of {id:?}"
                ));
            }
            if !st.checked.contains(&gid) {
                self.check_bound(g)?;
                st.checked.push(gid);
            }
        }
        if host.pipeline != Some(p.js) {
            out.extend([SET_PIPELINE, p.js]);
            host.pipeline = Some(p.js);
        }
        let program = &self.programs[p.program];
        for b in &program.blocks {
            let (buffer, offset, size) = match b.binding {
                None => {
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
                    let data = &e.bytes[range.start..range.start + p.push_size as usize];
                    (0, ring.place(data), b.size.next_multiple_of(16))
                }
                Some(at) => match self.entry(st, at)? {
                    Bound::Buffer { id, offset, size } => {
                        (self.buffer(id)?.js, offset as u32, size as u32)
                    }
                    other => return Err(format!("{at:?} holds {other:?}, not a uniform buffer")),
                },
            };
            if host.points.get(&b.point) != Some(&(buffer, offset, size)) {
                if b.binding.is_none() {
                    out.extend([PUSH_RANGE, offset, size]);
                } else {
                    out.extend([BIND_UBO, b.point, buffer, offset, size]);
                }
                host.points.insert(b.point, (buffer, offset, size));
            }
        }
        let mut viewed: Vec<(u32, (u32, u32))> = Vec::new();
        for u in &program.units {
            let Bound::Texture {
                id: tid,
                mips,
                layers,
            } = self.entry(st, u.texture)?
            else {
                return Err(format!("{:?} does not hold a texture", u.texture));
            };
            if attached.iter().any(|&(t, m, l)| {
                t == tid && overlap(mips, (m, m + 1)) && overlap(layers, (l, l + 1))
            }) {
                return Err(format!(
                    "{tid:?} is used as an attachment and as a bound texture in one pass"
                ));
            }
            let t = self.texture(tid)?;
            if u.view == TextureView::D2Array && !t.array {
                return Err(format!("{tid:?} is not an array texture"));
            }
            if let Some(&(_, m)) = viewed.iter().find(|(js, _)| *js == t.js)
                && m != mips
            {
                return Err(format!(
                    "{tid:?} is viewed with two mip ranges in one draw, which WebGL 2 cannot do"
                ));
            }
            viewed.push((t.js, mips));
            let sampler = match u.sampler {
                Some(at) => match self.entry(st, at)? {
                    Bound::Sampler(s) => self.sampler(s)?.js,
                    other => return Err(format!("{at:?} holds {other:?}, not a sampler")),
                },
                None => 0,
            };
            let levels = (mips.0, mips.1 - mips.0);
            if host.units.get(&u.unit) != Some(&(t.js, sampler))
                || host.levels.get(&t.js) != Some(&levels)
            {
                out.extend([BIND_UNIT, u.unit, t.js, sampler, levels.0, levels.1]);
                host.units.insert(u.unit, (t.js, sampler));
                host.levels.insert(t.js, levels);
            }
        }
        for s in &program.storage {
            let Bound::Buffer { id: bid, .. } = self.entry(st, s.binding)? else {
                return Err(format!("{:?} does not hold a storage buffer", s.binding));
            };
            let js = self.buffer(bid)?.js;
            if host.units.get(&s.unit) != Some(&(STORAGE_UNIT | js, 0)) {
                out.extend([BIND_STORAGE, s.unit, js]);
                host.units.insert(s.unit, (STORAGE_UNIT | js, 0));
            }
        }
        for (index, h) in program.hidden.iter().enumerate() {
            let value = match *h {
                HiddenValue::BaseVertex => call.base_vertex as u32,
                HiddenValue::BaseInstance => call.first_instance,
                HiddenValue::StorageOffset(g, b) => match self.entry(st, (g, b))? {
                    Bound::Buffer { offset, .. } => offset as u32,
                    _ => return Err(format!("({g}, {b}) does not hold a buffer")),
                },
                HiddenValue::StorageSize(g, b) => match self.entry(st, (g, b))? {
                    Bound::Buffer { size, .. } => size as u32,
                    _ => return Err(format!("({g}, {b}) does not hold a buffer")),
                },
                HiddenValue::TextureLevels(g, b) => match self.entry(st, (g, b))? {
                    Bound::Texture { mips, .. } => mips.1 - mips.0,
                    _ => return Err(format!("({g}, {b}) does not hold a texture")),
                },
            };
            let key = (program.js, index as u32);
            if host.hidden.get(&key) != Some(&value) {
                out.extend([SET_HIDDEN, index as u32, value]);
                host.hidden.insert(key, value);
            }
        }
        // The vertex array: the pipeline's layout over the vertex buffer, shifted by the base
        // vertex (WebGL 2 has no base-vertex draws), with the index buffer.
        let index = match (call.indexed, st.index) {
            (true, Some((ib, _))) => self.buffer(ib)?.js,
            _ => 0,
        };
        let (buffer, offset) = match (p.vertex_input, st.vertex) {
            (true, Some((vb, offset))) => {
                let shifted = offset as i64 + i64::from(call.base_vertex) * i64::from(p.stride);
                if shifted < 0 || shifted >= self.buffer(vb)?.desc.size as i64 {
                    return Err(format!(
                        "base vertex {} moves the vertices outside {vb:?}",
                        call.base_vertex
                    ));
                }
                (self.buffer(vb)?.js, shifted as u32)
            }
            _ => (0, 0),
        };
        let key = (p.layout, buffer, offset, index);
        let cached = self.vaos.borrow().get(&key).copied();
        let vao = match cached {
            Some(v) => v,
            None => {
                let v = ffi::create_vao(p.js, buffer, offset, index)?;
                self.vaos.borrow_mut().insert(key, v);
                v
            }
        };
        if host.vao != Some(vao) {
            out.extend([BIND_VAO, vao]);
            host.vao = Some(vao);
        }
        Ok(())
    }

    fn record_texture_copy(
        &self,
        (src, mip, layer): (TextureId, u32, u32),
        dst: BufferId,
        offset: u64,
        out: &mut Vec<u32>,
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
        if b.desc.memory != Memory::Readback {
            return Err(
                "copy_texture_to_buffer: WebGL 2 copies textures into readback buffers".into(),
            );
        }
        if t.desc.format.is_depth() {
            return Err("copy_texture_to_buffer: WebGL 2 cannot read depth textures back".into());
        }
        if mip >= t.desc.mips || layer >= t.desc.layers {
            return Err(format!(
                "copy_texture_to_buffer: mip {mip} layer {layer} is outside {src:?}"
            ));
        }
        let (w, h) = t.desc.mip_size(mip);
        let bytes = u64::from(w * t.desc.format.texel_size()) * u64::from(h);
        if offset % 4 != 0
            || offset
                .checked_add(bytes)
                .is_none_or(|end| end > b.desc.size)
        {
            return Err(format!(
                "copy_texture_to_buffer: {bytes} bytes at offset {offset} do not fit {dst:?}"
            ));
        }
        out.extend([COPY_TEXTURE, t.js, mip, layer, w, h, b.js, offset as u32]);
        Ok(())
    }

    fn record_buffer_copy(
        &self,
        (src, src_offset): (BufferId, u64),
        (dst, dst_offset): (BufferId, u64),
        size: u64,
        out: &mut Vec<u32>,
    ) -> Result<()> {
        let s = self.buffer(src)?;
        let d = self.buffer(dst)?;
        if !s.desc.usage.contains(BufferUsage::COPY_SRC) || s.desc.memory == Memory::Readback {
            return Err(format!(
                "copy_buffer_to_buffer: {src:?} lacks COPY_SRC usage"
            ));
        }
        if !d.desc.usage.contains(BufferUsage::COPY_DST) {
            return Err(format!(
                "copy_buffer_to_buffer: {dst:?} lacks COPY_DST usage"
            ));
        }
        if s.index != d.index && d.desc.memory != Memory::Readback {
            return Err(
                "copy_buffer_to_buffer: WebGL 2 copies index buffers only into index buffers"
                    .into(),
            );
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
        out.extend([
            COPY_BUFFER,
            s.js,
            src_offset as u32,
            d.js,
            dst_offset as u32,
            size as u32,
        ]);
        Ok(())
    }

    fn record_blit(
        &self,
        (src, src_mip, src_layer): (TextureId, u32, u32),
        (dst, dst_mip, dst_layer): (TextureId, u32, u32),
        filter: Filter,
        out: &mut Vec<u32>,
        acquired: Option<TextureId>,
    ) -> Result<()> {
        let s = self.usable(src, acquired)?;
        let d = self.usable(dst, acquired)?;
        if !s.desc.usage.contains(TextureUsage::COPY_SRC)
            || !d.desc.usage.contains(TextureUsage::COPY_DST) && !d.surface
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
        if src == dst && src_mip == dst_mip {
            return Err("blit: WebGL 2 cannot blit between layers of one mip".into());
        }
        if s.desc.format.is_depth() || d.desc.format.is_depth() {
            return Err("blit: WebGL 2 blits colour textures only".into());
        }
        if filter == Filter::Linear && is_float32(s.desc.format) && !self.info.float_linear {
            return Err(format!(
                "blit: {:?} cannot be filtered linearly on this device",
                s.desc.format
            ));
        }
        let dst_js = if d.surface { SURFACE } else { d.js };
        let src_js = if s.surface { SURFACE } else { s.js };
        out.extend([
            BLIT,
            src_js,
            src_mip,
            src_layer,
            dst_js,
            dst_mip,
            dst_layer,
            u32::from(filter == Filter::Linear),
        ]);
        Ok(())
    }
}
