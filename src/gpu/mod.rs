//! ABE's GPU layer ("RHI v0"): a backend-neutral interface shaped like WebGPU and Metal —
//! pipelines, bind groups, passes and automatic barriers — implemented today on the engine's
//! direct Vulkan 1.0 backend. Metal, WebGPU and D3D12 backends can implement the same API.
//!
//! # Objects
//! A [`Device`] owns every object and hands out small `Copy` handles ([`BufferId`],
//! [`TextureId`], [`SamplerId`], [`PipelineId`], [`BindGroupId`]). Handles are generational:
//! using a destroyed object is reported as an error. Everything is freed when the device drops
//! (after the GPU is idle). Pipelines are built from [`abe_shader::Compiled`] modules and their
//! reflection: bind-group layouts, push-constant sizes and vertex inputs come from the shaders.
//! A bind group is created for one `@group` of one pipeline and can be used with any pipeline
//! whose layout for that group is identical (same bindings, kinds and shader stages). Because
//! reflection lists only statically used resources, entries for unused bindings are ignored.
//!
//! # Command recording
//! An [`Encoder`] only records. [`Device::submit`] replays it into a command buffer, submits
//! it and waits for it (offscreen work, uploads, readback). [`Device::present`] replays it for
//! the swapchain texture returned by [`Device::acquire`] and presents without waiting.
//!
//! # Synchronization model
//! Applications never write barriers. During replay the backend tracks, per texture
//! subresource (mip, layer) and per buffer, the current layout and the pipeline stages and
//! accesses of the last write and of the reads since then. This state persists across
//! submissions in queue submission order, so work recorded by `present` for frame N+1 is
//! correctly ordered after frame N even while N is still executing.
//!
//! The commands of an encoder are grouped into *scopes*: each render pass (from `begin_pass`
//! to `end_pass`), each `dispatch`, each copy and each blit. Before a scope begins, the backend
//! collects every resource the scope uses —
//! * colour attachments (colour-attachment layout, read+write),
//! * the depth attachment (depth-attachment layout, read+write),
//! * every texture referenced by a bind group set inside the pass or passed to the dispatch
//!   (shader-read-only layout for sampled textures, *including depth textures*; general layout
//!   for storage textures), in the shader stages declared by the binding,
//! * uniform, storage, vertex and index buffers,
//! * copy/blit sources and destinations (transfer layouts),
//!
//! and emits one `vkCmdPipelineBarrier` that transitions layouts and orders the scope after
//! earlier writes (read-after-write, with memory visibility), earlier reads (write-after-read)
//! and earlier writes (write-after-write). Barriers are only ever placed between scopes, never
//! inside a pass. Read-after-read in the same layout needs no barrier. Contents are discarded
//! (old layout undefined) when a pass clears or ignores an attachment and when a blit or upload
//! overwrites a whole subresource. A subresource used in two different ways in one pass (for
//! example as an attachment and as a sampled texture) is an error. Within one pass, storage
//! writes are not ordered between draws (as in WebGPU); split the work into passes instead.
//!
//! Host interaction: `write_buffer` on `Upload` memory and `read_buffer` first wait for any
//! in-flight submission that uses the buffer, so they are always safe but can stall; index
//! per-frame buffers by [`Device::frame_slot`] to avoid stalls. `write_buffer` on `Device`
//! memory, `write_texture` and `generate_mips` record their own small submission, which is
//! ordered after earlier GPU work on the resource, and wait for it. Readback buffers written
//! by a submission are made visible to the host at its end.
//!
//! Presentation: `acquire` waits on the acquire semaphore in the colour-output and transfer
//! stages, so a frame can render or blit into the swapchain texture (whose previous contents
//! are undefined: clear it, ignore it or blit over it first); `present` transitions it for
//! presentation. At most two frames are in flight: when `present` returns, the GPU has
//! finished the frame that last used the new [`Device::frame_slot`].
//!
//! # Conventions
//! Clip space is Vulkan's: y points down and depth runs 0..1 (the engine camera already
//! produces this), and the viewport has a positive height. Uploads and readbacks use tightly
//! packed rows of `Format::texel_size` bytes.
// Without a backend (no Vulkan on this platform yet) only the descriptions and the encoder exist.
#![cfg_attr(
    not(any(
        target_os = "linux",
        target_os = "windows",
        target_os = "android",
        target_vendor = "apple"
    )),
    allow(dead_code, unused_imports)
)]
mod encoder;
mod types;
pub use abe_shader as shader;
pub(crate) use encoder::Command;
pub use encoder::Encoder;
pub use types::*;

#[cfg(target_vendor = "apple")]
use crate::metal::Gpu;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "android"))]
use crate::vulkan::rhi::Gpu;
#[cfg(target_arch = "wasm32")]
mod webgpu;

/// Compiles `entry` of a shader `source` for this platform's backend: SPIR-V and reflection,
/// plus WGSL (with push constants lowered to a dynamically offset uniform) when the backend is
/// WebGPU. Pipelines on WebGPU need shaders compiled this way.
pub fn compile_shader(source: &str, stage: shader::Stage, entry: &str) -> Result<shader::Compiled> {
    #[cfg(target_arch = "wasm32")]
    let compiled = abe_shader::compile_wgsl(source, stage, entry);
    #[cfg(not(target_arch = "wasm32"))]
    let compiled = abe_shader::compile(source, stage, entry);
    compiled.map_err(|e| format!("shader {entry}: {e}"))
}

/// Validation messages collected while `ABE_VULKAN_VALIDATION=1`. The log outlives the device,
/// so it can also report errors raised while the device is torn down.
#[derive(Clone, Debug, Default)]
pub struct ValidationLog(pub(crate) std::sync::Arc<std::sync::Mutex<Vec<String>>>);
impl ValidationLog {
    /// Error messages reported so far.
    pub fn errors(&self) -> Vec<String> {
        self.0.lock().map(|l| l.clone()).unwrap_or_default()
    }
}

/// A GPU device with its queue, object pools and, for [`Device::for_window`], a surface.
///
/// The lifetime is that of the window a presentation device draws into; headless devices are
/// `Device<'static>`. Set `ABE_VULKAN_DEVICE` to a name substring to pick an adapter and
/// `ABE_VULKAN_VALIDATION=1` to enable the Khronos validation layer with synchronization
/// validation (creation then fails if the layer is missing).
///
/// On macOS and iOS the device is Metal (see the `metal` module): on a Mac,
/// `ABE_METAL_DEVICE` picks a GPU by name substring, and Metal's own validation is enabled
/// with `MTL_DEBUG_LAYER=1` (reported on the console, not through [`ValidationLog`]).
#[cfg(any(
    target_os = "linux",
    target_os = "windows",
    target_os = "android",
    target_vendor = "apple"
))]
pub struct Device<'w> {
    gpu: Gpu<'w>,
}

#[cfg(any(
    target_os = "linux",
    target_os = "windows",
    target_os = "android",
    target_vendor = "apple"
))]
impl Device<'static> {
    /// An offscreen device.
    pub fn headless() -> Result<Self> {
        Ok(Self {
            gpu: Gpu::headless()?,
        })
    }
}

#[cfg(any(
    target_os = "linux",
    target_os = "windows",
    target_os = "android",
    target_vendor = "apple"
))]
impl<'w> Device<'w> {
    /// A device that can present to `window` (X11 on Linux, Win32 on Windows, the window's
    /// `CAMetalLayer` on macOS and iOS). The window must outlive the device.
    #[cfg(any(
        target_os = "linux",
        target_os = "windows",
        target_os = "macos",
        target_os = "ios"
    ))]
    pub fn for_window(window: &'w crate::window::Window) -> Result<Self> {
        Ok(Self {
            gpu: Gpu::for_window(window)?,
        })
    }
    pub fn name(&self) -> &str {
        self.gpu.name()
    }
    pub fn limits(&self) -> Limits {
        self.gpu.limits()
    }
    /// True when `ABE_VULKAN_VALIDATION=1` enabled the validation layer.
    pub fn validation_enabled(&self) -> bool {
        self.gpu.validation_log().is_some()
    }
    /// Validation errors reported so far (always empty without validation).
    pub fn validation_errors(&self) -> Vec<String> {
        self.gpu
            .validation_log()
            .map(|l| l.errors())
            .unwrap_or_default()
    }
    /// How long a submission may run before it is reported as lost (default 20 s). Applies to
    /// every device in the process; raise it for long offline renders on slow devices.
    pub fn set_submit_timeout(&mut self, timeout: std::time::Duration) {
        // Metal waits with waitUntilCompleted and relies on the system's GPU watchdog.
        #[cfg(not(target_vendor = "apple"))]
        crate::vulkan::rhi::set_fence_timeout(timeout);
        #[cfg(target_vendor = "apple")]
        let _ = timeout;
    }
    /// A handle to the validation log that stays readable after the device is dropped.
    pub fn validation_log(&self) -> ValidationLog {
        self.gpu.validation_log().unwrap_or_default()
    }

    pub fn create_buffer(&mut self, d: &BufferDesc) -> Result<BufferId> {
        self.gpu.create_buffer(d)
    }
    /// `Upload`/`Readback` memory: memcpy into the mapping (after waiting for in-flight GPU
    /// use of the buffer). `Device` memory: staged copy, submitted and waited for; requires
    /// `BufferUsage::COPY_DST`.
    pub fn write_buffer(&mut self, b: BufferId, offset: u64, data: &[u8]) -> Result<()> {
        self.gpu.write_buffer(b, offset, data)
    }
    /// The whole contents of a `Readback` buffer, after the submissions that wrote it.
    pub fn read_buffer(&self, b: BufferId) -> Result<Vec<u8>> {
        self.gpu.read_buffer(b)
    }
    pub fn create_texture(&mut self, d: &TextureDesc) -> Result<TextureId> {
        self.gpu.create_texture(d)
    }
    /// Replaces one whole subresource with tightly packed rows (requires `COPY_DST`) and leaves
    /// it ready to sample.
    pub fn write_texture(&mut self, t: TextureId, mip: u32, layer: u32, data: &[u8]) -> Result<()> {
        self.gpu.write_texture(t, mip, layer, data)
    }
    /// Fills mips 1.. of every layer by linear blits from mip 0 (requires `COPY_SRC | COPY_DST`
    /// and a filterable colour format) and leaves the texture ready to sample.
    pub fn generate_mips(&mut self, t: TextureId) -> Result<()> {
        self.gpu.generate_mips(t)
    }
    /// The description a texture was created with (swapchain textures included).
    pub fn texture_desc(&self, t: TextureId) -> Result<TextureDesc> {
        self.gpu.texture_desc(t)
    }
    /// Identical descriptions share one sampler.
    pub fn create_sampler(&mut self, d: &SamplerDesc) -> Result<SamplerId> {
        self.gpu.create_sampler(d)
    }
    pub fn create_render_pipeline(&mut self, d: &RenderPipelineDesc) -> Result<PipelineId> {
        self.gpu.create_render_pipeline(d)
    }
    pub fn create_compute_pipeline(&mut self, shader: &abe_shader::Compiled) -> Result<PipelineId> {
        self.gpu.create_compute_pipeline(shader)
    }
    /// Binds `entries` (`(binding, resource)`) to `@group(group)` of `pipeline`. Every binding
    /// the pipeline uses in that group must be given exactly once, with a matching kind.
    /// Reflection only lists what the entry points statically use, so entries for other
    /// bindings are ignored, and a group the pipeline does not use yields an empty bind group.
    pub fn create_bind_group(
        &mut self,
        pipeline: PipelineId,
        group: u32,
        entries: &[(u32, Resource)],
    ) -> Result<BindGroupId> {
        self.gpu.create_bind_group(pipeline, group, entries)
    }
    /// Safe while frames are in flight: the object is freed once the GPU is done with it.
    pub fn destroy_buffer(&mut self, b: BufferId) {
        self.gpu.destroy_buffer(b)
    }
    /// Safe while frames are in flight. Swapchain textures are owned by the device and ignored.
    pub fn destroy_texture(&mut self, t: TextureId) {
        self.gpu.destroy_texture(t)
    }
    /// Safe while frames are in flight.
    pub fn destroy_bind_group(&mut self, g: BindGroupId) {
        self.gpu.destroy_bind_group(g)
    }
    pub fn encoder(&self) -> Encoder {
        Encoder::new()
    }
    /// Executes `e` and waits for completion.
    pub fn submit(&mut self, e: Encoder) -> Result<()> {
        self.gpu.submit(e)
    }
    /// The next swapchain texture, or `None` while the window is hidden, minimized or the
    /// surface is temporarily unavailable. Recreates the swapchain after resizes. Calling it
    /// again before `present` returns the same texture.
    pub fn acquire(&mut self) -> Result<Option<TextureId>> {
        self.gpu.acquire()
    }
    /// Format of the swapchain textures: `Bgra8Srgb` when the surface offers it, otherwise
    /// `Bgra8Unorm`, `Rgba8Srgb` or `Rgba8Unorm`. Headless devices report `Bgra8Srgb`.
    pub fn surface_format(&self) -> Format {
        self.gpu.surface_format()
    }
    /// Size of the current swapchain (or of the window before the first `acquire`); `(0, 0)`
    /// for headless devices.
    pub fn surface_size(&self) -> (u32, u32) {
        self.gpu.surface_size()
    }
    /// Submits `e`, which renders into the texture returned by `acquire`, and presents it.
    pub fn present(&mut self, e: Encoder) -> Result<()> {
        self.gpu.present(e)
    }
    /// 0 or 1: which per-frame resources the next frame may overwrite. When `present` returns,
    /// the GPU has finished the earlier frame that used this slot.
    pub fn frame_slot(&self) -> usize {
        self.gpu.frame_slot()
    }
}

/// A GPU device in a browser, on WebGPU (`wasm32` builds hosted by `web/abe.js`).
///
/// The host creates the adapter, the device and (for [`Device::for_canvas`]) the canvas
/// context before the engine starts. The contract above holds with these differences:
/// `submit` and `present` never wait; queue writes and submissions run in call order, so
/// per-frame resources are not required for correctness; the frame is presented when the
/// `requestAnimationFrame` callback returns; `read_buffer` returns an error (browsers map
/// buffers asynchronously). WebGPU always validates: errors arrive asynchronously and
/// [`Device::validation_errors`] returns those reported so far. Shaders must come from
/// [`compile_shader`]. See the `webgpu` module for the remaining limits.
#[cfg(target_arch = "wasm32")]
pub struct Device<'w> {
    gpu: webgpu::Gpu<'w>,
}

#[cfg(target_arch = "wasm32")]
impl Device<'static> {
    /// The host's device, without presentation.
    pub fn headless() -> Result<Self> {
        Ok(Self {
            gpu: webgpu::Gpu::open(false)?,
        })
    }
    /// The host's device and its canvas: `acquire` returns the canvas texture.
    pub fn for_canvas() -> Result<Self> {
        Ok(Self {
            gpu: webgpu::Gpu::open(true)?,
        })
    }
}

#[cfg(target_arch = "wasm32")]
impl Device<'_> {
    pub fn name(&self) -> &str {
        self.gpu.name()
    }
    pub fn limits(&self) -> Limits {
        self.gpu.limits()
    }
    /// Always true: WebGPU validates every call.
    pub fn validation_enabled(&self) -> bool {
        true
    }
    /// WebGPU errors reported so far (validation, out of memory, lost device, shader
    /// compilation). They arrive asynchronously, after the call that caused them.
    pub fn validation_errors(&self) -> Vec<String> {
        self.gpu.validation_errors()
    }
    pub fn validation_log(&self) -> ValidationLog {
        ValidationLog(std::sync::Arc::new(std::sync::Mutex::new(
            self.validation_errors(),
        )))
    }
    /// No effect: browsers submit without waiting and report a lost device themselves.
    pub fn set_submit_timeout(&mut self, timeout: std::time::Duration) {
        let _ = timeout;
    }
    pub fn create_buffer(&mut self, d: &BufferDesc) -> Result<BufferId> {
        self.gpu.create_buffer(d)
    }
    /// `queue.writeBuffer`: ordered after earlier submissions; offset and length must be
    /// multiples of 4.
    pub fn write_buffer(&mut self, b: BufferId, offset: u64, data: &[u8]) -> Result<()> {
        self.gpu.write_buffer(b, offset, data)
    }
    /// Not available on WebGPU.
    pub fn read_buffer(&self, b: BufferId) -> Result<Vec<u8>> {
        self.gpu.read_buffer(b)
    }
    pub fn create_texture(&mut self, d: &TextureDesc) -> Result<TextureId> {
        self.gpu.create_texture(d)
    }
    pub fn write_texture(&mut self, t: TextureId, mip: u32, layer: u32, data: &[u8]) -> Result<()> {
        self.gpu.write_texture(t, mip, layer, data)
    }
    pub fn generate_mips(&mut self, t: TextureId) -> Result<()> {
        self.gpu.generate_mips(t)
    }
    pub fn texture_desc(&self, t: TextureId) -> Result<TextureDesc> {
        self.gpu.texture_desc(t)
    }
    pub fn create_sampler(&mut self, d: &SamplerDesc) -> Result<SamplerId> {
        self.gpu.create_sampler(d)
    }
    pub fn create_render_pipeline(&mut self, d: &RenderPipelineDesc) -> Result<PipelineId> {
        self.gpu.create_render_pipeline(d)
    }
    pub fn create_compute_pipeline(&mut self, shader: &abe_shader::Compiled) -> Result<PipelineId> {
        self.gpu.create_compute_pipeline(shader)
    }
    pub fn create_bind_group(
        &mut self,
        pipeline: PipelineId,
        group: u32,
        entries: &[(u32, Resource)],
    ) -> Result<BindGroupId> {
        self.gpu.create_bind_group(pipeline, group, entries)
    }
    pub fn destroy_buffer(&mut self, b: BufferId) {
        self.gpu.destroy_buffer(b)
    }
    pub fn destroy_texture(&mut self, t: TextureId) {
        self.gpu.destroy_texture(t)
    }
    pub fn destroy_bind_group(&mut self, g: BindGroupId) {
        self.gpu.destroy_bind_group(g)
    }
    pub fn encoder(&self) -> Encoder {
        Encoder::new()
    }
    /// Records `e` and submits it without waiting.
    pub fn submit(&mut self, e: Encoder) -> Result<()> {
        self.gpu.submit(e)
    }
    /// The canvas texture at the canvas's current size, or `None` while it has no area.
    pub fn acquire(&mut self) -> Result<Option<TextureId>> {
        self.gpu.acquire()
    }
    /// The canvas format (`Bgra8Unorm` or `Rgba8Unorm`; the renderer encodes sRGB itself).
    pub fn surface_format(&self) -> Format {
        self.gpu.surface_format()
    }
    /// The canvas size in device pixels.
    pub fn surface_size(&self) -> (u32, u32) {
        self.gpu.surface_size()
    }
    /// Submits `e`, which renders into the canvas texture; the browser presents it when the
    /// current animation frame callback returns.
    pub fn present(&mut self, e: Encoder) -> Result<()> {
        self.gpu.present(e)
    }
    /// 0 or 1, alternating with every `present`.
    pub fn frame_slot(&self) -> usize {
        self.gpu.frame_slot()
    }
}
