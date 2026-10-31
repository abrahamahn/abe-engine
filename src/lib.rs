//! ABE's portable scene data and camera. Game-specific content belongs to clients.

pub mod gpu;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "android"))]
pub mod vulkan;
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub mod window;

pub use abe_assets as assets;
pub use abe_core::*;
