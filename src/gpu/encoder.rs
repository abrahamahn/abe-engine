//! A backend-neutral command list. Recording never touches the GPU; misuse is remembered and
//! reported by `Device::submit`/`Device::present`, which also derive every barrier.
use super::types::*;
use core::ops::Range;

#[derive(Clone, Debug)]
pub(crate) enum Command {
    BeginPass(PassDesc),
    EndPass,
    SetPipeline(PipelineId),
    SetBindGroup(u32, BindGroupId),
    SetVertexBuffer(BufferId, u64),
    SetIndexBuffer(BufferId, u64),
    SetViewport([f32; 4]),
    SetScissor([u32; 4]),
    /// Byte range of `Encoder::bytes`.
    PushConstants(Range<usize>),
    Draw {
        vertices: u32,
        instances: u32,
        first_vertex: u32,
        first_instance: u32,
    },
    DrawIndexed {
        indices: u32,
        instances: u32,
        first_index: u32,
        base_vertex: i32,
        first_instance: u32,
    },
    Dispatch {
        pipeline: PipelineId,
        /// Range of `Encoder::groups`.
        groups: Range<usize>,
        /// Range of `Encoder::bytes`.
        push: Range<usize>,
        size: [u32; 3],
    },
    CopyTextureToBuffer {
        src: TextureId,
        mip: u32,
        layer: u32,
        dst: BufferId,
        offset: u64,
    },
    CopyBufferToBuffer {
        src: BufferId,
        src_offset: u64,
        dst: BufferId,
        dst_offset: u64,
        size: u64,
    },
    Blit {
        src: (TextureId, u32, u32),
        dst: (TextureId, u32, u32),
        filter: Filter,
    },
}

/// Records passes, dispatches and copies for one `Device::submit` or `Device::present`.
///
/// Inside a pass (between `begin_pass` and `end_pass`): `set_pipeline`, `set_bind_group`,
/// `set_vertex_buffer`, `set_index_buffer`, `set_viewport`, `set_scissor`, `push_constants`,
/// `draw` and `draw_indexed`. Pipeline, bind group, buffer and push-constant state starts empty
/// in every pass; viewport and scissor start as the full attachment. Bind groups and push
/// constants stay set across `set_pipeline` calls within the pass.
///
/// Outside passes: `dispatch`, `copy_texture_to_buffer`, `copy_buffer_to_buffer` and `blit`.
#[derive(Debug, Default)]
pub struct Encoder {
    pub(crate) commands: Vec<Command>,
    pub(crate) bytes: Vec<u8>,
    pub(crate) groups: Vec<(u32, BindGroupId)>,
    pub(crate) error: Option<String>,
    in_pass: bool,
}

impl Encoder {
    pub fn new() -> Self {
        Self::default()
    }
    /// The first recording error, which `submit`/`present` will return.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    fn fail(&mut self, message: String) {
        if self.error.is_none() {
            self.error = Some(message);
        }
    }
    fn inside(&mut self, what: &str) -> bool {
        if !self.in_pass {
            self.fail(format!("{what} must be recorded inside a pass"));
        }
        self.in_pass
    }
    fn outside(&mut self, what: &str) -> bool {
        if self.in_pass {
            self.fail(format!("{what} must be recorded outside passes"));
        }
        !self.in_pass
    }
    fn data(&mut self, data: &[u8]) -> Range<usize> {
        let start = self.bytes.len();
        self.bytes.extend_from_slice(data);
        start..self.bytes.len()
    }

    pub fn begin_pass(&mut self, d: PassDesc) {
        if self.outside("begin_pass") {
            self.in_pass = true;
            self.commands.push(Command::BeginPass(d));
        }
    }
    pub fn end_pass(&mut self) {
        if self.inside("end_pass") {
            self.in_pass = false;
            self.commands.push(Command::EndPass);
        }
    }
    pub fn set_pipeline(&mut self, p: PipelineId) {
        if self.inside("set_pipeline") {
            self.commands.push(Command::SetPipeline(p));
        }
    }
    pub fn set_bind_group(&mut self, group: u32, g: BindGroupId) {
        if self.inside("set_bind_group") {
            self.commands.push(Command::SetBindGroup(group, g));
        }
    }
    pub fn set_vertex_buffer(&mut self, b: BufferId, offset: u64) {
        if self.inside("set_vertex_buffer") {
            self.commands.push(Command::SetVertexBuffer(b, offset));
        }
    }
    /// 32-bit indices; `offset` must be a multiple of 4.
    pub fn set_index_buffer(&mut self, b: BufferId, offset: u64) {
        if self.inside("set_index_buffer") {
            self.commands.push(Command::SetIndexBuffer(b, offset));
        }
    }
    /// Positive width and height; depth range 0..1.
    pub fn set_viewport(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if self.inside("set_viewport") {
            if !(w > 0.0 && h > 0.0 && x.is_finite() && y.is_finite()) {
                self.fail(format!("invalid viewport {x},{y} {w}x{h}"));
            }
            self.commands.push(Command::SetViewport([x, y, w, h]));
        }
    }
    /// Clamped to the attachment.
    pub fn set_scissor(&mut self, x: u32, y: u32, w: u32, h: u32) {
        if self.inside("set_scissor") {
            self.commands.push(Command::SetScissor([x, y, w, h]));
        }
    }
    /// Replaces the push-constant bytes used by later draws in this pass. Pipelines read the
    /// first `push_constant_size` bytes (rounded up to 4); shorter data is an error at draw.
    pub fn push_constants(&mut self, data: &[u8]) {
        if self.inside("push_constants") {
            let range = self.data(data);
            self.commands.push(Command::PushConstants(range));
        }
    }
    pub fn draw(&mut self, vertices: u32, instances: u32, first_vertex: u32, first_instance: u32) {
        if self.inside("draw") {
            self.commands.push(Command::Draw {
                vertices,
                instances,
                first_vertex,
                first_instance,
            });
        }
    }
    pub fn draw_indexed(
        &mut self,
        indices: u32,
        instances: u32,
        first_index: u32,
        base_vertex: i32,
        first_instance: u32,
    ) {
        if self.inside("draw_indexed") {
            self.commands.push(Command::DrawIndexed {
                indices,
                instances,
                first_index,
                base_vertex,
                first_instance,
            });
        }
    }
    /// One compute dispatch with its own bind groups and push constants. Each dispatch is a
    /// synchronization scope: storage writes of earlier dispatches are visible to it.
    pub fn dispatch(
        &mut self,
        pipeline: PipelineId,
        groups: &[(u32, BindGroupId)],
        push: &[u8],
        x: u32,
        y: u32,
        z: u32,
    ) {
        if self.outside("dispatch") {
            let start = self.groups.len();
            self.groups.extend_from_slice(groups);
            let groups = start..self.groups.len();
            let push = self.data(push);
            self.commands.push(Command::Dispatch {
                pipeline,
                groups,
                push,
                size: [x, y, z],
            });
        }
    }
    /// Copies one whole subresource as tightly packed rows to `dst` at `offset`.
    pub fn copy_texture_to_buffer(
        &mut self,
        src: TextureId,
        mip: u32,
        layer: u32,
        dst: BufferId,
        offset: u64,
    ) {
        if self.outside("copy_texture_to_buffer") {
            self.commands.push(Command::CopyTextureToBuffer {
                src,
                mip,
                layer,
                dst,
                offset,
            });
        }
    }
    pub fn copy_buffer_to_buffer(
        &mut self,
        src: BufferId,
        src_off: u64,
        dst: BufferId,
        dst_off: u64,
        size: u64,
    ) {
        if self.outside("copy_buffer_to_buffer") {
            self.commands.push(Command::CopyBufferToBuffer {
                src,
                src_offset: src_off,
                dst,
                dst_offset: dst_off,
                size,
            });
        }
    }
    /// Scales the whole `(texture, mip, layer)` source onto the whole destination.
    pub fn blit(&mut self, src: (TextureId, u32, u32), dst: (TextureId, u32, u32), filter: Filter) {
        if self.outside("blit") {
            self.commands.push(Command::Blit { src, dst, filter });
        }
    }
    /// Checks the structure before a backend replays it.
    pub(crate) fn finish(&self) -> Result<()> {
        if let Some(e) = &self.error {
            return Err(e.clone());
        }
        if self.in_pass {
            return Err("encoder submitted with an open pass (missing end_pass)".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn misuse_is_reported_at_submission() {
        let buffer = BufferId::from_raw(0);
        let mut e = Encoder::new();
        e.draw(3, 1, 0, 0);
        e.begin_pass(PassDesc::default());
        e.copy_buffer_to_buffer(buffer, 0, buffer, 4, 4);
        assert_eq!(e.error(), Some("draw must be recorded inside a pass"));
        assert!(e.finish().is_err());

        let mut e = Encoder::new();
        e.begin_pass(PassDesc::default());
        assert!(e.finish().unwrap_err().contains("end_pass"));
        e.begin_pass(PassDesc::default());
        assert!(e.error().unwrap().contains("begin_pass"));

        let mut e = Encoder::new();
        e.begin_pass(PassDesc::default());
        e.push_constants(&[1, 2, 3, 4]);
        e.end_pass();
        e.dispatch(PipelineId::from_raw(1), &[], &[5, 6, 7, 8], 1, 1, 1);
        assert!(e.finish().is_ok());
        assert_eq!(e.bytes, [1, 2, 3, 4, 5, 6, 7, 8]);
        assert!(matches!(
            e.commands.last(),
            Some(Command::Dispatch { push, .. }) if *push == (4..8)
        ));
    }
}
