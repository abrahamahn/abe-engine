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

Shaders are WGSL (a documented subset, plus `var<push_constant>`) compiled at startup by the engine's own compiler, `abe-shader`, to SPIR-V. `cargo run -p abe-shader --example abe-shaderc -- shader.wgsl fragment fs_main out.spv` compiles one entry point and prints its reflection.

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

The offscreen tests check depth, pixel readback, GPU camera projection and mixed static/changing geometry. The desktop test needs a display and exercises resize, hide/restore, and presenter recreation. Linux presentation and keyboard input have been exercised on Mesa llvmpipe under Xvfb with synchronization validation. Windows is cross-built and still needs a native device run. Android has the offscreen backend only. Apple Metal and browser presentation are planned. The Vulkan ABI declarations are adapted from [Khronos Vulkan-Headers v1.3.275](https://github.com/KhronosGroup/Vulkan-Headers/tree/v1.3.275) under the license in [docs/Vulkan-Registry-LICENSE.txt](docs/Vulkan-Registry-LICENSE.txt).

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
