//! Hazard tracking for one texture subresource or one buffer. Pure logic: no Vulkan calls.
//!
//! A [`State`] remembers the layout, the stages/accesses of the last write (layout transitions
//! count as writes performed in the stages that requested them), which stages and accesses have
//! already been made aware of that write, and which stages read since it. [`State::apply`]
//! returns the barrier a new [`Use`] needs and advances the state, following the classic
//! "simple Vulkan synchronization" rules: read-after-write needs an execution and memory
//! dependency unless that write is already visible to the reader, write-after-read needs an
//! execution dependency, write-after-write and layout transitions need both.

// VkImageLayout values.
pub(super) const UNDEFINED: i32 = 0;
pub(super) const GENERAL: i32 = 1;
pub(super) const COLOR_ATTACHMENT: i32 = 2;
pub(super) const DEPTH_ATTACHMENT: i32 = 3;
pub(super) const SHADER_READ_ONLY: i32 = 5;
pub(super) const TRANSFER_SRC: i32 = 6;
pub(super) const TRANSFER_DST: i32 = 7;
#[cfg_attr(target_os = "android", allow(dead_code))] // presentation is desktop-only
pub(super) const PRESENT_SRC: i32 = 1_000_001_002;

/// VkPipelineStageFlagBits.
pub(super) mod stage {
    pub const TOP: u32 = 0x1;
    pub const VERTEX_INPUT: u32 = 0x4;
    pub const VERTEX_SHADER: u32 = 0x8;
    pub const FRAGMENT_SHADER: u32 = 0x80;
    pub const EARLY_TESTS: u32 = 0x100;
    pub const LATE_TESTS: u32 = 0x200;
    pub const COLOR_OUTPUT: u32 = 0x400;
    pub const COMPUTE: u32 = 0x800;
    pub const TRANSFER: u32 = 0x1000;
    #[cfg_attr(target_os = "android", allow(dead_code))]
    pub const BOTTOM: u32 = 0x2000;
    pub const HOST: u32 = 0x4000;
}
/// VkAccessFlagBits.
pub(super) mod access {
    pub const INDEX_READ: u32 = 0x2;
    pub const VERTEX_READ: u32 = 0x4;
    pub const UNIFORM_READ: u32 = 0x8;
    pub const SHADER_READ: u32 = 0x20;
    pub const SHADER_WRITE: u32 = 0x40;
    pub const COLOR_READ: u32 = 0x80;
    pub const COLOR_WRITE: u32 = 0x100;
    pub const DEPTH_READ: u32 = 0x200;
    pub const DEPTH_WRITE: u32 = 0x400;
    pub const TRANSFER_READ: u32 = 0x800;
    pub const TRANSFER_WRITE: u32 = 0x1000;
    pub const HOST_READ: u32 = 0x2000;
    pub const HOST_WRITE: u32 = 0x4000;
    pub const MEMORY_WRITE: u32 = 0x10000;
    pub const WRITES: u32 =
        SHADER_WRITE | COLOR_WRITE | DEPTH_WRITE | TRANSFER_WRITE | HOST_WRITE | MEMORY_WRITE;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct State {
    /// Current image layout (always `UNDEFINED` for buffers).
    pub layout: i32,
    /// Stages of the last write or layout transition.
    pub write_stages: u32,
    /// Accesses of the last write that may still need an availability operation.
    pub write_access: u32,
    /// Stages and accesses the last write has been made visible to.
    pub visible_stages: u32,
    pub visible_access: u32,
    /// Stages that read since the last write.
    pub read_stages: u32,
}

/// One way a scope uses a subresource: the layout it needs and the stages/accesses it performs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Use {
    pub layout: i32,
    pub stages: u32,
    pub access: u32,
    /// The whole subresource is overwritten without being read; old contents may be dropped.
    pub discard: bool,
}
impl Use {
    pub const fn new(layout: i32, stages: u32, access: u32) -> Self {
        Self {
            layout,
            stages,
            access,
            discard: false,
        }
    }
    pub const fn buffer(stages: u32, access: u32) -> Self {
        Self::new(UNDEFINED, stages, access)
    }
    pub const fn discarding(self) -> Self {
        Self {
            discard: true,
            ..self
        }
    }
    pub const fn writes(&self) -> bool {
        self.access & access::WRITES != 0
    }
    /// Combines two uses within one scope; they must agree on the layout.
    pub fn merge(&mut self, other: Use) -> bool {
        if self.layout != other.layout {
            return false;
        }
        self.stages |= other.stages;
        self.access |= other.access;
        self.discard &= other.discard;
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Barrier {
    pub src_stages: u32,
    pub src_access: u32,
    pub dst_stages: u32,
    pub dst_access: u32,
    pub old_layout: i32,
    pub new_layout: i32,
}

impl State {
    /// The barrier needed before `u` (if any); afterwards `self` describes the state after `u`.
    pub fn apply(&mut self, u: Use) -> Option<Barrier> {
        let write = u.writes();
        let transition = u.layout != self.layout;
        let prior = self.write_stages | self.read_stages;
        let barrier = if transition || write {
            // Layout transitions and writes must follow every earlier access.
            (transition || prior != 0).then_some(Barrier {
                src_stages: if prior == 0 { stage::TOP } else { prior },
                src_access: self.write_access,
                dst_stages: u.stages,
                dst_access: u.access,
                old_layout: if u.discard { UNDEFINED } else { self.layout },
                new_layout: u.layout,
            })
        } else if self.write_stages != 0
            && (u.stages & !self.visible_stages != 0 || u.access & !self.visible_access != 0)
        {
            // Read after a write that this reader has not been made aware of yet.
            Some(Barrier {
                src_stages: self.write_stages,
                src_access: self.write_access,
                dst_stages: u.stages,
                dst_access: u.access,
                old_layout: self.layout,
                new_layout: self.layout,
            })
        } else {
            None
        };
        if write {
            *self = State {
                layout: u.layout,
                write_stages: u.stages,
                write_access: u.access & access::WRITES,
                ..State::default()
            };
        } else if transition {
            // The transition is a write ordered before (and visible to) the requesting stages;
            // its own writes are made available automatically.
            *self = State {
                layout: u.layout,
                write_stages: u.stages,
                write_access: 0,
                visible_stages: u.stages,
                visible_access: u.access,
                read_stages: u.stages,
            };
        } else {
            if barrier.is_some() {
                self.visible_stages |= u.stages;
                self.visible_access |= u.access;
            }
            self.read_stages |= u.stages;
        }
        barrier
    }
    /// Makes the last GPU write visible to host reads after the submission's fence.
    pub fn host_read(&mut self) -> Option<Barrier> {
        if self.write_stages == 0 || self.visible_stages & stage::HOST != 0 {
            return None;
        }
        self.visible_stages |= stage::HOST;
        self.visible_access |= access::HOST_READ;
        Some(Barrier {
            src_stages: self.write_stages,
            src_access: self.write_access,
            dst_stages: stage::HOST,
            dst_access: access::HOST_READ,
            old_layout: self.layout,
            new_layout: self.layout,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLOR: Use = Use::new(
        COLOR_ATTACHMENT,
        stage::COLOR_OUTPUT,
        access::COLOR_READ | access::COLOR_WRITE,
    );
    const SAMPLED: Use = Use::new(
        SHADER_READ_ONLY,
        stage::FRAGMENT_SHADER,
        access::SHADER_READ,
    );

    #[test]
    fn render_then_sample_then_render_again() {
        let mut s = State::default();
        let first = s.apply(COLOR.discarding()).unwrap();
        assert_eq!(
            (first.old_layout, first.new_layout),
            (UNDEFINED, COLOR_ATTACHMENT)
        );
        assert_eq!(first.src_stages, stage::TOP);
        // Read after write: transition, wait for colour output, make the writes visible.
        let read = s.apply(SAMPLED).unwrap();
        assert_eq!(read.src_stages, stage::COLOR_OUTPUT);
        assert_eq!(read.src_access, access::COLOR_WRITE);
        assert_eq!(read.new_layout, SHADER_READ_ONLY);
        // Read after read in the same layout and stage: nothing to do.
        assert_eq!(s.apply(SAMPLED), None);
        // A new reader stage still needs visibility, chained after the transition.
        let vertex = Use::new(SHADER_READ_ONLY, stage::VERTEX_SHADER, access::SHADER_READ);
        let b = s.apply(vertex).unwrap();
        assert_eq!(b.src_stages, stage::FRAGMENT_SHADER);
        assert_eq!(b.old_layout, b.new_layout);
        assert_eq!(s.apply(vertex), None);
        // Write after read: wait for both reader stages; contents may be discarded.
        let again = s.apply(COLOR.discarding()).unwrap();
        assert_eq!(
            again.src_stages,
            stage::FRAGMENT_SHADER | stage::VERTEX_SHADER
        );
        assert_eq!(again.old_layout, UNDEFINED);
    }

    #[test]
    fn buffers_track_writes_reads_and_host_visibility() {
        let mut s = State::default();
        // Host-written data needs no GPU barrier before the first read.
        let vertex = Use::buffer(stage::VERTEX_INPUT, access::VERTEX_READ);
        assert_eq!(s.apply(vertex), None);
        // Write after read: execution dependency only.
        let copy = Use::buffer(stage::TRANSFER, access::TRANSFER_WRITE);
        let war = s.apply(copy).unwrap();
        assert_eq!((war.src_stages, war.src_access), (stage::VERTEX_INPUT, 0));
        let host = s.host_read().unwrap();
        assert_eq!(host.dst_access, access::HOST_READ);
        assert_eq!(s.host_read(), None);
        // The GPU still needs its own visibility operation.
        let raw = s.apply(vertex).unwrap();
        assert_eq!(raw.src_access, access::TRANSFER_WRITE);
        // Write after write.
        let storage = Use::buffer(stage::COMPUTE, access::SHADER_READ | access::SHADER_WRITE);
        let waw = s.apply(storage).unwrap();
        assert_eq!(waw.src_stages, stage::TRANSFER | stage::VERTEX_INPUT);
        assert_eq!(s.apply(storage).unwrap().src_access, access::SHADER_WRITE);
    }

    #[test]
    fn scope_uses_merge_only_with_the_same_layout() {
        let mut u = SAMPLED;
        assert!(u.merge(Use::new(
            SHADER_READ_ONLY,
            stage::VERTEX_SHADER,
            access::SHADER_READ
        )));
        assert_eq!(u.stages, stage::FRAGMENT_SHADER | stage::VERTEX_SHADER);
        assert!(!u.merge(COLOR));
        let mut d = COLOR.discarding();
        assert!(d.merge(COLOR));
        assert!(!d.discard);
    }
}
