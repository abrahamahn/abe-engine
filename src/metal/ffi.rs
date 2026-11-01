//! Metal and QuartzCore values used by the backend, from the public Metal framework headers
//! (`MTLPixelFormat.h`, `MTLTexture.h`, `MTLRenderPipeline.h`, ...). Plain data only; the
//! messages that use them are sent through [`crate::apple::objc`].
#![allow(dead_code)]

use crate::apple::objc::{Id, NSUInteger};

#[link(name = "Metal", kind = "framework")]
unsafe extern "C" {
    /// The system's default GPU (+1), or nil when Metal is unavailable.
    pub fn MTLCreateSystemDefaultDevice() -> Id;
    /// Every GPU of a Mac as an `NSArray` (+1). macOS only.
    #[cfg(target_os = "macos")]
    pub fn MTLCopyAllDevices() -> Id;
}

#[link(name = "QuartzCore", kind = "framework")]
unsafe extern "C" {}

pub mod pixel {
    pub const R8_UNORM: u64 = 10;
    pub const R16_FLOAT: u64 = 25;
    pub const R32_FLOAT: u64 = 55;
    pub const RG16_FLOAT: u64 = 65;
    pub const RGBA8_UNORM: u64 = 70;
    pub const RGBA8_UNORM_SRGB: u64 = 71;
    pub const BGRA8_UNORM: u64 = 80;
    pub const BGRA8_UNORM_SRGB: u64 = 81;
    pub const RGBA16_FLOAT: u64 = 115;
    pub const RGBA32_FLOAT: u64 = 125;
    pub const DEPTH32_FLOAT: u64 = 252;
}

pub mod texture_type {
    pub const D2: u64 = 2;
    pub const D2_ARRAY: u64 = 3;
    pub const CUBE: u64 = 5;
}

pub mod usage {
    pub const SHADER_READ: u64 = 1;
    pub const SHADER_WRITE: u64 = 2;
    pub const RENDER_TARGET: u64 = 4;
}

/// `MTLResourceOptions`: storage mode in bits 4..8, CPU cache mode in bits 0..4.
pub mod resource {
    pub const STORAGE_SHARED: u64 = 0;
    pub const STORAGE_PRIVATE: u64 = 2 << 4;
    pub const CPU_WRITE_COMBINED: u64 = 1;
}
pub const STORAGE_MODE_PRIVATE: u64 = 2;

pub mod load {
    pub const DONT_CARE: u64 = 0;
    pub const LOAD: u64 = 1;
    pub const CLEAR: u64 = 2;
}
pub mod store {
    pub const DONT_CARE: u64 = 0;
    pub const STORE: u64 = 1;
}

pub mod primitive {
    pub const LINE: u64 = 1;
    pub const TRIANGLE: u64 = 3;
    pub const TRIANGLE_STRIP: u64 = 4;
}
pub const INDEX_UINT32: u64 = 1;

pub mod cull {
    pub const NONE: u64 = 0;
    pub const FRONT: u64 = 1;
    pub const BACK: u64 = 2;
}
pub mod winding {
    pub const CLOCKWISE: u64 = 0;
    pub const COUNTER_CLOCKWISE: u64 = 1;
}
pub mod depth_clip {
    pub const CLIP: u64 = 0;
    pub const CLAMP: u64 = 1;
}

/// `MTLCompareFunction` (the same order as `VkCompareOp`).
pub mod compare {
    pub const NEVER: u64 = 0;
    pub const LESS: u64 = 1;
    pub const EQUAL: u64 = 2;
    pub const LESS_EQUAL: u64 = 3;
    pub const GREATER: u64 = 4;
    pub const NOT_EQUAL: u64 = 5;
    pub const GREATER_EQUAL: u64 = 6;
    pub const ALWAYS: u64 = 7;
}

pub mod blend {
    pub const ZERO: u64 = 0;
    pub const ONE: u64 = 1;
    pub const SOURCE_ALPHA: u64 = 4;
    pub const ONE_MINUS_SOURCE_ALPHA: u64 = 5;
    pub const ADD: u64 = 0;
}
/// `MTLColorWriteMask`: red is the highest bit.
pub mod write_mask {
    pub const RED: u64 = 8;
    pub const GREEN: u64 = 4;
    pub const BLUE: u64 = 2;
    pub const ALPHA: u64 = 1;
}

pub mod vertex_format {
    pub const UCHAR4_NORMALIZED: u64 = 9;
    pub const FLOAT: u64 = 28;
    pub const FLOAT2: u64 = 29;
    pub const FLOAT3: u64 = 30;
    pub const FLOAT4: u64 = 31;
    pub const UINT: u64 = 36;
}
pub const STEP_PER_VERTEX: u64 = 1;

pub mod sampler {
    pub const NEAREST: u64 = 0;
    pub const LINEAR: u64 = 1;
    pub const MIP_NEAREST: u64 = 1;
    pub const MIP_LINEAR: u64 = 2;
    pub const CLAMP_TO_EDGE: u64 = 0;
    pub const REPEAT: u64 = 2;
    pub const MIRROR_REPEAT: u64 = 3;
}

pub mod status {
    pub const COMPLETED: u64 = 4;
    pub const ERROR: u64 = 5;
}

/// `MTLGPUFamily` values for `supportsFamily:`.
pub mod family {
    pub const APPLE4: i64 = 1004;
    pub const APPLE7: i64 = 1007;
    pub const MAC2: i64 = 2002;
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MTLClearColor {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub alpha: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MTLViewport {
    pub origin_x: f64,
    pub origin_y: f64,
    pub width: f64,
    pub height: f64,
    pub znear: f64,
    pub zfar: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MTLScissorRect {
    pub x: NSUInteger,
    pub y: NSUInteger,
    pub width: NSUInteger,
    pub height: NSUInteger,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MTLOrigin {
    pub x: NSUInteger,
    pub y: NSUInteger,
    pub z: NSUInteger,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MTLSize {
    pub width: NSUInteger,
    pub height: NSUInteger,
    pub depth: NSUInteger,
}
