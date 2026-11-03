# Handoff: milestone 1, the agent command and physics (2026-11-03)

Work stopped at a usage limit. Nothing below is on `main` yet except this page. Read this first, then the
[design](https://github.com/abrahamahn/abe-engine/blob/physics-v2/docs/design/README.md) and
[roadmap](https://github.com/abrahamahn/abe-engine/blob/physics-v2/docs/roadmap.md) on the `physics-v2` branch.

## Branches

| Branch | What it holds | State |
| --- | --- | --- |
| `main` (`be10af5`) | the engine before this work | unchanged |
| `m1-scene` (`5150625`) | milestone 1: scene documents, prefabs, ops with undo, schemas, the asset pipeline, cooked worlds v4 with entity tables, world objects that move/hide/pick, ABE Studio (host + browser editor), the `abe` command for agents, the design docs and roadmap, and every fix from the milestone's adversarial review (43 defects) | verified: 259 Rust tests; editor end to end 46/46 on WebGPU and WebGL 2 |
| `physics-v2` (`fa7f142`) | `m1-scene` merged, plus `crates/abe-physics` and its integration (scene components, web library, Simulate in ABE Studio, `abe-build simulate`), plus three rounds of review fixes | verified: 379 `abe-physics` tests; the full suite below passed at `7842e05` |
| `fix-phys-convex` | narrow-phase convex fixes, in progress | WIP, unverified |
| `fix-phys-world3` | world/solver/query fixes, in progress | WIP, unverified |
| `fix-phys-gjk3` | GJK, hull and joint fixes, in progress | WIP, unverified |
| `physics` | an older copy of the physics work, before a rebase | stale: delete it (the rules forbid force pushes, so it was left) |

The full suite at `7842e05` (Linux x86-64): `cargo fmt --check`; clippy native and wasm32; unit tests; driver tests on
Mesa lavapipe with Vulkan validation (41); presentation under Xvfb; browser engine tests in headless Chromium 153 on
WebGPU (SwiftShader) and WebGL 2 (ANGLE on llvmpipe), 35 checks each, PSNR against native 47.7 and 51.6 dB; ABE Studio
end to end 50/50 on both, including the browser's physics state hash equal to `abe-build simulate`'s; `cargo check`
for Windows, macOS, iOS and Android (compiled only). No real device has run any of this.

## What remains, in order

1. **Finish the three WIP branches** (each stopped mid-work; read its commits, run `cargo test -p abe-physics`, finish
   with a regression test per item, merge into `physics-v2`):
   - `fix-phys-convex` (base `176de6f`): contact reduction must keep a resting manifold's points stable when the clip
     polygon has more than four corners (tolerance on depth, ties by feature key); the incident face must be chosen
     among the faces around the support vertex (an irregular hull 1 cm into a box reported +1.8 cm); the
     separating-axis edge loop is O(edges x edges) (5.8 ms for two 256-vertex hulls): prune edge pairs that cannot
     form a face of the Minkowski difference. After it lands, re-measure resting hexagonal prisms on meshes (the mesh
     contact merge and the convex reduction interact).
   - `fix-phys-world3` (base `4a2333a`): rolling balls sink because the substep separation turns the contact anchor
     with the ball (keep each shape's rounding radius in the constraint); continuous collision stops fast bodies at the
     seams between adjacent static boxes (take a convex target's time of impact only when the body's inner shape
     reaches it); shape casts hit the back faces of one-sided meshes and heightfields (skip them; `contacts_with`
     stays one-sided); a body with exactly two locked rotation axes drifts off its free axis over minutes (re-project
     onto its rest orientation turned about the free axis; test: a turntable spun at 3 rad/s keeps 3 rad/s for 10
     simulated minutes); very thin rods hinged across their width fall apart because the solver's inverse inertia is
     frozen for the whole step (rotate it each substep, keeping locked rows exactly zero).
   - `fix-phys-gjk3` (base `4a2333a`): a cast that starts inside the tolerance band reports a hit at fraction 0 even
     when moving away or parallel; GJK stops unconverged next to kilometre-sized faces; `validate` and `from_faces`
     accept coplanar neighbouring hull faces; accumulated joint impulses must saturate (bodies of huge inertia).
2. **Re-verify round two.** The verifiers of the second fix round (world: per-collider contact lists, warm-start
   matching, overflow-safe inputs, settings, CCD of elongated shapes, shape casts from contact; character: seams,
   platform teleports, pushes, tight gaps, wedges; scene: sheared boxes, step cap, schema docs) were stopped before
   reporting. Give each area an independent adversarial review before the push.
3. **Docs** in `docs/design/06-physics.md`: section 5, a sheared box collider is what is drawn (a box when its drawn
   edges are square, otherwise the hull of its eight drawn corners); section 6, platforms carry the character by
   their velocity (`set_body_pose` is a teleport and leaves it behind), openings need about 2·(radius + skin/8) plus a
   few millimetres, and surfaces that wedge the capsule together count as ground; section 7, `physicsStep` runs at most
   64 steps of at most 1/15 s per call. Record the platform evidence of the final run in `docs/roadmap.md`.
4. **Run the full suite** on the final `physics-v2` (commands below) and fix anything red.
5. **Rebuild a clean history on `main`** and push it as a fast-forward (never force):
   take each group from its source commit by removing the paths and checking them out again (so deleted files go),
   run `cargo check --workspace --all-targets` and `cargo fmt --check` for every commit, and finish with
   `git diff physics-v2` empty.

   | Commit | Paths | Source |
   | --- | --- | --- |
   | Scene documents, projects and the asset pipeline | `crates/abe-core`, `crates/abe-scene`, `crates/abe-assets`, `crates/abe-pipeline`, `src/bin/abe-build.rs`, `tests/abe_build.rs`; workspace members core, assets, pipeline, scene, shader, web; root dependencies plus pipeline and scene | `m1-scene` |
   | World objects move, hide and pick | `src/render`, `crates/abe-web`, `web/abe.js`, `web/abe.d.ts`, `tests/render.rs` (with `main`'s terrain red threshold, `1.4·g + 8`) | `m1-scene` |
   | ABE Studio | `crates/abe-studio`, `editor/`, `examples/studio_sample.rs`; member abe-studio | `m1-scene` |
   | The `abe` command for agents | `crates/abe-agent`, `Cargo.toml`, `Cargo.lock` | `m1-scene` |
   | Design docs and roadmap | `docs/`, `README.md` | `m1-scene` |
   | Two checks broken by distance-faded sharpening | `web/test/engine.html`, `tests/render.rs` | `m1-scene` |
   | abe-physics | `crates/abe-physics`, `docs/`; member abe-physics | `physics-v2` |
   | Physics in scenes, the web library and ABE Studio | everything else | `physics-v2` |

## Commands

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo clippy -p abe-web --target wasm32-unknown-unknown --locked --offline -- -D warnings
cargo test --workspace --locked --offline
VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json ABE_VULKAN_VALIDATION=1 \
  cargo test --release --locked --offline --test vulkan --test gpu --test render --test backend -- --ignored
xvfb-run -a -s "-screen 0 1280x720x24" cargo test --release --locked --offline --test present -- --ignored
scripts/build-web.sh web/dist
cargo build --release --bin abe-render --bin abe-build --example synthetic_world --example studio_sample -p abe-engine
cargo build --release -p abe-studio --bin abe-studio
cargo run --release --example synthetic_world -- SCENES
node web/test/run.cjs [--api webgl2] --dist web/dist --out OUT --abe-render target/release/abe-render --selftest \
  --engine SCENES/synthetic.abew \
  --case name=synthetic,scene=SCENES/synthetic.abew,look=SCENES/synthetic-look.json,view=test,time=17.5,weather=clear,size=640x360
node editor/test/run.cjs [--api webgl2] --out OUT
for t in x86_64-pc-windows-gnu aarch64-apple-darwin aarch64-apple-ios aarch64-linux-android; do
  cargo check --offline --target $t --workspace --exclude abe-web --bins --lib
done
```

Each worktree builds its own `target/` (2-3 GB): delete those of finished work when the disk fills, or the browser
runs fail with `ERR_INSUFFICIENT_RESOURCES`.

## Rules that hold

No third-party crates; samples and tests use synthetic data only; run every test level above and record per-platform
evidence; a game's migration stays reversible per platform; no game names or assets in this repository; never
force-push.

## The first game project

Its client rendering plan lives in its own repository. A revision of that plan (34 readability fixes, its decisions
renumbered so they do not collide with the project's own, and a module-by-module appendix) is written but not
committed: that repository's pre-commit hook needs packages this sandbox could not download, and skipping the hook was
not allowed. The founder has the patch; committing it is the founder's call. Vendoring the engine into that project
waits for the same.
