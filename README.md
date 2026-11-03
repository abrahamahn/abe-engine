# ABE Engine

ABE is a native Rust game engine in development. Its player, editor and build tools are intended for multiple games. Game assets, domain rules, network protocols and title-specific adapters belong in each game's repository. The [design](docs/design/README.md) explains where it is going: one runtime, one editor (ABE Studio) and one content pipeline shared by several games; the [roadmap](docs/roadmap.md) orders the work.

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

Games add their own objects while they run as props: `Renderer::create_prop_mesh`, `create_prop_texture`, `create_prop_material` (the world's shading models, base-colour, normal and emissive textures, alpha cut-out, lamp emission, and the Look's detail rules matched by material name) and `create_prop_instance`, which places a mesh with a material and can be moved, hidden, tinted (colour and alpha per instance, for fades and highlights), given another material or destroyed at any time; `update_prop_mesh` replaces vertex positions (a morphed mesh), and `set_prop_mesh_skin` with `set_prop_instance_joints` skins a mesh on the GPU: its vertices' joints and weights live in one buffer shared by every skinned mesh, each instance hands over its joint matrices, and every pass's vertex stage moves the vertices (picking tests them where they are drawn). Each frame the visible opaque instances are batched by material and mesh into instanced draws in every pass (shadow cascades, the water reflection, the main view), so a prop is lit, shadowed and styled like the same mesh cooked into the world; the driver test compares the two pixel for pixel. A material can instead blend (by alpha, or additively for glows) after the opaque scene, sorted by its draw order and far to near, or be an overlay: drawn after grading over the finished image in exactly its colour and texture, with or without a depth test against the scene, for labels, badges and rings; an opaque overlay that tests depth can also write it (`depth_write`), so a close-up drawn wholly in overlays (a table's felt, chips and curling cards seen under their own lens) hides its far pieces behind its near ones whatever order they draw in. `Renderer::pick` returns the prop under a screen point on the requested pick layers, tested against its triangles (an overlay drawn on top wins, otherwise the nearest), and `Renderer::project` maps a world point to the screen for interface anchored in the scene.

Run-time models come from GLB files (`abe_assets::model::load_glb`): node hierarchies, meshes (quantized geometry and sparse accessors decoded), materials, skins, morph targets and animation clips (linear, step and cubic-spline keys); images stay encoded for the host to decode, and Draco or meshopt compression must be decoded when the asset is prepared. `Renderer::create_model` turns a model's materials and static primitives into shared props, and each `create_model_instance` places one with a pose (`abe_engine::anim::Pose`): clips are mixed in with weights (1 replaces what a clip animates, less blends toward it, so cross-fades and two-clip blends are a matter of weights), nodes can be set directly, and `update_model_instance` applies the pose: skinned primitives follow its joint matrices on the GPU, and a morphed primitive is morphed on the CPU only when the weight of a target that moves its vertices changes (a figure's face targets leave its body alone), and then only the vertices its targets move (a face's few hundred of a whole figure), gathered at the front of its vertices as the model is uploaded (`anim::gather_morphed`) so they are rewritten in place as one short run (`update_prop_mesh_range`) however the modeller ordered them. Node visibility, tints, a node's world transform (to attach things to a socket) and the owner of a picked prop complete it.

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
engine.setLampBoost(1.9);                             // the lamps swell for a moment (1: as the hour has them)
const mesh = engine.createMesh({ positions, normals, uvs, indices });
const art = engine.createTexture(canvasOrImageBitmap);
const felt = engine.createMaterial({ name: 'felt', color: [0.1, 0.3, 0.2, 1], baseTexture: art });
const chip = engine.createInstance(mesh, felt, matrix); // 16 numbers, column-major
engine.setInstances([{ instance: chip, matrix, visible: true }]);
engine.render();
engine.project([0, 1, 0]);                            // CSS pixels for overlays
engine.pick(event.offsetX, event.offsetY);            // the prop under the pointer
```

A game hangs its own lights too: `Renderer::create_light` (`engine.createLight` on the web) places a point light or a spot light (a direction, and inner and outer half-angles) that is shaded like the cooked world's lamps in every pass and binned into the same screen tiles, and can be moved, recoloured or put out at any time; it shines at every hour unless it follows the lamps (lit only as the evening lights them). Up to 64 live at once. Up to two spots also cast shadows (`shadow`: how much of their light the shadows take): the opaque props whose materials cast shadows are drawn each frame into a shadow map seen from the light (`Quality::light_shadow_size` texels a side, 1024 in real time), filtered over eight turned taps; the cooked world is lit by them but does not block them.

A game draws its own effects on overlay props with prop shaders: `engine.createShader(source)` compiles WGSL that defines `fn shade(in: ShadeInput) -> vec4f` (straight linear colour and alpha), a material takes it as `shader` with twenty `params`, and `engine.setMaterialParams(material, params)` changes them every frame without rebuilding anything. A shader's pipelines (a blend mode; drawn on top, testing the scene's depth, or testing and writing it) are made the first time a prop draws with them, so a game can compile its effects up front without paying for variants it never uses. `ShadeInput` (`src/render/shaders/prop_shader_input.wgsl`) carries the UV, world position and normal, the instance tint, the material colour, its three textures sampled at the uv (`base`, `normal_map`, `emissive_map`: pictures the shader combines as it likes, such as a card's back and the face showing where it is peeled away; `base_tex`, `normal_tex` and `emissive_tex` with `material_sampler` sample them at another uv or mip bias) and the params; the shader may call the scene shader's functions but read no other resource (a shader that does is refused with a message), so the time comes in the params. The engine cuts, encodes and premultiplies its output as for any overlay, and builds it for every backend (`cargo test --test glsl` links one as GLSL ES; `tests/render.rs` and the browser check draw one).

Views take an `up` direction (to look straight down), a lens `shift` (where the target lands in the image, in normalized device units with y up, so a camera can frame its subject around an interface without turning), a `near` distance and a `focus` (depth of field: `{distance, depth, blur}`; nothing nearer than `distance` metres blurs, and past it the scene softens to its full `blur`, a share of the image height capped at 24 pixels, `depth` metres further on, so a table in focus keeps a busy garden behind it soft while overlays stay sharp; it is drawn with the painterly pass, which both quality presets run); Look files accept the same fields. `web/abe.d.ts` declares the library for TypeScript games, and `scripts/build-web.sh` ships it with `abe.js`. The wasm exports behind it are `abe_rt_*` in `crates/abe-web/src/runtime.rs`; every handle is nonzero, and a refused call throws with the engine's message. `engine.loadModel(url)` loads a GLB with the browser decoding its images, and `createModelInstance`, `modelClip`, `setModelNode`, `modelMorph`, `updateModel` and `modelNodeWorld` pose and place it. `node web/test/run.cjs --engine world.abew` (with or without `--api webgl2`) checks it end to end: props drawn where `project` says, texture orientation, picking and pick layers, hiding, batch moves, texture updates, an overlay's exact colour on the 8-bit canvas, blending, tints, a GLB built in the page (skinned, PNG-textured) bent by its clip and picked back, a prop shader and its parameters, and errors.

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

The cooker accepts embedded glTF 2.0 triangle meshes (float positions, normals and UVs, indices), PBR material factors, PNG base-colour, normal and emissive textures, `KHR_materials_emissive_strength`, `KHR_lights_punctual` point lights, alpha modes, double-sidedness, `extras.abe.shading` (`standard`, `foliage`, `water`, `unlit`) and `extras.abe.emission` (`lamp`: scaled by the time of day), and scene-node transforms. ABEW v3 stores meshes once, references them from affine instances, and adds materials, textures and lights; v1 and v2 files still load. Unsupported codecs, skins, animation and other image types are reported explicitly; games decode Draco before cooking.

`abe-build model file.glb` reads a GLB as the run-time model loader does (`createEngine().loadModel` in a page) and lists its nodes, primitives with their materials, skins and clips, so an asset pipeline can check a model before it ships.

For browsers, `abe-build cook --compact --keep-images` writes worlds about a fifth the size: `--compact` codes each mesh (vertices renumbered by first use, positions on a 0.5 mm grid, 12-bit octahedral normals, UVs on a 1/8192 grid, all delta-coded and deflated; about 7 bytes a triangle for a large estate), and `--keep-images` keeps PNG, JPEG and WebP images (WebP from `EXT_texture_webp`) as files instead of mip chains. The page decodes JPEG and WebP with the browser's decoders and the engine decodes PNG itself (a 2D canvas would premultiply transparent pixels away), and mip chains are made on the GPU. Natively, PNG is decoded and other encoded formats are refused with a message. The portable `abe-core` and `abe-assets` crates have no external dependencies.

## Edit a scene in ABE Studio

ABE Studio is the editor: a local host, `abe-studio`, that owns a project folder, and a browser interface whose viewport is the engine's web runtime. A project is a folder with `abe.project.json`; scenes and prefabs are canonical JSON documents, and every other asset has a `.meta.json` sidecar with its permanent id ([data design](docs/design/04-data.md)).

```sh
scripts/build-web.sh web/dist                                   # the runtime the viewport uses
cargo run --release --example studio_sample -- ~/abe-sample     # a synthetic project
cargo run --release -p abe-studio -- ~/abe-sample               # then open http://localhost:7480/
```

Open a scene from Assets (a static model imports as a scene with "Import as scene": one entity per glTF node, names and hierarchy kept; skinned, morphed and animated models stay run-time models). Click an object in the viewport to select it; W, E and R switch the gizmo between move, rotate and scale (hold Ctrl to snap); right-drag orbits, Shift+right-drag or the middle button pans, the wheel zooms, F frames the selection. The inspector is drawn from component schemas; Ctrl+Z and Ctrl+Shift+Z undo and redo (the host keeps the history), Ctrl+S saves, Ctrl+D duplicates, and Delete deletes when the viewport or the outliner has the keyboard (a field being typed into keeps Backspace and Delete). Structural changes re-cook the scene; moves apply at once. The interface is in English and Korean (the button at the top right), and the page reconnects by itself when the host restarts. The host answers only this machine at its own port (a port forward must keep the port number). `--lan` also listens on the local network and prints a URL with a pairing token, which every browser, this machine's included, opens once; a cookie keeps it after that.

From the command line, `abe-build import PROJECT content/model.glb content/model.scene.json` imports a model as a scene, and `abe-build cook-scene PROJECT content/model.scene.json out.abew` cooks a scene into an ABEW v4 world that keeps its entities (`abe-build inspect` lists them). In a page, `engine.worldEntities()` returns the table, and `setWorldInstance`, `pickWorld` and `setWorldLight` move, hide and pick a cooked world's objects.

## Simulate physics

`crates/abe-physics` is the engine's own rigid-body physics, with no dependency beyond `abe-core`: spheres,
capsules, boxes, convex hulls, triangle meshes and heightfields; static, kinematic and dynamic bodies; ball, hinge,
slider, fixed and distance joints with limits, motors and springs; sensors and contact events; ray, shape-cast and
overlap queries; continuous collision for fast bodies; sleeping islands; and a kinematic character controller that
slides, climbs steps, keeps off steep slopes and rides platforms. It steps in fixed slices with a soft-step solver and
is deterministic: only `+`, `−`, `×`, `÷` and `sqrt` run in a step, so a browser and a native build reach the same
state hash ([physics design](docs/design/06-physics.md)).

Scenes author it with `rigid_body` and `collider` components (a collider with no rigid body on its entity or above it is static scenery; one under a body is part of that body).
In ABE Studio, **Simulate** (P) re-cooks the scene, runs it at 60 Hz and puts everything back on a second press; in
a page, `physicsStart()`, `physicsStep(dt, steps)` (at most 64 steps of at most 1/15 s per call), `physicsStop()`, `physicsHash()` and `physicsError()` do the same for any cooked world.

```sh
cargo run --release --bin abe-build -- simulate scene.abew 120   # state hash and resting places after 2 s
VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json \
  cargo run --release --example physics_demo -- physics.png       # a pyramid hit by a ball, five moments
```

[Explore the six architecture layers](docs/architecture.html) (download and open the self-contained HTML to use the selectors; the [design](docs/design/README.md) is current where they differ). [Engine milestones](docs/roadmap.md).

## For agents: the abe command

`abe` lets AI agents (and people) inspect, validate, edit, diff, undo and render scenes without touching engine source. Every command prints one line of JSON: `{"ok":true,...}`, or `{"ok":false,"error":{"code":"...","message":"..."}}` with exit code 1 (2 for a wrong command line). `--pretty` indents it, and `abe help` lists the commands. The error codes are stable: `not-found`, `ambiguous-ref`, `bad-edit`, `invalid`, `changed-since`, `io`, `cook`, `render` and `usage`. The same inputs give the same bytes.

PROJECT is a project folder and SCENE a scene's (or prefab's) asset id or its path in the project. An entity reference (REF) is the entity's 16-hex id, its name when no other entity has it, or a `/`-separated path of names from a root (`Tower/Tower top`; `\/` is a slash inside a name). The examples use the synthetic studio sample:

```sh
cargo run --release --example studio_sample -- ~/abe-sample
cargo build --release -p abe-agent --bin abe && export PATH="$PWD/target/release:$PATH"
P=~/abe-sample S=content/scenes/blocks.scene.json
```

**Inspect** lists entities in tree order with their id, name, path, parent, depth, number of children and components. `--entity REF` starts at one entity, `--component KIND` keeps the entities that have that component (and shows only it), `--depth N` stops N levels down, and `--defaults` fills in every field a component leaves out from the schemas:

```sh
abe scene inspect $P $S --entity "Tower/Tower top" --defaults
# {"ok":true,"scene":{"id":"002e3cec...","name":"Blocks","path":"content/scenes/blocks.scene.json","kind":"scene"},"total":8,"count":1,
#  "entities":[{"id":"4146b57080b66538","name":"Tower top","path":"Tower/Tower top","depth":0,"parent":"7686dad007fbff4c","children":0,
#  "components":{"mesh":{"model":"861cf008...","node":"Tower/Tower top","static":true},"transform":{"rotation":[0,0,0,1],"scale":[0.5,0.25,0.5],"translation":[0,0.75,0]}}}]}
```

**Validate** checks the scene against the engine's and the game's component schemas with `abe_scene::scene::check`, the rules ABE Studio applies to every edit and the cook applies before it builds. Those problems are errors. References to assets the project lacks (or of the wrong kind) and to entities the scene lacks are warnings. The exit code is 1 when there is an error:

```sh
abe scene validate $P $S
# {"ok":true,"scene":{...},"errors":0,"warnings":0,"issues":[]}
```

**Apply** runs edits from a file, or from standard input with `-`, as one transaction. An edit is `set` (`entity`, `path`, `value`), `remove` (`entity`, `path`), `create` (`name`, optional `parent`, `components` and `id`), `delete` (`entity`, with what lies beneath it), `reparent` (`entity`, `parent`: a REF or `null` for the top level) or `duplicate` (`entity`, optional `name`). References resolve against the document as the earlier edits left it. A path is a dotted string below the entity (`name`, `order`, `components...`, `prefab...`) or an array of keys. A game's kinds keep their dot (`components.arena.spawn.team`), and arrays are set whole. The edits become the ops ABE Studio sends and are judged by the studio's rule that an edit may not add a problem the document did not have. The file is written with the project's canonical writer, so it reads exactly as if the edit had been made and saved in the studio. Every change is reported with its edit, entity, name, path, before and after:

```sh
abe scene apply $P $S - <<'EOF'
{"label": "Brighter lamp, a bench", "edits": [
  {"op": "set", "entity": "Lamp", "path": "components.light.intensity", "value": 30},
  {"op": "create", "name": "Bench", "parent": "Tower", "components": {"transform": {"translation": [1, 0, 0]}}},
  {"op": "reparent", "entity": "Block A", "parent": "Block B"},
  {"op": "duplicate", "entity": "Tower", "name": "Tower 2"},
  {"op": "remove", "entity": "Ground", "path": "components.mesh"},
  {"op": "delete", "entity": "Block B"}]}
EOF
# {"ok":true,"scene":{...},"dry_run":false,"written":true,"edits":6,"changes":[{"edit":0,"entity":"0f0413101eb4ecef","name":"Lamp",
#  "path":"components.light.intensity","before":18,"after":30},{"edit":1,"entity":"6ff31e963eaa6a60","name":"Bench","path":"","before":null,"after":{...}},...],
#  "created":[{"edit":1,"entity":"6ff31e963eaa6a60","name":"Bench"},{"edit":3,"entity":"0e4700abbbdbd9fe","name":"Tower 2"}],"issues":[],"hash":"2c983daa..."}
```

If any edit fails, or the result has a problem the document did not have, nothing is written and the answer names the edit (`error.edit`) and the problems (`error.issues`), as in `{"ok":false,"error":{"code":"invalid","message":"...","edit":0,"issues":[{"path":"entities.0f0413101eb4ecef.components.light.intensity","message":"-1 is out of range","entity":"0f0413101eb4ecef","name":"Lamp","edit":0}]}}`. `--dry-run` gives the same answer without writing. New entity ids are drawn from a generator seeded by the document and the edits, so a dry run reports the ids a real run writes. A written transaction is recorded with its inverse ops and the hashes of the file before and after it in `PROJECT/.abe/history/<scene id>.jsonl`.

**Undo** takes back the last transaction `abe` recorded for the scene, provided the file still has the bytes that transaction left. Otherwise it refuses with `changed-since`, for example after a save in ABE Studio or a hand edit. The original bytes come back exactly, even for a file written by hand. `--dry-run` reports without writing:

```sh
abe scene undo $P $S
# {"ok":true,"scene":{...},"dry_run":false,"written":true,"undone":{"label":"Brighter lamp, a bench","edits":6},"changes":[...],"restored":true,"remaining":0,"hash":"15180ce4..."}
```

**Diff** compares two scene documents by entity id. It reports entities `added`, `removed`, `renamed` and `reparented`, every other `changed` field (path, before, after) and the document's own fields, in a fixed order. One side may be `-` (standard input), so in a project under git `git show HEAD:$S | abe scene diff - $S` reviews uncommitted work:

```sh
abe scene diff before.scene.json $P/$S
# {"ok":true,"a":{"file":"before.scene.json",...},"b":{...},"equal":false,"document":[],"added":[{"id":"0e4700abbbdbd9fe","name":"Tower 2","parent":null,"value":{...}},...],
#  "removed":[...],"renamed":[],"reparented":[...],"changed":[{"entity":"0f0413101eb4ecef","name":"Lamp","path":"components.light.intensity","before":18,"after":30},...]}
```

**Render** cooks the scene in memory as `abe-build cook-scene` would and draws a still with the offscreen renderer exactly as `abe-render` draws the cooked file (for the same view, time and weather the PNG is byte for byte the one `abe-render` writes). It needs a Vulkan driver; `VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json` selects Mesa's software lavapipe. The view is `--camera REF`, else the scene's first camera entity, else the Look file's first view, else an overview. A camera looks along its transform's -Z with +Y up. Time, weather and Look come from the scene's `environment` component, and `--time` and `--weather` override them. The defaults are those of `abe-render` (1920x1080, 8 samples; `--samples N` changes it):

```sh
abe render $P $S --out blocks.png --size 960x540 --samples 4
# {"ok":true,"scene":{...},"file":"blocks.png","bytes":...,"sha256":"...","width":960,"height":540,"samples":4,
#  "view":{"name":"Overview camera","eye":[10,7,12],"target":[...],"up":[...],"fov":50,"source":"camera","entity":"3571b98d187af3e5"},
#  "time":16.5,"weather":"clear","look":"content/looks/day.look.json","entities":8,"device":"llvmpipe (LLVM 20.1.2, 256 bits)","warnings":[]}
```

The commands are a library too: `abe_agent::run` (`crates/abe-agent`) is the whole command line as a function, which its tests drive on synthetic projects. ABE Studio reads a document again when it is reopened (a page reload) with no unsaved edits, so changes `abe` made show up there; and it refuses to save over a file that changed on disk since it read it (the editor asks before overwriting). Until the command sends its transactions through a running host (planned), make a change in one of the two at a time.

## Verify

```sh
cargo test --workspace --locked --offline
cargo test --locked --offline --test vulkan -- --ignored
cargo test --locked --offline --test present -- --ignored
cargo test --locked --offline --test gpu --test render -- --ignored
cargo test --locked --offline -p abe-agent --test binary -- --ignored     # abe render through the binary
node editor/test/run.cjs --out editor-test                 # ABE Studio end to end (release builds and web/dist first)
node editor/test/run.cjs --api webgl2 --out editor-test-gl
```

`editor/test/run.cjs` writes the synthetic sample, starts `abe-studio` and drives the interface in headless Chromium with real pointer and key events: select by clicking, move with the gizmo, undo and redo, a parent carrying its child, renaming, adding a component, saving, reloading, and cooking and rendering the saved scene. Its header lists the release builds it needs.

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
