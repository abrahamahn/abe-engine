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

`abe_engine::render` draws cooked worlds with soft cel lighting and painted brushwork, two shadow cascades, tiled point lights, planar water reflections with shallows, foam and glitter, a time-of-day sky (sun and moon from the clock, latitude and date) and weather (`clear`, `cloudy`, `overcast`, `mist`, `rain`, `storm`, `petals`; drops stop at roofs), instanced grass and flowers that sway in the wind, then ambient occlusion, outlines, aerial perspective, light shafts, bloom, grading and sharpening (an optional painterly filter remains). A Look file can give materials procedural detail by name (`roof_tiles`, `wood`, `plaster`, `stone`, `lacquer`, `grass`, `terrain`, `lattice`; a rule with `none` and a colour recolours any material), and grass grows on `grass` and `terrain` materials; `terrain` also lays broad meadow patches and turns to beach sand just above the water and to faceted rock on steep slopes (the Look's `terrain` block), where no grass grows. Shallows and foam come from a top-down map around the camera and, further out, from a world map of the whole land drawn once. It runs on the backend-neutral `abe_engine::gpu` layer. The style is data: a game's Look file (JSON) sets shading, post-processing, sky palette, place and time, weather and named camera views, so different games share one renderer. With a scene that has materials or a `--look` file, the player flies: W/A/S/D move, Q/E down/up, arrows or left-drag look, Shift faster, 1–9 views, T/G time, C day cycle, Y weather, P pause, Esc quit. `--high` uses still-image quality. `abe-render --sequence START END FRAMES` renders a time-lapse of one view between two clock times as numbered PNGs in `--out-dir`, with water, clouds and weather moving at 24 frames a second.

Games add their own objects while they run as props: `Renderer::create_prop_mesh`, `create_prop_texture`, `create_prop_material` (the world's shading models, base-colour, normal and emissive textures, alpha cut-out, lamp emission, and the Look's detail rules matched by material name) and `create_prop_instance`, which places a mesh with a material and can be moved, hidden, given another material or destroyed at any time; `update_prop_mesh` replaces vertex positions for skinned or morphed meshes. Each frame the visible instances are batched by material and mesh into instanced draws in every pass (shadow cascades, the water reflection, the main view), so a prop is lit, shadowed and styled like the same mesh cooked into the world; the driver test compares the two pixel for pixel. `Renderer::pick` returns the nearest prop under a screen point, tested against its triangles, and `Renderer::project` maps a world point to the screen for interface anchored in the scene.

Shaders are WGSL (a documented subset, plus `var<push_constant>`) compiled at startup by the engine's own compiler, `abe-shader`, to SPIR-V, for Metal to MSL, for WebGPU back to standard WGSL and for WebGL 2 to GLSL ES 3.00. `cargo run -p abe-shader --example abe-shaderc -- shader.wgsl fragment fs_main out.spv` compiles one entry point and prints its reflection; an `out.wgsl` path writes the WebGPU WGSL instead.

## Run in a browser (WebGPU or WebGL 2)

The web player draws a cooked world with the same stylized renderer in a canvas through WebGPU, for Chrome and Edge 113+ and Safari 26 (macOS, iPhone, iPad), and through WebGL 2 everywhere else (older iPhones and iPads, Android GPUs without WebGPU, Firefox on Linux and Android, Linux Chrome). It is `crates/abe-web`, a wasm32 module without third-party crates that links both backends, plus `web/abe.js` and `web/index.html`, which own the device or context, the canvas, input and the frame loop. `web/abe.js` picks WebGPU when the browser has it; `?api=webgl2` forces WebGL 2 and `?api=webgpu` refuses the fallback. Build it with rustup:

```sh
rustup target add wasm32-unknown-unknown
scripts/build-web.sh                  # web/dist: index.html, abe.js, abe_web.wasm
```

Without bash, `cargo build --release -p abe-web --target wasm32-unknown-unknown` builds the same module (`target/wasm32-unknown-unknown/release/abe_web.wasm`); put it next to `web/index.html` and `web/abe.js`. A rustc without a wasm32 standard library (such as a distribution's `rustc-1.91`) rebuilds std from source and links with LLVM's `wasm-ld`; std's own dependencies (dlmalloc) come from crates.io on the first build:

```sh
ABE_WEB_TOOLCHAIN=build-std ABE_WEB_RUSTC=rustc-1.91 ABE_WEB_CARGO=cargo-1.91 \
  ABE_WEB_RUST_SRC=/path/to/rust-src-1.91/library scripts/build-web.sh
```

WebGPU needs a secure context: serve the folder from localhost or over https (a phone on the local network needs https), with the world and Look file at URLs the page can fetch. Keep game files out of this repository: build into a folder elsewhere and put them next to the page there:

```sh
scripts/build-web.sh ~/abe-site
cp world.abew look.json ~/abe-site/
python3 -m http.server -d ~/abe-site 8000
# http://localhost:8000/?scene=world.abew&look=look.json
```

`scene` and `look` name the files; `view=NAME`, `time=HOURS`, `weather=PRESET`, `seconds=S` and `paused=1` set the starting state; `quality=high` uses still-image quality; `dpr=N` caps the device pixel ratio (default 2, 1.5 on touch screens), `scale=F` scales the resolution and `size=WxH` fixes it. The controls are abe-player's (W/A/S/D, Q/E, arrows or drag, Shift, 1–9, T/G, C, Y, P) plus H for help. On a touch screen, drag with one finger to look and two to move; buttons change the time, weather and view. The page shows loading progress, a status line (clock, weather, frame rate, view, resolution) and every error, including WebGPU validation errors.

### Library mode: a game drives the engine

A game with its own interface uses the same module as a renderer instead of the player. `createEngine` from `web/abe.js` starts it on a canvas with a cooked world (or none: a scene of props only) and a Look; the page then sets the camera, clock and weather, creates and moves props, and calls `render()` when it wants a frame:

```js
import { createEngine } from './abe.js';
const engine = await createEngine({ canvas, wasm: 'abe_web.wasm', world: 'world.abew', look });
engine.fitCanvas();                                   // canvas size = CSS box x devicePixelRatio
engine.setView({ eye: [0, 4, 10], target: [0, 0.5, 0], fov: 45, shift: [0, 0.1] });
engine.setClock(18.5, performance.now() / 1000);     // time of day, animation clock
engine.setWeather('clear');
const mesh = engine.createMesh({ positions, normals, uvs, indices });
const art = engine.createTexture(canvasOrImageBitmap);
const felt = engine.createMaterial({ name: 'felt', color: [0.1, 0.3, 0.2, 1], baseTexture: art });
const chip = engine.createInstance(mesh, felt, matrix); // 16 numbers, column-major
engine.setInstances([{ instance: chip, matrix, visible: true }]);
engine.render();
engine.project([0, 1, 0]);                            // CSS pixels for overlays
engine.pick(event.offsetX, event.offsetY);            // the prop under the pointer
```

Views take an `up` direction (to look straight down), a lens `shift` (where the target lands in the image, in normalized device units with y up, so a camera can frame its subject around an interface without turning) and a `near` distance; Look files accept the same fields. The wasm exports behind it are `abe_rt_*` in `crates/abe-web/src/runtime.rs`; every handle is nonzero, and a refused call throws with the engine's message. `node web/test/run.cjs --engine world.abew` (with or without `--api webgl2`) checks it end to end: props drawn where `project` says, texture orientation, picking, hiding, batch moves, texture updates and errors.

On WebGPU, `abe_shader::compile_wgsl` (through `gpu::compile_shader`) turns each shader into standard WGSL: `var<push_constant>` becomes a uniform at `@group(3)` that the backend fills from a per-submission ring with dynamic offsets, and a generated entry point negates clip-space y because ABE uses Vulkan's convention. The browser derives barriers, `submit` and `present` do not wait, the frame is presented when the `requestAnimationFrame` callback returns, and `read_buffer` is unavailable because browsers map buffers asynchronously.

On WebGL 2, `abe_shader::compile_glsl` turns each shader into GLSL ES 3.00 (`crates/abe-shader/src/glsl.rs` documents the lowering): uniforms become std140 blocks, every texture/sampler pair a combined sampler on its own texture unit, push constants the block `abe_push` filled from a ring, and read-only storage buffers RGBA32UI textures that loads fetch from; the wrapper keeps Vulkan's clip space apart from the depth range, so images lie in memory as on Vulkan and the canvas is flipped when presented. A pipeline is a linked program plus fixed-function state; the backend (`src/gpu/webgl`) resolves bind groups at each draw and emits only the GL bindings that changed. `read_buffer` works and is synchronous. Compute, storage writes, storage textures and texture views do not exist there, so those calls report errors (the renderer uses none of them).

The browser test serves the built page and the files from where they are, renders each case in headless Chromium, reads back the canvas, and with `--abe-render` compares it with a native render of the same view, time and weather (`NAME-web.png`, `NAME-native.png`, `NAME-diff.png`, `results.json`). Any WebGPU error fails a case:

```sh
cargo run --release --example synthetic_world -- scenes
cargo build --release --bin abe-render
node web/test/run.cjs --dist web/dist --out web-test --abe-render target/release/abe-render \
  --case name=synthetic,scene=scenes/synthetic.abew,look=scenes/synthetic-look.json,view=test,time=17.5,weather=clear,size=640x360
ABE_WGSL_DUMP=wgsl cargo test -p abe-shader --test wgsl && node web/test/validate-wgsl.cjs wgsl
```

The last line compiles every corpus entry point with the browser's WGSL compiler. One is expected to fail: `compute.wgsl`'s `splat` calls `workgroupBarrier` after a non-uniform `return`, which WebGPU's uniformity analysis rejects and abe-shader does not check.

Without a GPU, Chromium runs WebGPU on SwiftShader; the scripts pass `--enable-unsafe-webgpu --enable-features=Vulkan --use-angle=vulkan --use-vulkan=swiftshader`, because with software compositing headless Chromium cannot allocate canvas images and drops the WebGPU device after a few frames. `--api webgl2` runs the cases and the self-test on the WebGL 2 backend (ANGLE). `--selftest` also runs the GPU-layer paths the renderer does not use (compute with push constants into storage textures and buffers, or on WebGL 2 a drawn image read back synchronously; copies, mip generation, a blit into the canvas), and `interact=1` drives the keyboard, mouse, a two-finger touch drag, an on-screen button and a window resize. In Chromium 153 on SwiftShader, the synthetic scene matches the native Vulkan render on llvmpipe at 47–50 dB PSNR (mean difference under 0.5 of 255) across noon, dusk, night and storm, and a 694,000-triangle game world (kept outside this repository) at 34–40 dB across five views, times and weathers (night rain is the lowest), with differences only at sub-pixel edges, thin rain streaks, foliage and texture filtering. On WebGL 2 (ANGLE on Vulkan, llvmpipe) the synthetic scene matches at 49–53 dB (mean difference about 0.3 of 255) across the same four times and weathers, the game world at 43–47 dB in two views, and the interaction test passes. `cargo test --test glsl` compiles and links every renderer shader pair with `glslangValidator` when it is installed. Chrome and Edge on Windows GPUs, Safari 26 and WebGL 2 on phones still need a device run.

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

The offscreen tests check depth, pixel readback, GPU camera projection and mixed static/changing geometry. The desktop test needs a display and exercises resize, hide/restore, and presenter recreation. Linux presentation and keyboard input have been exercised on Mesa llvmpipe under Xvfb with synchronization validation. Windows is cross-built and still needs a native device run. Android has the offscreen backend only. Browsers run the WebGPU backend (see above), and Apple Metal is described below. The Vulkan ABI declarations are adapted from [Khronos Vulkan-Headers v1.3.275](https://github.com/KhronosGroup/Vulkan-Headers/tree/v1.3.275) under the license in [docs/Vulkan-Registry-LICENSE.txt](docs/Vulkan-Registry-LICENSE.txt).

## Run on a Mac or an iPhone (Metal)

On macOS and iOS the same `gpu::Device` API runs on Metal, bound through the Objective-C runtime with no third-party code. `abe-shader` emits each entry point as Metal Shading Language next to its SPIR-V (`abe_shader::compile_msl`; `abe-shaderc shader.wgsl fragment fs_main out.metal` prints one), keeping WGSL's buffer layouts byte for byte, and the device compiles it when a pipeline is created. Status: compiled for `aarch64-apple-darwin`, `aarch64-apple-ios` and `aarch64-apple-ios-sim`, and every shader's MSL compiles against a stand-in Metal library with clang; nothing has run on Apple hardware yet.

macOS needs the Xcode command-line tools (`xcode-select --install`) and Rust:

```sh
scripts/apple/build-mac.sh                  # release abe-player and abe-render
scripts/apple/build-mac.sh --shaders        # also compile every renderer shader with Apple's metal compiler
scripts/apple/build-mac.sh --test           # also run the backend tests and render the test scene on Metal
target/aarch64-apple-darwin/release/abe-render --scene world.abew --look look.json --all-views --out-dir renders
target/aarch64-apple-darwin/release/abe-player --scene world.abew --look look.json
```

The player's controls are those listed above. Window sizes are pixels, so `--size 1600x900` is an 800x450-point window on a Retina display. `MTL_DEBUG_LAYER=1` turns on Metal's API validation (messages go to the console) and `ABE_METAL_DEVICE` picks a GPU by name on Macs with several. The built-in observatory needs the Vulkan presenter, so the Mac player needs `--scene`.

The iPhone app, `abe-ios`, renders a cooked world bundled into it. Drag one finger to look, drag two fingers to move, and tap the buttons to step the time of day, cycle the weather and cycle the Look file's views. Building it needs Xcode itself (the iOS SDK and simulators; `sudo xcode-select -s /Applications/Xcode.app`) and `rustup target add aarch64-apple-ios aarch64-apple-ios-sim`:

```sh
scripts/apple/build-ios.sh --sim --world world.abew --look look.json
ABE_IOS_TEAM=ABCDE12345 scripts/apple/build-ios.sh --device --world world.abew --look look.json
```

Installing on a device needs an Apple ID team (a free personal team works for development) and a development provisioning profile for the app's bundle identifier, `dev.abe.player.<team id in lower case>` unless `ABE_IOS_BUNDLE_ID` sets another. To get one, once: sign in with the Apple ID in Xcode > Settings > Accounts, connect and trust the iPhone, enable Developer Mode on it, create an iOS App project in Xcode with that bundle identifier and your team, and run it on the phone. The script then finds the profile Xcode downloaded, signs with your Apple Development identity (`ABE_IOS_PROFILE` and `ABE_IOS_IDENTITY` override the choice), installs with `xcrun devicectl` and launches the app. Free-team profiles expire after seven days.

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
