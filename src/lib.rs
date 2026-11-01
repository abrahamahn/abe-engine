//! ABE's portable scene data and camera. Game-specific content belongs to clients.

#[cfg(target_vendor = "apple")]
mod apple;
pub mod gpu;
#[cfg(target_vendor = "apple")]
mod metal;
pub mod render;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "android"))]
pub mod vulkan;
#[cfg(any(
    target_os = "linux",
    target_os = "windows",
    target_os = "macos",
    target_os = "ios"
))]
pub mod window;

pub use abe_assets as assets;
pub use abe_core::*;
