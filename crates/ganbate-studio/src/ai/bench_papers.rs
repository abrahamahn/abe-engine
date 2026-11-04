//! Full 3-Paper PhD Dissertation Empirical Benchmark Suite (`Paper 1` + `Paper 2` + `Paper 3`).
//!
//! Executes live, deterministic, zero-dependency, empirically falsifiable experiments covering:
//! - **Paper 1 (UIST / CHI / SIGGRAPH):** `N = 30` `.gbs` relational spatial authoring tasks
//!   (`spawn`, `snap onto`, `socket`, `beside`, `ring`, `look_at`, `cam frame`), `Document`
//!   `Op::Set` / `Op::Remove` SHA-256 `Ctrl+Z` invertibility, and `<100ms` `scene_verify` oracle
//!   vs. an ungrounded raw-coordinate perturbation baseline.
//! - **Paper 2 (ICSE / FSE / NeurIPS):** Deterministic workspace `Cargo.toml` AST/manifest audit
//!   (`14` internal crates, `0` external `crates.io` dependencies, `#![forbid(unsafe_code)]` crates)
//!   plus live read-only Ecosystem X-Ray (`code-surface-catalog.json`) and AI Context Pyramid
//!   (`ai-context-pyramid.json`) provenance extraction.
//! - **Paper 3 (EuroSys / ASPLOS / SIGGRAPH / IEEE VR):** 4-target zero-dependency WGSL shader
//!   compiler (`SPIR-V 1.0`, `MSL`, `WebGPU WGSL`, `GLSL ES 3.00`), `.gbew` v4 binary mesh cooking
//!   (`~7 B/triangle` + `0.5 mm` grid round-trip verification), `10,000`-step `#![forbid(unsafe_code)]`
//!   bit-exact `ganbate-physics` `state_hash` determinism, and `90Hz VR` (`11.11ms`) closed-loop
//!   frame-budget governance.

use std::collections::BTreeMap;
use std::fs;
use std::hint::black_box;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use ganbate_assets::cooked;
use ganbate_core::Vec3;
use ganbate_core::id::Ids;
use ganbate_core::json::{self, Value};
use ganbate_core::world::{EntityRecord, EntityTable, Instance, Material, Mesh, Transform, World};
use ganbate_core::xr::FrameBudgetGovernor;
use ganbate_physics::math::{Quat as PhysQuat, Vec3 as PhysVec3};
use ganbate_physics::shape::Shape as PhysShape;
use ganbate_physics::world::{BodyDesc, BodyKind, ColliderDesc, PhysicsWorld, Settings};
use ganbate_pipeline::project::{Kind, Project};
use ganbate_scene::doc::{Document, Op};
use ganbate_scene::scene;
use ganbate_scene::schema::engine_registry;
use ganbate_shader::{Stage, compile, compile_glsl, compile_msl, compile_wgsl};

use super::super::catalog::crystallize_brick;
use super::super::gbs::execute_gbs;
use super::super::verify::{VerifyOptions, verify_scene};
use super::LatencyDistributionUs;
use super::{round_f64, sha256_hex, summarize_samples_ns};

const SPIRV_MAGIC: u32 = 0x0723_0203;
const MAX_XRAY_HEADER_BYTES: usize = 64 * 1024;
const MAX_XRAY_FALLBACK_BYTES: u64 = 16 * 1024 * 1024;
const PHYSICS_DETERMINISM_STEPS: usize = 10_000;
const PHYSICS_DT_SECONDS: f32 = 1.0 / 60.0;

struct TempBenchDir(PathBuf);

impl Drop for TempBenchDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Paper 1 (`UIST / CHI / SIGGRAPH`): `N = 30` `.gbs` relational spatial authoring, `Document`
/// `Ctrl+Z` SHA-256 invertibility, and `<100ms` `scene_verify` oracle empirical metrics.
#[derive(Clone, Debug, PartialEq)]
pub struct Paper1SpatialSuite {
    pub total_tasks: usize,
    pub tasks_passed: usize,
    pub op_ctrl_z_sha256_recovered: usize,
    pub op_ctrl_z_sha256_recovery_rate: f64,
    pub op_set_and_remove_exercised: bool,
    pub gbs_snapped_lint_free_tasks: usize,
    pub gbs_snapped_spatial_lint_free_rate: f64,
    pub ungrounded_baseline_linted_tasks: usize,
    pub ungrounded_baseline_lint_free_rate: f64,
    pub ungrounded_baseline_floating_lints: usize,
    pub ungrounded_baseline_penetration_lints: usize,
    pub screen_projection_verified_tasks: usize,
    pub budget_within_limits_tasks: usize,
    pub verify_under_100ms_sla: bool,
    pub gbs_compile_apply_us: LatencyDistributionUs,
    pub scene_verify_us: LatencyDistributionUs,
}

impl Paper1SpatialSuite {
    pub fn verified(&self) -> bool {
        self.total_tasks == 30
            && self.tasks_passed == self.total_tasks
            && self.op_ctrl_z_sha256_recovered == self.total_tasks
            && self.op_set_and_remove_exercised
            && self.gbs_snapped_lint_free_tasks == self.total_tasks
            && self.ungrounded_baseline_linted_tasks == self.total_tasks
            && self.screen_projection_verified_tasks == self.total_tasks
            && self.budget_within_limits_tasks == self.total_tasks
            && self.verify_under_100ms_sla
    }

    pub fn to_json_value(&self) -> Value {
        let fmt_lat = |l: &LatencyDistributionUs| -> Value {
            Value::Object(BTreeMap::from([
                ("id".into(), Value::String(l.id.clone())),
                ("label".into(), Value::String(l.label.clone())),
                ("iterations".into(), Value::Number(l.iterations as f64)),
                ("min_us".into(), Value::Number(round_f64(l.min_us, 3))),
                ("p50_us".into(), Value::Number(round_f64(l.p50_us, 3))),
                ("p95_us".into(), Value::Number(round_f64(l.p95_us, 3))),
                ("p99_us".into(), Value::Number(round_f64(l.p99_us, 3))),
                ("max_us".into(), Value::Number(round_f64(l.max_us, 3))),
                ("mean_us".into(), Value::Number(round_f64(l.mean_us, 3))),
            ]))
        };

        Value::Object(BTreeMap::from([
            ("verified".into(), Value::Bool(self.verified())),
            ("total_tasks".into(), Value::Number(self.total_tasks as f64)),
            (
                "tasks_passed".into(),
                Value::Number(self.tasks_passed as f64),
            ),
            (
                "op_ctrl_z_sha256_recovered".into(),
                Value::Number(self.op_ctrl_z_sha256_recovered as f64),
            ),
            (
                "op_ctrl_z_sha256_recovery_rate".into(),
                Value::Number(round_f64(self.op_ctrl_z_sha256_recovery_rate, 4)),
            ),
            (
                "op_set_and_remove_exercised".into(),
                Value::Bool(self.op_set_and_remove_exercised),
            ),
            (
                "gbs_snapped_lint_free_tasks".into(),
                Value::Number(self.gbs_snapped_lint_free_tasks as f64),
            ),
            (
                "gbs_snapped_spatial_lint_free_rate".into(),
                Value::Number(round_f64(self.gbs_snapped_spatial_lint_free_rate, 4)),
            ),
            (
                "ungrounded_baseline_linted_tasks".into(),
                Value::Number(self.ungrounded_baseline_linted_tasks as f64),
            ),
            (
                "ungrounded_baseline_lint_free_rate".into(),
                Value::Number(round_f64(self.ungrounded_baseline_lint_free_rate, 4)),
            ),
            (
                "ungrounded_baseline_floating_lints".into(),
                Value::Number(self.ungrounded_baseline_floating_lints as f64),
            ),
            (
                "ungrounded_baseline_penetration_lints".into(),
                Value::Number(self.ungrounded_baseline_penetration_lints as f64),
            ),
            (
                "screen_projection_verified_tasks".into(),
                Value::Number(self.screen_projection_verified_tasks as f64),
            ),
            (
                "budget_within_limits_tasks".into(),
                Value::Number(self.budget_within_limits_tasks as f64),
            ),
            (
                "verify_under_100ms_sla".into(),
                Value::Bool(self.verify_under_100ms_sla),
            ),
            (
                "gbs_compile_apply_us".into(),
                fmt_lat(&self.gbs_compile_apply_us),
            ),
            ("scene_verify_us".into(), fmt_lat(&self.scene_verify_us)),
        ]))
    }
}

/// Paper 2 (`ICSE / FSE / NeurIPS`): Zero-External-Dependency Cargo purity & live Ecosystem
/// Provenance (`code-surface-catalog.json` + `ai-context-pyramid.json`) metrics.
#[derive(Clone, Debug, PartialEq)]
pub struct Paper2EcosystemGovernance {
    pub workspace_crates: usize,
    pub internal_path_dependencies: usize,
    pub external_crates_io_dependencies: usize,
    pub forbid_unsafe_crates: usize,
    pub zero_external_deps_verified: bool,
    pub xray_available: bool,
    pub indexed_surfaces: u64,
    pub code_files: u64,
    pub dependency_edges: u64,
    pub avg_implementation_pct: f64,
    pub avg_integration_pct: f64,
    pub reclaimable_loc: u64,
    pub pyramid_available: bool,
    pub health_score: f64,
    pub cluster_count: u64,
    pub l0_tokens: u64,
    pub max_l1_tokens: u64,
    pub pyramid_within_budget: bool,
}

impl Paper2EcosystemGovernance {
    pub fn verified(&self) -> bool {
        self.workspace_crates >= 14
            && self.external_crates_io_dependencies == 0
            && self.zero_external_deps_verified
            && self.forbid_unsafe_crates >= 1
            && (!self.pyramid_available || self.pyramid_within_budget)
    }

    pub fn to_json_value(&self) -> Value {
        Value::Object(BTreeMap::from([
            ("verified".into(), Value::Bool(self.verified())),
            (
                "workspace_crates".into(),
                Value::Number(self.workspace_crates as f64),
            ),
            (
                "internal_path_dependencies".into(),
                Value::Number(self.internal_path_dependencies as f64),
            ),
            (
                "external_crates_io_dependencies".into(),
                Value::Number(self.external_crates_io_dependencies as f64),
            ),
            (
                "forbid_unsafe_crates".into(),
                Value::Number(self.forbid_unsafe_crates as f64),
            ),
            (
                "zero_external_deps_verified".into(),
                Value::Bool(self.zero_external_deps_verified),
            ),
            ("xray_available".into(), Value::Bool(self.xray_available)),
            (
                "indexed_surfaces".into(),
                Value::Number(self.indexed_surfaces as f64),
            ),
            ("code_files".into(), Value::Number(self.code_files as f64)),
            (
                "dependency_edges".into(),
                Value::Number(self.dependency_edges as f64),
            ),
            (
                "avg_implementation_pct".into(),
                Value::Number(round_f64(self.avg_implementation_pct, 2)),
            ),
            (
                "avg_integration_pct".into(),
                Value::Number(round_f64(self.avg_integration_pct, 2)),
            ),
            (
                "reclaimable_loc".into(),
                Value::Number(self.reclaimable_loc as f64),
            ),
            (
                "pyramid_available".into(),
                Value::Bool(self.pyramid_available),
            ),
            (
                "health_score".into(),
                Value::Number(round_f64(self.health_score, 2)),
            ),
            (
                "cluster_count".into(),
                Value::Number(self.cluster_count as f64),
            ),
            ("l0_tokens".into(), Value::Number(self.l0_tokens as f64)),
            (
                "max_l1_tokens".into(),
                Value::Number(self.max_l1_tokens as f64),
            ),
            (
                "pyramid_within_budget".into(),
                Value::Bool(self.pyramid_within_budget),
            ),
        ]))
    }
}

/// Per-backend compilation metrics for the 4-target zero-dependency `ganbate-shader` compiler.
#[derive(Clone, Debug, PartialEq)]
pub struct ShaderTargetMetrics {
    pub id: String,
    pub label: String,
    pub vertex_bytes: usize,
    pub fragment_bytes: usize,
    pub total_bytes: usize,
    pub compile_us: f64,
    pub sha256: String,
    pub verified: bool,
}

/// `.gbew` v4 ahead-of-time binary mesh cooking & `0.5 mm` grid round-trip verification metrics.
#[derive(Clone, Debug, PartialEq)]
pub struct GbewCookMetrics {
    pub version: u32,
    pub vertex_count: usize,
    pub triangle_count: usize,
    pub raw_mesh_bytes: usize,
    pub cooked_gbew_bytes: usize,
    pub bytes_per_triangle: f64,
    pub compression_ratio: f64,
    pub max_position_error_mm: f64,
    pub cook_us: f64,
    pub decode_us: f64,
    pub roundtrip_verified: bool,
}

/// `10,000`-step `#![forbid(unsafe_code)]` bit-exact `ganbate-physics` determinism metrics.
#[derive(Clone, Debug, PartialEq)]
pub struct PhysicsDeterminismMetrics {
    pub steps: usize,
    pub dt_seconds: f64,
    pub dynamic_bodies: usize,
    pub hash_initial_hex: String,
    pub hash_run_a_hex: String,
    pub hash_run_b_hex: String,
    pub bit_exact_verified: bool,
    pub zero_nan_verified: bool,
    pub mean_us_per_step: f64,
    pub run_a_ms: f64,
    pub run_b_ms: f64,
}

/// Paper 3 (`EuroSys / ASPLOS / SIGGRAPH / IEEE VR`): 4-Target Shader Compiler, `.gbew` Cooker,
/// `10,000`-step Bit-Exact Physics Determinism, and `90Hz VR` Closed-Loop Frame Governance.
#[derive(Clone, Debug, PartialEq)]
pub struct Paper3PolyglotSystems {
    pub shader_targets: Vec<ShaderTargetMetrics>,
    pub all_four_targets_verified: bool,
    pub gbew: GbewCookMetrics,
    pub physics: PhysicsDeterminismMetrics,
    pub vr_90hz_target_ms: f64,
    pub vr_90hz_step_down_frames: usize,
    pub vr_90hz_recovery_frames: usize,
    pub vr_90hz_governor_verified: bool,
}

impl Paper3PolyglotSystems {
    pub fn verified(&self) -> bool {
        self.all_four_targets_verified
            && self.shader_targets.len() == 4
            && self.gbew.roundtrip_verified
            && self.gbew.triangle_count >= 2_048
            && self.physics.bit_exact_verified
            && self.physics.zero_nan_verified
            && self.vr_90hz_governor_verified
    }

    pub fn to_json_value(&self) -> Value {
        let targets = Value::Array(
            self.shader_targets
                .iter()
                .map(|t| {
                    Value::Object(BTreeMap::from([
                        ("id".into(), Value::String(t.id.clone())),
                        ("label".into(), Value::String(t.label.clone())),
                        ("vertex_bytes".into(), Value::Number(t.vertex_bytes as f64)),
                        (
                            "fragment_bytes".into(),
                            Value::Number(t.fragment_bytes as f64),
                        ),
                        ("total_bytes".into(), Value::Number(t.total_bytes as f64)),
                        (
                            "compile_us".into(),
                            Value::Number(round_f64(t.compile_us, 3)),
                        ),
                        ("sha256".into(), Value::String(t.sha256.clone())),
                        ("verified".into(), Value::Bool(t.verified)),
                    ]))
                })
                .collect(),
        );

        let gbew = Value::Object(BTreeMap::from([
            ("version".into(), Value::Number(self.gbew.version as f64)),
            (
                "vertex_count".into(),
                Value::Number(self.gbew.vertex_count as f64),
            ),
            (
                "triangle_count".into(),
                Value::Number(self.gbew.triangle_count as f64),
            ),
            (
                "raw_mesh_bytes".into(),
                Value::Number(self.gbew.raw_mesh_bytes as f64),
            ),
            (
                "cooked_gbew_bytes".into(),
                Value::Number(self.gbew.cooked_gbew_bytes as f64),
            ),
            (
                "bytes_per_triangle".into(),
                Value::Number(round_f64(self.gbew.bytes_per_triangle, 3)),
            ),
            (
                "compression_ratio".into(),
                Value::Number(round_f64(self.gbew.compression_ratio, 2)),
            ),
            (
                "max_position_error_mm".into(),
                Value::Number(round_f64(self.gbew.max_position_error_mm, 4)),
            ),
            (
                "cook_us".into(),
                Value::Number(round_f64(self.gbew.cook_us, 3)),
            ),
            (
                "decode_us".into(),
                Value::Number(round_f64(self.gbew.decode_us, 3)),
            ),
            (
                "roundtrip_verified".into(),
                Value::Bool(self.gbew.roundtrip_verified),
            ),
        ]));

        let physics = Value::Object(BTreeMap::from([
            ("steps".into(), Value::Number(self.physics.steps as f64)),
            (
                "dt_seconds".into(),
                Value::Number(round_f64(self.physics.dt_seconds, 6)),
            ),
            (
                "dynamic_bodies".into(),
                Value::Number(self.physics.dynamic_bodies as f64),
            ),
            (
                "hash_initial_hex".into(),
                Value::String(self.physics.hash_initial_hex.clone()),
            ),
            (
                "hash_run_a_hex".into(),
                Value::String(self.physics.hash_run_a_hex.clone()),
            ),
            (
                "hash_run_b_hex".into(),
                Value::String(self.physics.hash_run_b_hex.clone()),
            ),
            (
                "bit_exact_verified".into(),
                Value::Bool(self.physics.bit_exact_verified),
            ),
            (
                "zero_nan_verified".into(),
                Value::Bool(self.physics.zero_nan_verified),
            ),
            (
                "mean_us_per_step".into(),
                Value::Number(round_f64(self.physics.mean_us_per_step, 3)),
            ),
            (
                "run_a_ms".into(),
                Value::Number(round_f64(self.physics.run_a_ms, 2)),
            ),
            (
                "run_b_ms".into(),
                Value::Number(round_f64(self.physics.run_b_ms, 2)),
            ),
        ]));

        Value::Object(BTreeMap::from([
            ("verified".into(), Value::Bool(self.verified())),
            ("shader_targets".into(), targets),
            (
                "all_four_targets_verified".into(),
                Value::Bool(self.all_four_targets_verified),
            ),
            ("gbew_cooker".into(), gbew),
            ("physics_determinism".into(), physics),
            (
                "vr_90hz_governor".into(),
                Value::Object(BTreeMap::from([
                    (
                        "target_ms".into(),
                        Value::Number(round_f64(self.vr_90hz_target_ms, 3)),
                    ),
                    (
                        "step_down_frames".into(),
                        Value::Number(self.vr_90hz_step_down_frames as f64),
                    ),
                    (
                        "recovery_frames".into(),
                        Value::Number(self.vr_90hz_recovery_frames as f64),
                    ),
                    (
                        "verified".into(),
                        Value::Bool(self.vr_90hz_governor_verified),
                    ),
                ])),
            ),
        ]))
    }
}

struct SpatialTaskSpec {
    grounded_gbs: String,
    ungrounded_gbs: String,
    viewport: (u32, u32),
}

fn build_paper1_tasks() -> Vec<SpatialTaskSpec> {
    let mut tasks = Vec::with_capacity(30);

    // 1..=6: Ground snapping across varied world coordinates, clearances, and yaw angles.
    let ground_coords: [([f32; 2], f32, f32); 6] = [
        ([0.0, 0.0], 0.0, 0.0),
        ([-2.4, 1.2], 0.01, 30.0),
        ([2.8, -1.6], 0.02, 45.0),
        ([-3.2, -2.0], 0.0, 90.0),
        ([1.5, 3.0], 0.015, 135.0),
        ([0.0, -3.5], 0.02, 180.0),
    ];
    for (i, ([x, z], clearance, yaw)) in ground_coords.into_iter().enumerate() {
        let grounded_gbs = format!(
            "tx \"Ground Snap {}\"\nspawn ped brick:stage_pedestal name=\"Pedestal_{}\"\nplace $ped snap onto ground at [{x:.2}, {z:.2}] clearance={clearance:.3} yaw={yaw:.1}",
            i + 1,
            i + 1
        );
        let float_y = 0.65 + (i as f32) * 0.12;
        let ungrounded_gbs = format!(
            "tx \"Ungrounded Float {}\"\nspawn ped brick:stage_pedestal name=\"Pedestal_{}\"\nplace $ped at [{x:.2}, {float_y:.2}, {z:.2}]",
            i + 1,
            i + 1
        );
        tasks.push(SpatialTaskSpec {
            grounded_gbs,
            ungrounded_gbs,
            viewport: (320, 180),
        });
    }

    // 7..=12: Hierarchical `socket top_center` stacking of pillar lamps onto stage pedestals.
    let socket_coords: [[f32; 2]; 6] = [
        [0.0, 0.0],
        [2.2, 0.8],
        [-2.2, 0.8],
        [1.8, -2.2],
        [-1.8, -2.2],
        [0.0, 2.6],
    ];
    for (i, [x, z]) in socket_coords.into_iter().enumerate() {
        let idx = i + 7;
        let grounded_gbs = format!(
            "tx \"Socket Stack {idx}\"\nspawn base brick:stage_pedestal name=\"Base_{idx}\"\nplace $base snap onto ground at [{x:.2}, {z:.2}]\nspawn lamp brick:pillar_lamp name=\"Lamp_{idx}\"\nplace $lamp socket top_center on $base"
        );
        let ungrounded_gbs = format!(
            "tx \"Ungrounded Overlap {idx}\"\nspawn base brick:stage_pedestal name=\"Base_{idx}\"\nplace $base snap onto ground at [{x:.2}, {z:.2}]\nspawn lamp brick:pillar_lamp name=\"Lamp_{idx}\"\nplace $lamp at [{x:.2}, 0.25, {z:.2}]"
        );
        tasks.push(SpatialTaskSpec {
            grounded_gbs,
            ungrounded_gbs,
            viewport: (320, 180),
        });
    }

    // 13..=18: Directional `beside` adjacency (`left`, `right`, `front`, `back`, `above`).
    let beside_cases: [(&str, f32, [f32; 2]); 6] = [
        ("left", 0.20, [0.0, 0.0]),
        ("right", 0.25, [1.0, -0.5]),
        ("front", 0.30, [-1.0, 0.5]),
        ("back", 0.35, [0.5, 1.2]),
        ("left", 0.45, [-1.5, -1.2]),
        ("above", 0.02, [0.0, -1.0]),
    ];
    for (i, (dir, gap, [x, z])) in beside_cases.into_iter().enumerate() {
        let idx = i + 13;
        let grounded_gbs = format!(
            "tx \"Beside {dir} {idx}\"\nspawn anchor brick:stage_pedestal name=\"Anchor_{idx}\"\nplace $anchor snap onto ground at [{x:.2}, {z:.2}]\nspawn peer brick:pillar_lamp name=\"Peer_{idx}\"\nplace $peer beside $anchor {dir} gap={gap:.2}"
        );
        let ungrounded_gbs = format!(
            "tx \"Ungrounded Beside {idx}\"\nspawn anchor brick:stage_pedestal name=\"Anchor_{idx}\"\nplace $anchor snap onto ground at [{x:.2}, {z:.2}]\nspawn peer brick:pillar_lamp name=\"Peer_{idx}\"\nplace $peer at [{:.2}, 0.20, {:.2}]",
            x + 0.12,
            z + 0.08
        );
        tasks.push(SpatialTaskSpec {
            grounded_gbs,
            ungrounded_gbs,
            viewport: (320, 180),
        });
    }

    // 19..=24: Radial `ring` arrangements (`count = 3..=8`) with `snap=ground` and facing modes.
    let ring_cases: [(usize, f32, &str, [f32; 3]); 6] = [
        (3, 2.2, "in", [0.0, 0.0, 0.0]),
        (4, 2.8, "in", [0.5, 0.0, -0.5]),
        (5, 3.2, "out", [-0.5, 0.0, 0.5]),
        (6, 3.8, "in", [0.0, 0.0, 0.0]),
        (7, 4.2, "tangent", [0.0, 0.0, 1.0]),
        (8, 4.8, "in", [0.0, 0.0, -1.0]),
    ];
    for (i, (count, radius, face, [cx, cy, cz])) in ring_cases.into_iter().enumerate() {
        let idx = i + 19;
        let grounded_gbs = format!(
            "tx \"Ring Layout {idx}\"\nring {count} brick:pillar_lamp around [{cx:.2}, {cy:.2}, {cz:.2}] radius={radius:.2} face={face} snap=ground name=\"Colonnade_{idx}\""
        );
        let ungrounded_gbs = format!(
            "tx \"Ungrounded Ring {idx}\"\nring {count} brick:pillar_lamp around [{cx:.2}, 0.95, {cz:.2}] radius={radius:.2} face={face} name=\"Colonnade_{idx}\""
        );
        tasks.push(SpatialTaskSpec {
            grounded_gbs,
            ungrounded_gbs,
            viewport: (320, 180),
        });
    }

    // 25..=27: `look_at` & `rotate` multi-entity orientation constraints.
    let orient_cases: [([f32; 2], [f32; 2], f32); 3] = [
        ([0.0, 0.0], [2.4, 2.0], 25.0),
        ([-1.0, 0.5], [1.8, -2.2], 60.0),
        ([0.8, -1.2], [-2.5, 1.6], 120.0),
    ];
    for (i, ([fx, fz], [wx, wz], yaw)) in orient_cases.into_iter().enumerate() {
        let idx = i + 25;
        let grounded_gbs = format!(
            "tx \"LookAt Orientation {idx}\"\nspawn focal brick:stage_pedestal name=\"Focal_{idx}\"\nplace $focal snap onto ground at [{fx:.2}, {fz:.2}]\nrotate $focal yaw={yaw:.1}\nspawn watcher brick:pillar_lamp name=\"Watcher_{idx}\"\nplace $watcher snap onto ground at [{wx:.2}, {wz:.2}]\nlook_at $watcher $focal"
        );
        let ungrounded_gbs = format!(
            "tx \"Ungrounded LookAt {idx}\"\nspawn focal brick:stage_pedestal name=\"Focal_{idx}\"\nplace $focal at [{fx:.2}, 0.85, {fz:.2}]\nspawn watcher brick:pillar_lamp name=\"Watcher_{idx}\"\nplace $watcher at [{wx:.2}, 1.10, {wz:.2}]"
        );
        tasks.push(SpatialTaskSpec {
            grounded_gbs,
            ungrounded_gbs,
            viewport: (320, 180),
        });
    }

    // 28..=30: `cam frame` composition across landscape (`16:9`) and portrait (`9:16`) viewports.
    let cam_cases: [(f32, f32, f32, (u32, u32)); 3] = [
        (35.0, 24.0, 50.0, (320, 180)),
        (-40.0, 28.0, 46.0, (180, 320)),
        (15.0, 32.0, 55.0, (320, 180)),
    ];
    for (i, (az, el, fov, viewport)) in cam_cases.into_iter().enumerate() {
        let idx = i + 28;
        let grounded_gbs = format!(
            "tx \"Camera Frame {idx}\"\nspawn stage brick:stage_pedestal name=\"Stage_{idx}\"\nplace $stage snap onto ground at [0.0, 0.0]\nspawn left_lamp brick:pillar_lamp name=\"LeftLamp_{idx}\"\nplace $left_lamp beside $stage left gap=0.40\nspawn right_lamp brick:pillar_lamp name=\"RightLamp_{idx}\"\nplace $right_lamp beside $stage right gap=0.40\nspawn cam empty name=\"ShotCam_{idx}\"\ncam frame $cam all azimuth={az:.1} elevation={el:.1} padding=1.40 fov={fov:.1}"
        );
        let ungrounded_gbs = format!(
            "tx \"Ungrounded Cam {idx}\"\nspawn stage brick:stage_pedestal name=\"Stage_{idx}\"\nplace $stage at [0.0, 0.90, 0.0]\nspawn left_lamp brick:pillar_lamp name=\"LeftLamp_{idx}\"\nplace $left_lamp at [0.10, 0.90, 0.05]"
        );
        tasks.push(SpatialTaskSpec {
            grounded_gbs,
            ungrounded_gbs,
            viewport,
        });
    }

    tasks
}

/// Executes Paper 1's live `N = 30` `.gbs` relational spatial + `Document` invertibility +
/// `<100ms` `scene_verify` empirical suite inside an isolated self-cleaning workspace.
pub fn run_paper1_spatial_suite() -> Result<Paper1SpatialSuite, String> {
    let temp = TempBenchDir(
        std::env::temp_dir().join(format!("ganbate-p1-bench-{}", Ids::from_os().entity())),
    );
    fs::create_dir_all(&temp.0).map_err(|e| format!("paper1 temp dir: {e}"))?;

    let mut ids = Ids::seeded(20_261_105);
    let mut project = Project::create(&temp.0, "Paper1 Spatial Bench", &mut ids)?;
    let registry = engine_registry();
    let no_prefabs = |_: &str| -> Option<Value> { None };

    // Author and crystallize two reusable Smart Bricks (`stage_pedestal` and `pillar_lamp`) once.
    let seed_scene_id = ids.asset();
    let seed_blank = scene::new_document(scene::Kind::Scene, &seed_scene_id, "BrickSeed");
    let seed_script = [
        r#"tx "Seed Reusable Bricks""#,
        r#"spawn ped empty name="SeedPedestal""#,
        r#"attach @this collider shape="box" half_extents=[0.6, 0.35, 0.6]"#,
        r#"place @this snap onto ground at [0.0, 0.0]"#,
        r#"spawn post empty name="SeedLampPost""#,
        r#"attach @this collider shape="box" half_extents=[0.22, 0.65, 0.22]"#,
        r#"place @this snap onto ground at [3.0, 0.0]"#,
        r#"spawn emitter empty name="SeedLampGlow" parent=$post"#,
        r#"attach @this light intensity=4.5 range=8.0"#,
        r#"place @this at [3.0, 1.35, 0.0]"#,
    ]
    .join("\n");
    let seeded = execute_gbs(
        &project,
        &registry,
        &seed_blank,
        &seed_script,
        None,
        &no_prefabs,
        &mut ids,
    )?;
    crystallize_brick(
        &project,
        &registry,
        &seeded.root,
        "stage_pedestal",
        &["SeedPedestal".into()],
        &[],
    )?;
    crystallize_brick(
        &project,
        &registry,
        &seeded.root,
        "pillar_lamp",
        &["SeedLampPost".into()],
        &[],
    )?;

    // Baseline scene document for all 30 tasks (contains an ambient directional/point key light).
    let stage_id = ids.asset();
    let stage_blank = scene::new_document(scene::Kind::Scene, &stage_id, "AuthoringBenchStage");
    let baseline_init = execute_gbs(
        &project,
        &registry,
        &stage_blank,
        "tx \"Init Stage\"\nspawn key_light empty name=\"KeyLight\"\nattach @this light intensity=3.0 range=18.0\nplace @this at [2.0, 5.0, 3.0]",
        None,
        &no_prefabs,
        &mut ids,
    )?;
    let baseline_root = baseline_init.root;
    project.write_document(
        "content/bench_stage.scene.json",
        Kind::Scene,
        &baseline_root,
    )?;
    let baseline_canonical = json::to_canonical(&baseline_root)?;
    let baseline_sha256 = sha256_hex(baseline_canonical.as_bytes());

    let tasks = build_paper1_tasks();
    let total_tasks = tasks.len();
    let mut tasks_passed = 0usize;
    let mut op_ctrl_z_sha256_recovered = 0usize;
    let mut saw_op_set = false;
    let mut saw_op_remove = false;
    let mut gbs_snapped_lint_free_tasks = 0usize;
    let mut ungrounded_baseline_linted_tasks = 0usize;
    let mut ungrounded_baseline_floating_lints = 0usize;
    let mut ungrounded_baseline_penetration_lints = 0usize;
    let mut screen_projection_verified_tasks = 0usize;
    let mut budget_within_limits_tasks = 0usize;

    let mut gbs_samples_ns = Vec::with_capacity(total_tasks);
    let mut verify_samples_ns = Vec::with_capacity(total_tasks);

    for (task_idx, spec) in tasks.iter().enumerate() {
        let mut task_ids = Ids::seeded(90_000 + (task_idx as u64) * 31);

        // 1. Compile & apply grounded `.gbs` script.
        let t_gbs = Instant::now();
        let outcome = execute_gbs(
            &project,
            &registry,
            black_box(&baseline_root),
            black_box(&spec.grounded_gbs),
            None,
            &no_prefabs,
            &mut task_ids,
        )?;
        gbs_samples_ns.push(t_gbs.elapsed().as_nanos());

        // 2. Document Op::Set / Op::Remove forward + inverse (`Ctrl+Z`) SHA-256 verification.
        let mut doc = Document::new(baseline_root.clone())?;
        let undo_ops = doc.apply_all(&outcome.ops)?;
        for op in outcome.ops.iter().chain(undo_ops.iter()) {
            match op {
                Op::Set { .. } => saw_op_set = true,
                Op::Remove { .. } => saw_op_remove = true,
            }
        }
        let mutated_canonical = json::to_canonical(doc.root())?;
        let mutated_differs = mutated_canonical != baseline_canonical;
        doc.apply_all(&undo_ops)?;
        let restored_canonical = json::to_canonical(doc.root())?;
        let restored_sha256 = sha256_hex(restored_canonical.as_bytes());
        let ctrl_z_ok = mutated_differs && restored_sha256 == baseline_sha256;
        if ctrl_z_ok {
            op_ctrl_z_sha256_recovered += 1;
        }

        // 3. `<100ms` `scene_verify` oracle on grounded `.gbs` output.
        let verify_opts = VerifyOptions {
            camera_ref: None,
            width: spec.viewport.0,
            height: spec.viewport.1,
            max_draws: 256,
            max_triangles: 250_000,
            max_vram_mb: 128,
            render_contact_sheet: false,
        };
        let t_ver = Instant::now();
        let rep = verify_scene(
            &project,
            &registry,
            black_box(&outcome.root),
            None,
            &no_prefabs,
            &verify_opts,
        )?;
        verify_samples_ns.push(t_ver.elapsed().as_nanos());

        let lint_free = rep.ok && rep.schema_issues.is_empty() && rep.spatial_lints.is_empty();
        if lint_free {
            gbs_snapped_lint_free_tasks += 1;
        }
        let has_onscreen_box = rep.screen_boxes.iter().any(|sb| sb.on_screen);
        if has_onscreen_box {
            screen_projection_verified_tasks += 1;
        }
        let draws = rep.budget.get("drawCalls").copied().unwrap_or(u64::MAX);
        let tris = rep.budget.get("triangles").copied().unwrap_or(u64::MAX);
        let within_budget = draws <= verify_opts.max_draws && tris <= verify_opts.max_triangles;
        if within_budget {
            budget_within_limits_tasks += 1;
        }

        if ctrl_z_ok && lint_free && has_onscreen_box && within_budget {
            tasks_passed += 1;
        }

        // 4. Matched ungrounded coordinate perturbation baseline (`floating-prop` / `static-penetration`).
        let mut base_ids = Ids::seeded(190_000 + (task_idx as u64) * 31);
        let ungrounded_out = execute_gbs(
            &project,
            &registry,
            &baseline_root,
            &spec.ungrounded_gbs,
            None,
            &no_prefabs,
            &mut base_ids,
        )?;
        let ungrounded_rep = verify_scene(
            &project,
            &registry,
            &ungrounded_out.root,
            None,
            &no_prefabs,
            &verify_opts,
        )?;
        let floating_cnt = ungrounded_rep
            .spatial_lints
            .iter()
            .filter(|l| l.code == "floating-prop")
            .count();
        let pen_cnt = ungrounded_rep
            .spatial_lints
            .iter()
            .filter(|l| l.code == "static-penetration")
            .count();
        ungrounded_baseline_floating_lints += floating_cnt;
        ungrounded_baseline_penetration_lints += pen_cnt;
        if floating_cnt + pen_cnt > 0 {
            ungrounded_baseline_linted_tasks += 1;
        }
    }

    let gbs_compile_apply_us = summarize_samples_ns(
        "paper1_gbs_compile_apply",
        "Paper 1 .gbs Spatial Compile + Apply (N=30 tasks)",
        gbs_samples_ns,
    );
    let scene_verify_us = summarize_samples_ns(
        "paper1_scene_verify_oracle",
        "Paper 1 scene_verify Spatial/Budget Oracle (N=30 tasks)",
        verify_samples_ns,
    );
    let verify_under_100ms_sla = scene_verify_us.max_us < 100_000.0;

    let denom = (total_tasks.max(1)) as f64;
    Ok(Paper1SpatialSuite {
        total_tasks,
        tasks_passed,
        op_ctrl_z_sha256_recovered,
        op_ctrl_z_sha256_recovery_rate: (op_ctrl_z_sha256_recovered as f64) / denom,
        op_set_and_remove_exercised: saw_op_set && saw_op_remove,
        gbs_snapped_lint_free_tasks,
        gbs_snapped_spatial_lint_free_rate: (gbs_snapped_lint_free_tasks as f64) / denom,
        ungrounded_baseline_linted_tasks,
        ungrounded_baseline_lint_free_rate: ((total_tasks
            .saturating_sub(ungrounded_baseline_linted_tasks))
            as f64)
            / denom,
        ungrounded_baseline_floating_lints,
        ungrounded_baseline_penetration_lints,
        screen_projection_verified_tasks,
        budget_within_limits_tasks,
        verify_under_100ms_sla,
        gbs_compile_apply_us,
        scene_verify_us,
    })
}

fn is_dependency_section(header: &str) -> bool {
    let h = header.trim();
    if !(h.starts_with('[') && h.ends_with(']')) || h.starts_with("[[") {
        return false;
    }
    let inner = h[1..h.len() - 1].trim();
    matches!(
        inner,
        "dependencies" | "dev-dependencies" | "build-dependencies" | "workspace.dependencies"
    ) || inner.ends_with(".dependencies")
        || inner.ends_with(".dev-dependencies")
        || inner.ends_with(".build-dependencies")
}

fn strip_toml_comment(line: &str) -> &str {
    let mut in_quotes = false;
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if in_quotes => {
                i += 2;
                continue;
            }
            b'"' => in_quotes = !in_quotes,
            b'#' if !in_quotes => return &line[..i],
            _ => {}
        }
        i += 1;
    }
    line
}

fn scan_cargo_manifest_deps(manifest_text: &str) -> (usize, usize) {
    let mut in_deps = false;
    let mut brace_depth = 0i32;
    let mut current_entry = String::new();
    let mut internal_path = 0usize;
    let mut external_crates_io = 0usize;

    let classify_entry = |rhs: &str, internal: &mut usize, external: &mut usize| {
        let trimmed = rhs.trim();
        if trimmed.is_empty() {
            return;
        }
        if trimmed.contains("path") && trimmed.contains('=') {
            *internal += 1;
        } else if trimmed.contains("workspace")
            && trimmed.contains("true")
            && !trimmed.contains("version")
        {
            *internal += 1;
        } else {
            *external += 1;
        }
    };

    for raw_line in manifest_text.lines() {
        let line = strip_toml_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        if brace_depth == 0 && line.starts_with('[') {
            in_deps = is_dependency_section(line);
            continue;
        }
        if !in_deps {
            continue;
        }
        if brace_depth > 0 {
            for ch in line.chars() {
                if ch == '{' {
                    brace_depth += 1;
                } else if ch == '}' {
                    brace_depth -= 1;
                }
            }
            current_entry.push(' ');
            current_entry.push_str(line);
            if brace_depth <= 0 {
                brace_depth = 0;
                classify_entry(&current_entry, &mut internal_path, &mut external_crates_io);
                current_entry.clear();
            }
            continue;
        }

        if let Some((_, rhs)) = line.split_once('=') {
            let open_cnt = rhs.chars().filter(|&c| c == '{').count() as i32;
            let close_cnt = rhs.chars().filter(|&c| c == '}').count() as i32;
            if open_cnt > close_cnt {
                brace_depth = open_cnt - close_cnt;
                current_entry.clear();
                current_entry.push_str(rhs);
            } else {
                classify_entry(rhs, &mut internal_path, &mut external_crates_io);
            }
        }
    }

    (internal_path, external_crates_io)
}

fn resolve_engine_workspace_root(project_root: &Path) -> PathBuf {
    let compile_time_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(Path::to_path_buf);
    if let Some(root) = compile_time_root
        && root.join("Cargo.toml").is_file()
        && root.join("crates").is_dir()
    {
        return root;
    }
    if project_root.join("Cargo.toml").is_file() && project_root.join("crates").is_dir() {
        return project_root.to_path_buf();
    }
    let sibling = project_root.join("../ganbate-engine");
    if sibling.join("Cargo.toml").is_file() && sibling.join("crates").is_dir() {
        return sibling;
    }
    project_root.to_path_buf()
}

/// Extracts a top-level JSON object field (such as `"summary": { ... }`) from a bounded byte slice
/// without parsing multi-megabyte sibling arrays.
fn extract_named_json_object_slice<'a>(buf: &'a [u8], field_key: &str) -> Option<&'a [u8]> {
    let needle = format!("\"{field_key}\"");
    let needle_bytes = needle.as_bytes();
    let mut search_from = 0usize;

    while search_from + needle_bytes.len() < buf.len() {
        let rel = buf[search_from..]
            .windows(needle_bytes.len())
            .position(|w| w == needle_bytes)?;
        let after_key = search_from + rel + needle_bytes.len();
        let mut idx = after_key;
        while idx < buf.len() && buf[idx].is_ascii_whitespace() {
            idx += 1;
        }
        if idx >= buf.len() || buf[idx] != b':' {
            search_from = after_key;
            continue;
        }
        idx += 1;
        while idx < buf.len() && buf[idx].is_ascii_whitespace() {
            idx += 1;
        }
        if idx >= buf.len() || buf[idx] != b'{' {
            search_from = idx;
            continue;
        }

        let obj_start = idx;
        let mut depth = 0i32;
        let mut in_quotes = false;
        let mut i = obj_start;
        while i < buf.len() {
            let b = buf[i];
            if in_quotes {
                if b == b'\\' {
                    i += 2;
                    continue;
                }
                if b == b'"' {
                    in_quotes = false;
                }
            } else {
                match b {
                    b'"' => in_quotes = true,
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(&buf[obj_start..=i]);
                        }
                    }
                    _ => {}
                }
            }
            i += 1;
        }
        return None;
    }
    None
}

fn find_existing_file(project_root: &Path, candidates: &[&str]) -> Option<PathBuf> {
    for rel in candidates {
        let p = project_root.join(rel);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// Executes Paper 2's deterministic Cargo dependency purity audit and live Ecosystem X-Ray /
/// AI Context Pyramid provenance scan (strictly read-only).
pub fn run_paper2_ecosystem_governance(
    project_root: &Path,
) -> Result<Paper2EcosystemGovernance, String> {
    let engine_root = resolve_engine_workspace_root(project_root);

    let mut manifest_dirs = Vec::new();
    if engine_root.join("Cargo.toml").is_file() {
        manifest_dirs.push(engine_root.clone());
    }
    let crates_dir = engine_root.join("crates");
    if let Ok(entries) = fs::read_dir(&crates_dir) {
        let mut subdirs: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.join("Cargo.toml").is_file())
            .collect();
        subdirs.sort();
        manifest_dirs.extend(subdirs);
    }

    let workspace_crates = manifest_dirs.len();
    let mut internal_path_dependencies = 0usize;
    let mut external_crates_io_dependencies = 0usize;
    let mut forbid_unsafe_crates = 0usize;

    for crate_dir in &manifest_dirs {
        let cargo_toml = fs::read_to_string(crate_dir.join("Cargo.toml"))
            .map_err(|e| format!("{}/Cargo.toml: {e}", crate_dir.display()))?;
        let (internal_cnt, ext_cnt) = scan_cargo_manifest_deps(&cargo_toml);
        internal_path_dependencies += internal_cnt;
        external_crates_io_dependencies += ext_cnt;

        let lib_rs = crate_dir.join("src/lib.rs");
        let main_rs = crate_dir.join("src/main.rs");
        let root_src = if lib_rs.is_file() {
            fs::read_to_string(&lib_rs).unwrap_or_default()
        } else if main_rs.is_file() {
            fs::read_to_string(&main_rs).unwrap_or_default()
        } else {
            String::new()
        };
        if root_src.contains("#![forbid(unsafe_code)]") {
            forbid_unsafe_crates += 1;
        }
    }

    let zero_external_deps_verified =
        workspace_crates >= 14 && external_crates_io_dependencies == 0;

    // Live Ecosystem X-Ray provenance (`code-surface-catalog.json`, read-only bounded header scan).
    let xray_candidates = [
        "main/apps/web/public/ganbate-studio/code-surface-catalog.json",
        "apps/web/public/ganbate-studio/code-surface-catalog.json",
        "public/ganbate-studio/code-surface-catalog.json",
        "code-surface-catalog.json",
    ];
    let mut xray_available = false;
    let mut indexed_surfaces = 0u64;
    let mut code_files = 0u64;
    let mut dependency_edges = 0u64;
    let mut avg_implementation_pct = 0.0f64;
    let mut avg_integration_pct = 0.0f64;
    let mut reclaimable_loc = 0u64;

    if let Some(xray_path) = find_existing_file(project_root, &xray_candidates)
        && let Ok(file) = fs::File::open(&xray_path)
    {
        let mut head = Vec::with_capacity(MAX_XRAY_HEADER_BYTES);
        if file
            .take(MAX_XRAY_HEADER_BYTES as u64)
            .read_to_end(&mut head)
            .is_ok()
        {
            let summary_val = if let Some(slice) = extract_named_json_object_slice(&head, "summary")
            {
                json::parse(slice).ok()
            } else if fs::metadata(&xray_path)
                .map(|m| m.len() <= MAX_XRAY_FALLBACK_BYTES)
                .unwrap_or(false)
            {
                fs::read(&xray_path)
                    .ok()
                    .and_then(|full| json::parse(&full).ok())
                    .and_then(|v| v.get("summary").cloned())
            } else {
                None
            };

            if let Some(sum) = summary_val {
                xray_available = true;
                indexed_surfaces = sum
                    .get("totalItems")
                    .and_then(|v| v.unsigned().ok())
                    .unwrap_or(0);
                code_files = sum
                    .get("codeFileCount")
                    .and_then(|v| v.unsigned().ok())
                    .unwrap_or(0);
                dependency_edges = sum
                    .get("totalDependencyEdges")
                    .and_then(|v| v.unsigned().ok())
                    .unwrap_or(0);
                avg_implementation_pct = sum
                    .get("avgImplementationPct")
                    .and_then(|v| v.float().ok())
                    .map(f64::from)
                    .unwrap_or(0.0);
                avg_integration_pct = sum
                    .get("avgIntegrationPct")
                    .and_then(|v| v.float().ok())
                    .map(f64::from)
                    .unwrap_or(0.0);
                reclaimable_loc = sum
                    .get("reclaimableLoc")
                    .and_then(|v| v.unsigned().ok())
                    .unwrap_or(0);
            }
        }
    }

    // Live AI Context Pyramid provenance (`ai-context-pyramid.json`, read-only).
    let pyramid_candidates = [
        "main/apps/web/public/ganbate-studio/ai-context-pyramid.json",
        "apps/web/public/ganbate-studio/ai-context-pyramid.json",
        "public/ganbate-studio/ai-context-pyramid.json",
        "ai-context-pyramid.json",
    ];
    let mut pyramid_available = false;
    let mut health_score = 0.0f64;
    let mut cluster_count = 0u64;
    let mut l0_tokens = 0u64;
    let mut max_l1_tokens = 0u64;
    let mut pyramid_within_budget = true;

    if let Some(pyr_path) = find_existing_file(project_root, &pyramid_candidates)
        && let Ok(bytes) = fs::read(&pyr_path)
        && let Ok(pyr) = json::parse(&bytes)
    {
        pyramid_available = true;
        health_score = pyr
            .get("optimizationMonitor")
            .and_then(|m| m.get("workspaceHealthSummary"))
            .and_then(|w| w.get("healthScore"))
            .and_then(|v| v.float().ok())
            .map(f64::from)
            .unwrap_or(0.0);
        cluster_count = match pyr.get("l1") {
            Some(Value::Object(m)) => m.len() as u64,
            Some(Value::Array(a)) => a.len() as u64,
            _ => pyr
                .get("clusters")
                .and_then(|v| v.array().ok())
                .map(|a| a.len() as u64)
                .unwrap_or(0),
        };
        if let Some(budgets) = pyr.get("budgets") {
            l0_tokens = budgets
                .get("actualL0Tokens")
                .and_then(|v| v.unsigned().ok())
                .unwrap_or(0);
            max_l1_tokens = budgets
                .get("maxActualL1Tokens")
                .and_then(|v| v.unsigned().ok())
                .unwrap_or(0);
            pyramid_within_budget = budgets
                .get("withinBudget")
                .and_then(|v| match v {
                    Value::Bool(b) => Some(*b),
                    _ => None,
                })
                .unwrap_or(true);
        }
    }

    Ok(Paper2EcosystemGovernance {
        workspace_crates,
        internal_path_dependencies,
        external_crates_io_dependencies,
        forbid_unsafe_crates,
        zero_external_deps_verified,
        xray_available,
        indexed_surfaces,
        code_files,
        dependency_edges,
        avg_implementation_pct,
        avg_integration_pct,
        reclaimable_loc,
        pyramid_available,
        health_score,
        cluster_count,
        l0_tokens,
        max_l1_tokens,
        pyramid_within_budget,
    })
}

const BENCH_STYLIZED_LIT_WGSL: &str = r#"
struct FrameUniforms {
    view_proj: mat4x4f,
    camera_pos: vec4f,
    sun_dir: vec4f,
    sun_color: vec4f,
};

struct DrawPush {
    model: mat4x4f,
    tint: vec4f,
};

@group(0) @binding(0) var<uniform> frame: FrameUniforms;
@group(0) @binding(1) var albedo_tex: texture_2d<f32>;
@group(0) @binding(2) var linear_samp: sampler;
var<push_constant> draw: DrawPush;

struct VsIn {
    @location(0) pos: vec3f,
    @location(1) normal: vec3f,
    @location(2) uv: vec2f,
};

struct VsOut {
    @builtin(position) clip_pos: vec4f,
    @location(0) world_pos: vec3f,
    @location(1) world_nrm: vec3f,
    @location(2) uv: vec2f,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    let world = draw.model * vec4f(in.pos, 1.0);
    let nrm = normalize((draw.model * vec4f(in.normal, 0.0)).xyz);
    var out: VsOut;
    out.clip_pos = frame.view_proj * world;
    out.world_pos = world.xyz;
    out.world_nrm = nrm;
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4f {
    let n = normalize(in.world_nrm);
    let l = normalize(frame.sun_dir.xyz);
    let v = normalize(frame.camera_pos.xyz - in.world_pos);
    let h = normalize(l + v);
    let tex = textureSample(albedo_tex, linear_samp, in.uv);
    let base = tex.rgb * draw.tint.rgb;
    let wrap_ndotl = clamp(dot(n, l) * 0.5 + 0.5, 0.0, 1.0);
    let toon_band = smoothstep(0.35, 0.68, wrap_ndotl);
    let spec = pow(max(dot(n, h), 0.0), 32.0) * 0.25;
    let rim = pow(1.0 - max(dot(n, v), 0.0), 3.0) * 0.18;
    let lit = base * (0.22 + 0.78 * toon_band) * frame.sun_color.rgb + vec3f(spec + rim);
    let mapped = lit / (lit + vec3f(1.0));
    return vec4f(mapped, tex.a * draw.tint.a);
}
"#;

fn median_compile_us<T, F>(runs: usize, mut f: F) -> Result<(T, f64), String>
where
    F: FnMut() -> Result<T, String>,
{
    let count = runs.max(1);
    let mut samples = Vec::with_capacity(count);
    let mut last_out = None;
    for _ in 0..count {
        let t0 = Instant::now();
        let out = f()?;
        samples.push(t0.elapsed().as_nanos());
        last_out = Some(out);
    }
    samples.sort_unstable();
    let mid_us = (samples[samples.len() / 2] as f64) / 1_000.0;
    Ok((last_out.ok_or("empty compile run")?, mid_us))
}

fn measure_four_shader_backends() -> Result<Vec<ShaderTargetMetrics>, String> {
    let src = BENCH_STYLIZED_LIT_WGSL;

    // 1. Vulkan 1.0 SPIR-V 1.0
    let ((vs_spv, fs_spv), spv_us) = median_compile_us(5, || {
        let vs = compile(black_box(src), Stage::Vertex, "vs_main").map_err(|e| e.to_string())?;
        let fs = compile(black_box(src), Stage::Fragment, "fs_main").map_err(|e| e.to_string())?;
        Ok((vs, fs))
    })?;
    let vs_spv_bytes = vs_spv.to_bytes();
    let fs_spv_bytes = fs_spv.to_bytes();
    let spv_ok = vs_spv.words.first() == Some(&SPIRV_MAGIC)
        && fs_spv.words.first() == Some(&SPIRV_MAGIC)
        && vs_spv.push_constant_size == 80
        && fs_spv.bindings.len() == 3;
    let mut spv_combined = Vec::with_capacity(vs_spv_bytes.len() + fs_spv_bytes.len());
    spv_combined.extend_from_slice(&vs_spv_bytes);
    spv_combined.extend_from_slice(&fs_spv_bytes);

    // 2. Apple Metal Shading Language (MSL 2.1+)
    let ((vs_msl, fs_msl), msl_us) = median_compile_us(5, || {
        let vs = compile_msl(black_box(src), Stage::Vertex, "vs_main")
            .map_err(|e| e.to_string())?
            .msl
            .ok_or_else(|| String::from("missing vertex MSL output"))?;
        let fs = compile_msl(black_box(src), Stage::Fragment, "fs_main")
            .map_err(|e| e.to_string())?
            .msl
            .ok_or_else(|| String::from("missing fragment MSL output"))?;
        Ok((vs, fs))
    })?;
    let msl_ok = vs_msl.source.contains("#include <metal_stdlib>")
        && fs_msl.source.contains("#include <metal_stdlib>")
        && vs_msl.source.contains("vertex ")
        && fs_msl.source.contains("fragment ");
    let mut msl_combined = Vec::with_capacity(vs_msl.source.len() + fs_msl.source.len());
    msl_combined.extend_from_slice(vs_msl.source.as_bytes());
    msl_combined.extend_from_slice(fs_msl.source.as_bytes());

    // 3. WebGPU WGSL
    let ((vs_wgsl, fs_wgsl), wgsl_us) = median_compile_us(5, || {
        let vs = compile_wgsl(black_box(src), Stage::Vertex, "vs_main")
            .map_err(|e| e.to_string())?
            .wgsl
            .ok_or_else(|| String::from("missing vertex WGSL output"))?;
        let fs = compile_wgsl(black_box(src), Stage::Fragment, "fs_main")
            .map_err(|e| e.to_string())?
            .wgsl
            .ok_or_else(|| String::from("missing fragment WGSL output"))?;
        Ok((vs, fs))
    })?;
    let wgsl_ok = vs_wgsl.source.contains("@vertex")
        && fs_wgsl.source.contains("@fragment")
        && vs_wgsl.push_constants.is_some();
    let mut wgsl_combined = Vec::with_capacity(vs_wgsl.source.len() + fs_wgsl.source.len());
    wgsl_combined.extend_from_slice(vs_wgsl.source.as_bytes());
    wgsl_combined.extend_from_slice(fs_wgsl.source.as_bytes());

    // 4. WebGL 2 GLSL ES 3.00
    let ((vs_glsl, fs_glsl), glsl_us) = median_compile_us(5, || {
        let vs = compile_glsl(black_box(src), Stage::Vertex, "vs_main")
            .map_err(|e| e.to_string())?
            .glsl
            .ok_or_else(|| String::from("missing vertex GLSL output"))?;
        let fs = compile_glsl(black_box(src), Stage::Fragment, "fs_main")
            .map_err(|e| e.to_string())?
            .glsl
            .ok_or_else(|| String::from("missing fragment GLSL output"))?;
        Ok((vs, fs))
    })?;
    let glsl_ok = vs_glsl.source.starts_with("#version 300 es")
        && fs_glsl.source.starts_with("#version 300 es")
        && !fs_glsl.units.is_empty();
    let mut glsl_combined = Vec::with_capacity(vs_glsl.source.len() + fs_glsl.source.len());
    glsl_combined.extend_from_slice(vs_glsl.source.as_bytes());
    glsl_combined.extend_from_slice(fs_glsl.source.as_bytes());

    Ok(vec![
        ShaderTargetMetrics {
            id: "spirv_1_0".into(),
            label: "Vulkan 1.0 SPIR-V 1.0".into(),
            vertex_bytes: vs_spv_bytes.len(),
            fragment_bytes: fs_spv_bytes.len(),
            total_bytes: spv_combined.len(),
            compile_us: spv_us,
            sha256: sha256_hex(&spv_combined),
            verified: spv_ok,
        },
        ShaderTargetMetrics {
            id: "msl_2_1".into(),
            label: "Metal Shading Language (MSL)".into(),
            vertex_bytes: vs_msl.source.len(),
            fragment_bytes: fs_msl.source.len(),
            total_bytes: msl_combined.len(),
            compile_us: msl_us,
            sha256: sha256_hex(&msl_combined),
            verified: msl_ok,
        },
        ShaderTargetMetrics {
            id: "webgpu_wgsl".into(),
            label: "WebGPU WGSL".into(),
            vertex_bytes: vs_wgsl.source.len(),
            fragment_bytes: fs_wgsl.source.len(),
            total_bytes: wgsl_combined.len(),
            compile_us: wgsl_us,
            sha256: sha256_hex(&wgsl_combined),
            verified: wgsl_ok,
        },
        ShaderTargetMetrics {
            id: "webgl2_glsl_es_300".into(),
            label: "WebGL 2 GLSL ES 3.00".into(),
            vertex_bytes: vs_glsl.source.len(),
            fragment_bytes: fs_glsl.source.len(),
            total_bytes: glsl_combined.len(),
            compile_us: glsl_us,
            sha256: sha256_hex(&glsl_combined),
            verified: glsl_ok,
        },
    ])
}

fn build_procedural_torus_mesh(major_seg: usize, minor_seg: usize) -> Mesh {
    let major_r = 1.25_f32;
    let minor_r = 0.42_f32;
    let cols = minor_seg + 1;
    let vert_count = (major_seg + 1) * cols;
    let mut positions = Vec::with_capacity(vert_count);
    let mut normals = Vec::with_capacity(vert_count);
    let mut uvs = Vec::with_capacity(vert_count);

    for i in 0..=major_seg {
        let u = (i as f32) / (major_seg as f32);
        let theta = u * std::f32::consts::TAU;
        let (sin_t, cos_t) = theta.sin_cos();
        for j in 0..=minor_seg {
            let v = (j as f32) / (minor_seg as f32);
            let phi = v * std::f32::consts::TAU;
            let (sin_p, cos_p) = phi.sin_cos();

            // Multi-harmonic architectural fluting & micro-stone relief (realistic authored mesh entropy).
            let flute = 0.035 * (12.0 * theta + 3.0 * phi).sin()
                + 0.014 * (29.0 * theta - 11.0 * phi).cos()
                + 0.006 * (53.0 * theta + 23.0 * phi).sin();
            let local_r = minor_r + flute;
            let ring_r = major_r + local_r * cos_p;
            positions.push(Vec3(ring_r * cos_t, local_r * sin_p, ring_r * sin_t));

            let nx = cos_p * cos_t + 0.18 * (17.0 * theta).sin();
            let ny = sin_p + 0.18 * (13.0 * phi).cos();
            let nz = cos_p * sin_t - 0.18 * (19.0 * theta + 7.0 * phi).sin();
            let n_len = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-6);
            normals.push(Vec3(nx / n_len, ny / n_len, nz / n_len));
            uvs.push([u, v]);
        }
    }

    let mut indices = Vec::with_capacity(major_seg * minor_seg * 6);
    for i in 0..major_seg {
        for j in 0..minor_seg {
            let a = (i * cols + j) as u32;
            let b = ((i + 1) * cols + j) as u32;
            let c = ((i + 1) * cols + (j + 1)) as u32;
            let d = (i * cols + (j + 1)) as u32;
            indices.extend_from_slice(&[a, b, c, a, c, d]);
        }
    }

    Mesh {
        positions,
        normals,
        uvs,
        indices,
        color: [0.86, 0.78, 0.64],
        material: Some(0),
    }
}

fn measure_gbew_cooking() -> Result<GbewCookMetrics, String> {
    // 64 x 32 smooth UV torus = 2,145 vertices, 4,096 triangles (>= 2,048 tris).
    let mesh = build_procedural_torus_mesh(64, 32);
    let vertex_count = mesh.positions.len();
    let triangle_count = mesh.indices.len() / 3;
    let raw_mesh_bytes = vertex_count * (12 + 12 + 8) + mesh.indices.len() * 4;

    let world = World {
        meshes: vec![mesh],
        instances: vec![Instance {
            mesh: 0,
            transform: Transform::IDENTITY,
        }],
        materials: vec![Material {
            name: "stylized_stone".into(),
            roughness: 0.65,
            metallic: 0.05,
            ..Material::default()
        }],
        textures: Vec::new(),
        lights: Vec::new(),
    };
    let table = EntityTable {
        entities: vec![EntityRecord {
            id: 0x0001_0002_0003_0004,
            name: "TorusArch".into(),
            parent: None,
            transform: Transform::IDENTITY,
            instances: 0..1,
            lights: 0..0,
        }],
        data: String::from("{}"),
    };

    let (cooked_bytes, cook_us) = median_compile_us(3, || {
        cooked::encode_scene(black_box(&world), black_box(&table), true)
    })?;
    let ((decoded_world, decoded_table), decode_us) =
        median_compile_us(3, || cooked::decode_scene(black_box(&cooked_bytes)))?;

    let cooked_gbew_bytes = cooked_bytes.len();
    let bytes_per_triangle = (cooked_gbew_bytes as f64) / (triangle_count.max(1) as f64);
    let compression_ratio = (raw_mesh_bytes as f64) / (cooked_gbew_bytes.max(1) as f64);

    // `.gbew` renumbers vertices in first-use triangle index order for delta coding, so compare
    // every triangle corner position `orig.positions[orig.indices[k]]` vs `dec.positions[dec.indices[k]]`.
    let mut max_err_m = 0.0_f32;
    let orig_mesh = &world.meshes[0];
    let dec_mesh = decoded_world
        .meshes
        .first()
        .ok_or("decoded .gbew missing mesh 0")?;
    if dec_mesh.indices.len() == orig_mesh.indices.len() {
        for (&oi, &di) in orig_mesh.indices.iter().zip(&dec_mesh.indices) {
            let a = orig_mesh.positions[oi as usize];
            let b = dec_mesh.positions[di as usize];
            let err = (a.0 - b.0)
                .abs()
                .max((a.1 - b.1).abs())
                .max((a.2 - b.2).abs());
            if err > max_err_m {
                max_err_m = err;
            }
        }
    } else {
        max_err_m = f32::INFINITY;
    }

    let max_position_error_mm = f64::from(max_err_m) * 1_000.0;
    let roundtrip_verified = cooked::version(&cooked_bytes) == Some(cooked::SCENE_VERSION)
        && dec_mesh.indices.len() == orig_mesh.indices.len()
        && decoded_table.entities.len() == 1
        && max_err_m.is_finite()
        && max_err_m <= 0.000_51;

    Ok(GbewCookMetrics {
        version: cooked::SCENE_VERSION,
        vertex_count,
        triangle_count,
        raw_mesh_bytes,
        cooked_gbew_bytes,
        bytes_per_triangle,
        compression_ratio,
        max_position_error_mm,
        cook_us,
        decode_us,
        roundtrip_verified,
    })
}

fn run_single_10k_physics_arena() -> Result<(u64, u64, usize, bool, f64), String> {
    let mut world = PhysicsWorld::new(Settings {
        substeps: 1,
        ..Settings::default()
    });

    // Static arena floor slab.
    let arena = world.create_body(&BodyDesc {
        kind: BodyKind::Static,
        position: PhysVec3::ZERO,
        ..BodyDesc::default()
    })?;
    let mut floor_col = ColliderDesc::new(PhysShape::Box {
        half_extents: PhysVec3::new(12.0, 0.5, 12.0),
    });
    floor_col.local.position = PhysVec3::new(0.0, -0.5, 0.0);
    floor_col.restitution = 0.86;
    floor_col.friction = 0.30;
    world.add_collider(arena, &floor_col)?;

    // 3 heterogeneous dynamic rigid bodies (sphere, box, capsule) interacting over the floor.
    let specs: [(PhysVec3, PhysVec3, PhysVec3, PhysShape); 3] = [
        (
            PhysVec3::new(-0.22, 1.4, 0.08),
            PhysVec3::new(0.45, -0.4, -0.25),
            PhysVec3::new(0.3, 1.2, 0.4),
            PhysShape::Sphere { radius: 0.30 },
        ),
        (
            PhysVec3::new(0.24, 2.1, -0.06),
            PhysVec3::new(-0.40, 0.0, 0.20),
            PhysVec3::new(0.7, -0.9, 0.2),
            PhysShape::Box {
                half_extents: PhysVec3::new(0.26, 0.26, 0.26),
            },
        ),
        (
            PhysVec3::new(0.02, 2.8, 0.04),
            PhysVec3::new(0.15, -0.6, -0.18),
            PhysVec3::new(-0.5, 0.8, 1.0),
            PhysShape::Capsule {
                half_height: 0.24,
                radius: 0.20,
            },
        ),
    ];

    let mut dyn_ids = Vec::with_capacity(specs.len());
    for (idx, (pos, lin_vel, ang_vel, shape)) in specs.into_iter().enumerate() {
        let body = world.create_body(&BodyDesc {
            kind: BodyKind::Dynamic,
            position: pos,
            rotation: PhysQuat::IDENTITY,
            linear_velocity: lin_vel,
            angular_velocity: ang_vel,
            linear_damping: 0.02,
            angular_damping: 0.03,
            can_sleep: false,
            user_data: (idx + 1) as u64,
            ..BodyDesc::default()
        })?;
        let mut col = ColliderDesc::new(shape);
        col.restitution = 0.85;
        col.friction = 0.25;
        world.add_collider(body, &col)?;
        dyn_ids.push(body);
    }

    let initial_hash = world.state_hash();
    let t0 = Instant::now();
    for step_idx in 0..PHYSICS_DETERMINISM_STEPS {
        if step_idx > 0 && step_idx % 500 == 0 {
            let phase = if (step_idx / 500) % 2 == 0 { 1.0 } else { -1.0 };
            world.set_linear_velocity(dyn_ids[0], PhysVec3::new(0.35 * phase, 2.8, -0.20 * phase));
            world.set_linear_velocity(dyn_ids[1], PhysVec3::new(-0.30 * phase, 2.4, 0.25 * phase));
        }
        world.step(black_box(PHYSICS_DT_SECONDS));
    }
    let elapsed_ms = t0.elapsed().as_secs_f64() * 1_000.0;

    let mut zero_nan = true;
    for &bid in &dyn_ids {
        if let Some(pose) = world.body_pose(bid) {
            if !pose.position.is_finite() || !pose.rotation.is_finite() {
                zero_nan = false;
            }
        } else {
            zero_nan = false;
        }
    }

    let final_hash = world.state_hash();
    Ok((
        initial_hash,
        final_hash,
        dyn_ids.len(),
        zero_nan,
        elapsed_ms,
    ))
}

fn measure_physics_determinism_10k() -> Result<PhysicsDeterminismMetrics, String> {
    let (init_a, hash_a, dyn_count, zero_nan_a, run_a_ms) = run_single_10k_physics_arena()?;
    let (init_b, hash_b, _, zero_nan_b, run_b_ms) = run_single_10k_physics_arena()?;

    let bit_exact_verified = init_a == init_b && hash_a == hash_b && hash_a != init_a;
    let zero_nan_verified = zero_nan_a && zero_nan_b;
    let mean_us_per_step =
        ((run_a_ms + run_b_ms) * 1_000.0) / ((2 * PHYSICS_DETERMINISM_STEPS) as f64);

    Ok(PhysicsDeterminismMetrics {
        steps: PHYSICS_DETERMINISM_STEPS,
        dt_seconds: f64::from(PHYSICS_DT_SECONDS),
        dynamic_bodies: dyn_count,
        hash_initial_hex: format!("0x{init_a:016x}"),
        hash_run_a_hex: format!("0x{hash_a:016x}"),
        hash_run_b_hex: format!("0x{hash_b:016x}"),
        bit_exact_verified,
        zero_nan_verified,
        mean_us_per_step,
        run_a_ms,
        run_b_ms,
    })
}

fn verify_vr_90hz_governor() -> (f64, usize, usize, bool) {
    let mut gov = FrameBudgetGovernor::new_90hz_vr();
    let target_ms = f64::from(gov.target_ms());
    let mut step_down_frames = 0usize;
    for frame in 1..=32 {
        gov.record_frame_ms(13.8);
        if gov.tier() == 0 && step_down_frames == 0 {
            step_down_frames = frame;
        }
    }
    let mut deadband_ok = gov.tier() == 0;
    for _ in 0..20 {
        gov.record_frame_ms(10.1);
        if gov.tier() != 0 {
            deadband_ok = false;
        }
    }
    let mut recovery_frames = 0usize;
    for frame in 1..=96 {
        gov.record_frame_ms(7.8);
        if gov.tier() == 3 && recovery_frames == 0 {
            recovery_frames = frame;
        }
    }
    let ok = step_down_frames > 0 && deadband_ok && recovery_frames > 0 && gov.tier() == 3;
    (target_ms, step_down_frames, recovery_frames, ok)
}

/// Executes Paper 3's 4-target shader compilation, `.gbew` v4 binary cooking, `10,000`-step
/// bit-exact `ganbate-physics` simulation, and `90Hz VR` closed-loop frame governance suite.
pub fn run_paper3_polyglot_systems() -> Result<Paper3PolyglotSystems, String> {
    let shader_targets = measure_four_shader_backends()?;
    let all_four_targets_verified =
        shader_targets.len() == 4 && shader_targets.iter().all(|t| t.verified);
    let gbew = measure_gbew_cooking()?;
    let physics = measure_physics_determinism_10k()?;
    let (
        vr_90hz_target_ms,
        vr_90hz_step_down_frames,
        vr_90hz_recovery_frames,
        vr_90hz_governor_verified,
    ) = verify_vr_90hz_governor();

    Ok(Paper3PolyglotSystems {
        shader_targets,
        all_four_targets_verified,
        gbew,
        physics,
        vr_90hz_target_ms,
        vr_90hz_step_down_frames,
        vr_90hz_recovery_frames,
        vr_90hz_governor_verified,
    })
}
