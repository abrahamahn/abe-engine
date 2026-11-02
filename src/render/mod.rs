//! ABE's stylized renderer: soft cel shading, a time-of-day sky and weather, water reflections
//! and painterly post-processing, built on the backend-neutral [`crate::gpu`] layer and shaders
//! compiled by ABE's own compiler. The look is data ([`Look`], usually from a game's Look
//! file); nothing game-specific lives here.
pub mod look;
pub mod math;
mod renderer;
mod scene;
pub mod sky;

pub use look::{DetailPreset, DetailRule, Grass, Look, LookFile, Terrain, View};
pub use renderer::{Output, Quality, Renderer};
pub use sky::{Environment, Palette, SkyKey, TimeOfDay, Weather};
