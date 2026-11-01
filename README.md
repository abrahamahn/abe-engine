# ABE Engine

ABE is a native Rust game engine in development. Its player, editor and build tools are intended for multiple games. Game assets, domain rules, network protocols and title-specific adapters belong in each game's repository.

The core has zero third-party Cargo dependencies. The Vulkan backend calls the system loader directly and builds its shader programs as SPIR-V from Rust source.

## Run the native player

```sh
cargo run --locked --offline --release --bin abe-player
```

On Linux the player uses X11, including XWayland, and on Windows it uses Win32. Both call Vulkan directly. Click or press Space to pause, use the arrow keys to orbit, and press Escape to exit. Pass `--frames 120` after Cargo's `--` separator for a finite smoke run. The presentation path uses two reusable frame slots, per-image depth and synchronization, and a swapchain. The normal frame loop does not read pixels back to the CPU.

![The native player on a Linux virtual display](docs/player.png)

## Render an offscreen reference

On Linux or Windows with a Vulkan loader and driver:

```sh
cargo run --locked --offline --release --example observatory -- scene.ppm
```

The example renders a generic 3D sculpture to a Netpbm image. It uses the same camera and renderer interfaces available to game clients.

## Render with the stylized renderer

```sh
cargo run --locked --offline --release --bin abe-render -- --scene world.abew --look look.json --all-views --time 20.5 --weather clear --width 1920 --height 1080 --samples 16 --out-dir renders
cargo run --locked --offline --release --bin abe-player -- --scene world.abew --look look.json
```

`abe_engine::render` draws cooked worlds with soft cel lighting, two shadow cascades, tiled point lights, planar water reflections, a time-of-day sky (sun and moon from the clock, latitude and date) and weather (`clear`, `cloudy`, `overcast`, `mist`, `rain`, `storm`, `petals`; drops stop at roofs), then ambient occlusion, outlines, aerial perspective, bloom, grading and a painterly filter. It runs on the backend-neutral `abe_engine::gpu` layer. The style is data: a game's Look file (JSON) sets shading, post-processing, sky palette, place and time, weather and named camera views, so different games share one renderer. With a scene that has materials or a `--look` file, the player flies: W/A/S/D move, Q/E down/up, arrows or left-drag look, Shift faster, 1–9 views, T/G time, C day cycle, Y weather, P pause, Esc quit. `--high` uses still-image quality. `abe-render --sequence START END FRAMES` renders a time-lapse of one view between two clock times as numbered PNGs in `--out-dir`, with water, clouds and weather moving at 24 frames a second.

Shaders are WGSL (a documented subset, plus `var<push_constant>`) compiled at startup by the engine's own compiler, `abe-shader`, to SPIR-V, and for WebGPU also back to standard WGSL. `cargo run -p abe-shader --example abe-shaderc -- shader.wgsl fragment fs_main out.spv` compiles one entry point and prints its reflection; an `out.wgsl` path writes the WebGPU WGSL instead.

## Run in a browser (WebGPU)

The web player draws a cooked world with the same stylized renderer in a canvas through WebGPU, for Chrome and Edge 113+ and Safari 26 (macOS, iPhone, iPad). It is `crates/abe-web`, a wasm32 module without third-party crates, plus `web/abe.js` and `web/index.html`, which own the WebGPU device, the canvas, input and the frame loop. Build it with rustup:

```sh
rustup target add wasm32-unknown-unknown
scripts/build-web.sh                  # web/dist: index.html, abe.js, abe_web.wasm
```

Without bash, `cargo build --release -p abe-web --target wasm32-unknown-unknown` builds the same module (`target/wasm32-unknown-unknown/release/abe_web.wasm`); put it next to `web/index.html` and `web/abe.js`. A rustc without a wasm32 standard library (such as a distribution's `rustc-1.91`) rebuilds std from source and links with LLVM's `wasm-ld`; std's own dependencies (dlmalloc) come from crates.io on the first build:

```sh
ABE_WEB_TOOLCHAIN=build-std ABE_WEB_RUSTC=rustc-1.91 ABE_WEB_CARGO=cargo-1.91 \
  ABE_WEB_RUST_SRC=/path/to/rust-src-1.91/library scripts/build-web.sh
```

WebGPU needs a secure context: serve the folder from localhost or over https, with the world and Look file at URLs the page can fetch (`web/dist` is not tracked):

```sh
cp world.abew look.json web/dist/
python3 -m http.server -d web/dist 8000
# http://localhost:8000/?scene=world.abew&look=look.json
```

`scene` and `look` name the files; `view=NAME`, `time=HOURS`, `weather=PRESET`, `seconds=S` and `paused=1` set the starting state; `quality=high` uses still-image quality; `dpr=N` caps the device pixel ratio (default 2, 1.5 on touch screens), `scale=F` scales the resolution and `size=WxH` fixes it. The controls are abe-player's (W/A/S/D, Q/E, arrows or drag, Shift, 1–9, T/G, C, Y, P) plus H for help. On a touch screen, drag with one finger to look and two to move; buttons change the time, weather and view. The page shows loading progress, a status line (clock, weather, frame rate, view, resolution) and every error, including WebGPU validation errors.

On WebGPU, `abe_shader::compile_wgsl` (through `gpu::compile_shader`) turns each shader into standard WGSL: `var<push_constant>` becomes a uniform at `@group(3)` that the backend fills from a per-submission ring with dynamic offsets, and a generated entry point negates clip-space y because ABE uses Vulkan's convention. The browser derives barriers, `submit` and `present` do not wait, the frame is presented when the `requestAnimationFrame` callback returns, and `read_buffer` is unavailable because browsers map buffers asynchronously.

The browser test serves the built page and the files from where they are, renders each case in headless Chromium, reads back the canvas, and with `--abe-render` compares it with a native render of the same view, time and weather (`NAME-web.png`, `NAME-native.png`, `NAME-diff.png`, `results.json`). Any WebGPU error fails a case:

```sh
cargo run --release --example synthetic_world -- scenes
cargo build --release --bin abe-render
node web/test/run.cjs --dist web/dist --out web-test --abe-render target/release/abe-render \
  --case name=synthetic,scene=scenes/synthetic.abew,look=scenes/synthetic-look.json,view=test,time=17.5,weather=clear,size=640x360
ABE_WGSL_DUMP=wgsl cargo test -p abe-shader --test wgsl && node web/test/validate-wgsl.cjs wgsl
```

The last line compiles every corpus entry point with the browser's WGSL compiler. One is expected to fail: `compute.wgsl`'s `splat` calls `workgroupBarrier` after a non-uniform `return`, which WebGPU's uniformity analysis rejects and abe-shader does not check.

Without a GPU, Chromium runs WebGPU on SwiftShader; the scripts pass `--enable-unsafe-webgpu --enable-features=Vulkan --use-angle=vulkan --use-vulkan=swiftshader`, because with software compositing headless Chromium cannot allocate canvas images and drops the WebGPU device after a few frames. `--selftest` also runs the GPU-layer paths the renderer does not use (compute with push constants into storage textures and buffers, copies, mip generation, a blit into the canvas), and `interact=1` drives the keyboard, mouse, a two-finger touch drag, an on-screen button and a window resize. In Chromium 153 on SwiftShader, the synthetic scene matches the native Vulkan render on llvmpipe at 47–50 dB PSNR (mean difference under 0.5 of 255) across noon, dusk, night and storm, and a 694,000-triangle game world (kept outside this repository) at 34–40 dB across five views, times and weathers (night rain is the lowest), with differences only at sub-pixel edges, thin rain streaks, foliage and texture filtering. Chrome and Edge on Windows GPUs and Safari 26 still need a device run.

## Cook a static scene

```sh
cargo run --locked --offline --release --bin abe-build -- cook source.glb scene.abew
cargo run --locked --offline --release --bin abe-build -- inspect scene.abew
```

The cooker accepts embedded glTF 2.0 triangle meshes (float positions, normals and UVs, indices), PBR material factors, PNG base-colour, normal and emissive textures, `KHR_materials_emissive_strength`, `KHR_lights_punctual` point lights, alpha modes, double-sidedness, `extras.abe.shading` (`standard`, `foliage`, `water`, `unlit`) and `extras.abe.emission` (`lamp`: scaled by the time of day), and scene-node transforms. ABEW v2 stores meshes once, references them from affine instances, and adds materials, mip-chained RGBA8 textures and lights; v1 files still load. Unsupported codecs, skins, animation and other image types are reported explicitly. Games convert authored content (Draco, JPEG, WebP) before cooking. The portable `abe-core` and `abe-assets` crates have no external dependencies.

[Explore the six architecture layers](docs/architecture.html) (download and open the self-contained HTML to use the selectors). [Engine milestones](docs/roadmap.md).

## Verify

```sh
cargo test --workspace --locked --offline
cargo test --locked --offline --test vulkan -- --ignored
cargo test --locked --offline --test present -- --ignored
cargo test --locked --offline --test gpu --test render -- --ignored
```

Use `Presenter::set_static_scene` once for the environment and `draw_with_static` each frame for changing objects. The two draws share a depth buffer.

The offscreen tests check depth, pixel readback, GPU camera projection and mixed static/changing geometry. The desktop test needs a display and exercises resize, hide/restore, and presenter recreation. Linux presentation and keyboard input have been exercised on Mesa llvmpipe under Xvfb with synchronization validation. Windows is cross-built and still needs a native device run. Android has the offscreen backend only. Apple Metal is planned. Browsers run the WebGPU backend (see above). The Vulkan ABI declarations are adapted from [Khronos Vulkan-Headers v1.3.275](https://github.com/KhronosGroup/Vulkan-Headers/tree/v1.3.275) under the license in [docs/Vulkan-Registry-LICENSE.txt](docs/Vulkan-Registry-LICENSE.txt).

## Measure frame time

```sh
cargo run --release --locked --offline --bin abe-player -- --scene scene.abew
cargo run --release --locked --offline --bin abe-bench -- --scene scene.abew --width 1920 --height 1080 --warmup 40 --frames 240
```

Omit `--scene` to run the 38-triangle observatory. The benchmark uploads static geometry once, moves the camera each frame and renders into two reusable offscreen targets. It reports completed frames per second, CPU command preparation and submission, fence waits, and GPU timestamp percentiles when the driver exposes them. Set `ABE_VULKAN_DEVICE` to a device-name substring to select an adapter. Report its name, resolution, workload, sample count and validation settings alongside results.

The uncapped engine ceiling under investigation is 500–1,000 FPS (2–1 ms). Applications set their own presentation budgets. A 120 Hz mode has 8.33 ms per complete frame and a 240 Hz mode has 4.17 ms. Measure the application's input, simulation, UI, composition and presentation separately from this render-only benchmark.

`abe-framecheck` evaluates recorded presentation times from a real device. Export one timestamp in milliseconds per distinct displayed frame from a single surface. Feed it an uninterrupted active session, including hitches. Submitting frames to a driver or receiving display callbacks does not prove they became visible.

```sh
cargo run --release --locked --offline --bin abe-framecheck -- 120 1200 presented-ms.txt
cargo run --release --locked --offline --bin abe-framecheck -- 240 600 presented-ms.txt
```

The example mobile test lasts 20 minutes after warm-up on a 120 Hz handset. The desktop example lasts 10 minutes on a 240 Hz display. The tool requires at least 98% of target cadence overall and in the worst sliding minute, with the 99th-percentile interval within 10% of a frame period. It reports intervals over 1.5 periods separately. Record resolution, device, requested and active refresh rate, power mode, and thermal state with each trace.
