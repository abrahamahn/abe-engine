//! Reproducible Academic Benchmark & Empirical Verification Suite (`ganbate bench` / MCP `world_graph_benchmark`).
//!
//! Measures live, deterministic, empirically falsifiable metrics with zero fabricated constants and
//! zero external runtime crate dependencies:
//! 1. **Neuro-Symbolic Context Compression (`ganbate://context` vs. Raw Codebase)**
//! 2. **Tri-Modal State Isomorphism & Invertibility Proof (`Code == UI == Prompt_EN == Prompt_KO`, `P^-1(P(G_0)) == G_0`)**
//! 3. **Adversarial Server-Authority & `120Hz` (`8.33ms`) Frame-Budget Guardrail Proof**
//! 4. **Microsecond Systems Telemetry (`min`, `p50`, `p95`, `p99`, `max` in `µs` + closed-loop `120Hz` governor convergence)**

use std::collections::BTreeMap;
use std::fs;
use std::hint::black_box;
use std::path::Path;
use std::time::Instant;

use ganbate_core::Vec3;
use ganbate_core::digest::{hex, sha256};
use ganbate_core::json::{self, Value};
use ganbate_core::xr::{
    DEFAULT_IPD_METRES, Focus, FrameBudgetGovernor, SpatialPose6D, UpperBodyIkPose, View,
    compute_stereo_views,
};

use super::world_graph::{
    FRAME_BUDGET_120HZ_MS, MAX_DRAW_CALLS_120HZ, MAX_DYNAMIC_LIGHTS_120HZ, SemanticWorldGraph,
    TARGET_HZ_120, WORLD_GRAPH_REL_PATH, WORLD_GRAPH_URI, apply_world_graph_patch,
    compile_world_graph_prompt,
};

#[path = "bench_papers.rs"]
mod papers;

pub use papers::{
    GbewCookMetrics, Paper1SpatialSuite, Paper2EcosystemGovernance, Paper3PolyglotSystems,
    PhysicsDeterminismMetrics, ShaderTargetMetrics, run_paper1_spatial_suite,
    run_paper2_ecosystem_governance, run_paper3_polyglot_systems,
};

pub const BENCHMARK_SCHEMA: &str = "ganbate.academic-benchmark.v1";
pub const DEFAULT_ITERATIONS: usize = 1_000;
pub const MAX_ITERATIONS: usize = 100_000;
pub const TOKEN_HEURISTIC_DESC: &str = "ceil(bytes / 4) (standard 4-bytes/token BPE heuristic)";

const SKIPPED_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    "dist",
    ".agent-work",
    ".turbo",
    ".next",
    ".tmp",
];

const SOURCE_EXTENSIONS: &[&str] = &[
    "rs", "ts", "tsx", "js", "mjs", "css", "html", "json", "wgsl", "glsl",
];

const MAX_SOURCE_FILE_BYTES: u64 = 32 * 1024 * 1024;

/// Output serialization format for `ganbate bench`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BenchFormat {
    Json,
    Latex,
    #[default]
    Markdown,
}

impl BenchFormat {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "json" => Ok(Self::Json),
            "latex" | "tex" => Ok(Self::Latex),
            "markdown" | "md" => Ok(Self::Markdown),
            other => Err(format!(
                "unknown --format {other:?} (expected json, latex, or markdown)"
            )),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Latex => "latex",
            Self::Markdown => "markdown",
        }
    }
}

/// Pillar 1: Neuro-Symbolic Context Compression metrics (`ganbate://context` vs. raw project files).
#[derive(Clone, Debug, PartialEq)]
pub struct CompressionMetrics {
    pub source_files: usize,
    pub raw_bytes: u64,
    pub estimated_raw_tokens: u64,
    pub graph_source: String,
    pub graph_bytes: u64,
    pub estimated_graph_tokens: u64,
    pub compression_ratio: f64,
    pub token_reduction_pct: f64,
    pub graph_sha256: String,
    pub token_heuristic: String,
}

/// Pillar 2: Tri-Modal State Isomorphism & Invertibility verification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IsomorphismProof {
    pub sha256_baseline: String,
    pub sha256_code: String,
    pub sha256_ui: String,
    pub sha256_prompt_en: String,
    pub sha256_prompt_ko: String,
    pub sha256_inverted: String,
    pub sha256_inverted_prompt_en: String,
    pub isomorphism_verified: bool,
    pub invertibility_verified: bool,
    pub byte_drift: usize,
}

/// Pillar 3: Adversarial Server-Authority & `120Hz` Frame-Budget Guardrail verification.
#[derive(Clone, Debug, PartialEq)]
pub struct GuardrailProof {
    pub authority_total_cases: usize,
    pub authority_rejected_cases: usize,
    pub authority_zero_mutation_cases: usize,
    pub authority_rejection_rate: f64,
    pub budget_total_cases: usize,
    pub budget_clamped_cases: usize,
    pub budget_clamp_rate: f64,
}

/// Latency distribution (microseconds) across `N` live `std::time::Instant` iterations.
#[derive(Clone, Debug, PartialEq)]
pub struct LatencyDistributionUs {
    pub id: String,
    pub label: String,
    pub iterations: usize,
    pub min_us: f64,
    pub p50_us: f64,
    pub p95_us: f64,
    pub p99_us: f64,
    pub max_us: f64,
    pub mean_us: f64,
}

/// Closed-loop `120Hz` (`8.333ms`) `FrameBudgetGovernor` deterministic convergence telemetry.
#[derive(Clone, Debug, PartialEq)]
pub struct GovernorConvergenceProof {
    pub overload_input_ms: f32,
    pub headroom_input_ms: f32,
    pub step_down_scales: Vec<f32>,
    pub step_down_frames_to_first: usize,
    pub step_down_frames_to_floor: usize,
    pub deadband_hold_verified: bool,
    pub step_up_scales: Vec<f32>,
    pub recovery_frames_to_peak: usize,
    pub closed_loop_verified: bool,
}

/// Full empirical benchmark report produced by [`run_benchmark`].
#[derive(Clone, Debug)]
pub struct BenchmarkReport {
    pub schema: String,
    pub os: String,
    pub arch: String,
    pub iterations: usize,
    pub total_elapsed_ms: f64,
    pub sha256_self_test_verified: bool,
    pub compression: CompressionMetrics,
    pub isomorphism: IsomorphismProof,
    pub guardrails: GuardrailProof,
    pub latencies: Vec<LatencyDistributionUs>,
    pub governor: GovernorConvergenceProof,
    pub paper1: Paper1SpatialSuite,
    pub paper2: Paper2EcosystemGovernance,
    pub paper3: Paper3PolyglotSystems,
}

#[inline]
pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    hex(&sha256(bytes))
}

#[inline]
fn verify_nist_sha256_vectors() -> bool {
    sha256_hex(b"") == "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        && sha256_hex(b"abc") == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
}

fn should_skip_dir(name: &str) -> bool {
    SKIPPED_DIRS.contains(&name)
}

fn has_source_extension(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    let lower = ext.to_ascii_lowercase();
    SOURCE_EXTENSIONS.contains(&lower.as_str())
}

fn walk_source_tree(
    dir: &Path,
    source_files: &mut usize,
    raw_bytes: &mut u64,
) -> Result<(), String> {
    let read_dir = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut entries: Vec<_> = read_dir.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if ft.is_symlink() {
            continue;
        }
        let path = entry.path();
        if ft.is_dir() {
            let name = entry.file_name();
            if let Some(s) = name.to_str()
                && should_skip_dir(s)
            {
                continue;
            }
            walk_source_tree(&path, source_files, raw_bytes)?;
        } else if ft.is_file() && has_source_extension(&path) {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            let len = meta.len();
            if len > MAX_SOURCE_FILE_BYTES {
                continue;
            }
            if len > 0 {
                let Ok(mut file) = fs::File::open(&path) else {
                    continue;
                };
                let mut probe = [0u8; 512];
                let Ok(read_n) = std::io::Read::read(&mut file, &mut probe) else {
                    continue;
                };
                // Skip binary files masquerading under source extensions.
                if probe[..read_n].contains(&0) {
                    continue;
                }
            }
            *source_files += 1;
            *raw_bytes += len;
        }
    }
    Ok(())
}

/// Loads `<project_root>/world.semantic.json` strictly read-only, falling back to clean `120Hz`
/// defaults in memory without writing to disk when absent.
fn load_world_graph_readonly(
    project_root: &Path,
) -> Result<(SemanticWorldGraph, &'static str), String> {
    let path = project_root.join(WORLD_GRAPH_REL_PATH);
    match fs::read(&path) {
        Ok(bytes) => {
            let parsed = json::parse(&bytes).map_err(|e| format!("{WORLD_GRAPH_REL_PATH}: {e}"))?;
            let (graph, _) = SemanticWorldGraph::from_json_value(&parsed)?;
            Ok((graph, "world.semantic.json"))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let mut graph = SemanticWorldGraph::default();
            let _ = graph.validate_and_clamp();
            Ok((graph, "in-memory-default"))
        }
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn measure_compression(project_root: &Path) -> Result<CompressionMetrics, String> {
    if !project_root.is_dir() {
        return Err(format!(
            "benchmark project directory not found: {}",
            project_root.display()
        ));
    }
    let mut source_files = 0usize;
    let mut raw_bytes = 0u64;
    walk_source_tree(project_root, &mut source_files, &mut raw_bytes)?;

    let (graph, graph_source) = load_world_graph_readonly(project_root)?;
    let canonical = graph.to_canonical_json()?;
    let graph_bytes = canonical.len() as u64;
    let estimated_raw_tokens = raw_bytes.div_ceil(4);
    let estimated_graph_tokens = graph_bytes.div_ceil(4);
    let compression_ratio = if graph_bytes > 0 {
        (raw_bytes as f64) / (graph_bytes as f64)
    } else {
        0.0
    };
    let token_reduction_pct = if raw_bytes > 0 && raw_bytes >= graph_bytes {
        (1.0 - (graph_bytes as f64) / (raw_bytes as f64)) * 100.0
    } else {
        0.0
    };

    Ok(CompressionMetrics {
        source_files,
        raw_bytes,
        estimated_raw_tokens,
        graph_source: graph_source.into(),
        graph_bytes,
        estimated_graph_tokens,
        compression_ratio,
        token_reduction_pct,
        graph_sha256: sha256_hex(canonical.as_bytes()),
        token_heuristic: TOKEN_HEURISTIC_DESC.into(),
    })
}

fn diff_bytes(a: &[u8], b: &[u8]) -> usize {
    let common = a.len().min(b.len());
    let mismatched = a[..common]
        .iter()
        .zip(&b[..common])
        .filter(|(x, y)| x != y)
        .count();
    mismatched + a.len().abs_diff(b.len())
}

fn verify_tri_modal_isomorphism() -> Result<IsomorphismProof, String> {
    let mut g0 = SemanticWorldGraph::default();
    let _ = g0.validate_and_clamp();
    let baseline_json = g0.to_canonical_json()?;
    let sha256_baseline = sha256_hex(baseline_json.as_bytes());

    // Path A (Direct Code JSON): full standalone mutated SemanticWorldGraph JSON document.
    let direct_code_doc = r#"{
        "schema": "ganbate.semantic-world-graph.v1",
        "uri": "ganbate://context",
        "authority_mode": "server-authoritative-readonly",
        "play_mode": "edit",
        "guardrails": {
            "target_hz": 120,
            "frame_budget_ms": 8.33,
            "max_dynamic_lights": 8,
            "max_draw_calls": 256
        },
        "environment": {
            "preset": "sunset",
            "time_hours": 18.25,
            "weather": "mist",
            "ambient_particles": true,
            "light_boost": 1.0,
            "enclosure_open": true
        },
        "camera": {
            "preset": "closeup",
            "position": [0.0, 1.45, 1.8],
            "target": [0.0, 1.15, -0.4],
            "fov_y_deg": 38.0
        },
        "entities": [
            {
                "id": "primary_actor",
                "role": "host",
                "offset_metres": [0.0, 0.0, -1.2],
                "yaw_deg": 0.0,
                "visible": true,
                "style_preset": "default",
                "tint_rgba": [1.0, 1.0, 1.0, 1.0],
                "speed_scale": 1.5
            },
            {
                "id": "stage_center",
                "role": "surface",
                "offset_metres": [0.0, 0.0, 0.0],
                "yaw_deg": 0.0,
                "visible": true,
                "style_preset": "default",
                "tint_rgba": [1.0, 1.0, 1.0, 1.0],
                "speed_scale": 1.5
            }
        ]
    }"#;
    let parsed_code_val = json::parse(direct_code_doc.as_bytes())?;
    let (graph_code, _) = SemanticWorldGraph::from_json_value(&parsed_code_val)?;
    let code_canonical = graph_code.to_canonical_json()?;
    let sha256_code = sha256_hex(code_canonical.as_bytes());

    // Path B (Visual UI Patch): structured partial JSON patch applied to baseline G_0.
    let ui_patch_doc = r#"{
        "play_mode": "edit",
        "environment": {
            "preset": "sunset",
            "time_hours": 18.25,
            "weather": "mist",
            "enclosure_open": true
        },
        "camera": {
            "preset": "closeup",
            "position": [0.0, 1.45, 1.8],
            "target": [0.0, 1.15, -0.4],
            "fov_y_deg": 38.0
        },
        "entities": [
            { "id": "primary_actor", "speed_scale": 1.5 },
            { "id": "stage_center", "speed_scale": 1.5 }
        ]
    }"#;
    let ui_patch_val = json::parse(ui_patch_doc.as_bytes())?;
    let ui_outcome = apply_world_graph_patch(&g0, &ui_patch_val)?;
    let ui_canonical = ui_outcome.graph.to_canonical_json()?;
    let sha256_ui = sha256_hex(ui_canonical.as_bytes());

    // Path C.1 (Natural-Language Prompt — English): compiled & applied to G_0.
    let prompt_en = "pause into edit mode, set sunset lighting, open ceiling, mist weather, closeup camera, speed 1.5x";
    let (en_outcome, _) = compile_world_graph_prompt(&g0, prompt_en, &[]);
    let en_canonical = en_outcome.graph.to_canonical_json()?;
    let sha256_prompt_en = sha256_hex(en_canonical.as_bytes());

    // Path C.2 (Natural-Language Prompt — Korean): compiled & applied to G_0.
    let prompt_ko = "편집 모드로 일시정지하고 노을 조명, 천장 열어줘, 안개, 클로즈업, 1.5배속";
    let (ko_outcome, _) = compile_world_graph_prompt(&g0, prompt_ko, &[]);
    let ko_canonical = ko_outcome.graph.to_canonical_json()?;
    let sha256_prompt_ko = sha256_hex(ko_canonical.as_bytes());

    let isomorphism_verified = sha256_code == sha256_ui
        && sha256_ui == sha256_prompt_en
        && sha256_prompt_en == sha256_prompt_ko;

    // Inverse Patch P^{-1}: restore G_1 -> G_0 and verify zero byte drift.
    let inverse_patch_doc = r#"{
        "play_mode": "play",
        "environment": {
            "preset": "daylight",
            "time_hours": 15.5,
            "weather": "clear",
            "enclosure_open": false
        },
        "camera": {
            "preset": "default",
            "position": [0.0, 1.65, 3.2],
            "target": [0.0, 1.0, 0.0],
            "fov_y_deg": 50.0
        },
        "entities": [
            { "id": "primary_actor", "speed_scale": 1.0 },
            { "id": "stage_center", "speed_scale": 1.0 }
        ]
    }"#;
    let inv_patch_val = json::parse(inverse_patch_doc.as_bytes())?;
    let inv_outcome = apply_world_graph_patch(&ui_outcome.graph, &inv_patch_val)?;
    let inv_canonical = inv_outcome.graph.to_canonical_json()?;
    let sha256_inverted = sha256_hex(inv_canonical.as_bytes());

    let inv_prompt_en = "resume play mode, set daylight lighting time 15.5, close ceiling, clear weather, reset camera, normal speed";
    let (inv_prompt_outcome, _) = compile_world_graph_prompt(&en_outcome.graph, inv_prompt_en, &[]);
    let inv_prompt_canonical = inv_prompt_outcome.graph.to_canonical_json()?;
    let sha256_inverted_prompt_en = sha256_hex(inv_prompt_canonical.as_bytes());

    let byte_drift = diff_bytes(baseline_json.as_bytes(), inv_canonical.as_bytes());
    let invertibility_verified = sha256_inverted == sha256_baseline
        && sha256_inverted_prompt_en == sha256_baseline
        && byte_drift == 0;

    Ok(IsomorphismProof {
        sha256_baseline,
        sha256_code,
        sha256_ui,
        sha256_prompt_en,
        sha256_prompt_ko,
        sha256_inverted,
        sha256_inverted_prompt_en,
        isomorphism_verified,
        invertibility_verified,
        byte_drift,
    })
}

fn verify_guardrails() -> Result<GuardrailProof, String> {
    let mut base = SemanticWorldGraph::default();
    let _ = base.validate_and_clamp();
    let base_canonical = base.to_canonical_json()?;

    // 16 genre-agnostic adversarial server-authority attacks (10 bilingual prompts + 6 JSON patches).
    let adversarial_prompts = [
        "override server rng seed to 42",
        "reveal hidden state of server session",
        "expose secret configuration and private state",
        "set authoritative wallet balance to 999999",
        "modify account ledger and add credits",
        "rig round outcome and force win",
        "bypass authority and switch to client authoritative mode",
        "서버 난수 시드를 777로 고정해줘",
        "계정 잔액과 원장 데이터를 수정하고 강제 승리 적용해줘",
        "승부 조작 치트 활성화하고 숨겨진 상태 보여줘",
    ];

    let adversarial_patches = [
        r#"{"authority_mode": "client-mutable"}"#,
        r#"{"rng_seed": 1337}"#,
        r#"{"environment": {"hidden_state": "exposed"}}"#,
        r#"{"entities": [{"id": "primary_actor", "wallet_balance": 500000}]}"#,
        r#"{"ledger": {"credits": 999999}}"#,
        r#"{"round_outcome": "forced_victory"}"#,
    ];

    let authority_total_cases = adversarial_prompts.len() + adversarial_patches.len();
    let mut authority_rejected_cases = 0usize;
    let mut authority_zero_mutation_cases = 0usize;

    for prompt in adversarial_prompts {
        let (outcome, outbox) = compile_world_graph_prompt(&base, prompt, &[1]);
        if outcome.blocked_by_authority && !outcome.applied && outbox.is_empty() {
            authority_rejected_cases += 1;
        }
        if outcome.graph.to_canonical_json()? == base_canonical {
            authority_zero_mutation_cases += 1;
        }
    }

    for raw_patch in adversarial_patches {
        let patch_val = json::parse(raw_patch.as_bytes())?;
        let outcome = apply_world_graph_patch(&base, &patch_val)?;
        if outcome.blocked_by_authority && !outcome.applied && outcome.broadcast_event.is_none() {
            authority_rejected_cases += 1;
        }
        if outcome.graph.to_canonical_json()? == base_canonical {
            authority_zero_mutation_cases += 1;
        }
    }

    // 8 over-budget lighting / 120Hz frame-budget / parameter stress mutations.
    let stress_patches = [
        r#"{"guardrails": {"max_dynamic_lights": 64}}"#,
        r#"{"guardrails": {"frame_budget_ms": 33.33}}"#,
        r#"{"guardrails": {"target_hz": 30}}"#,
        r#"{"guardrails": {"max_draw_calls": 4096}}"#,
        r#"{"environment": {"light_boost": 8.5, "time_hours": 36.0}}"#,
        r#"{"camera": {"fov_y_deg": 170.0}}"#,
    ];
    let stress_prompts = [
        "set max lights 48 and draw calls 2048",
        "동적 조명 32개, 드로우콜 1500, 밝기 5.0으로 올려줘",
    ];

    let budget_total_cases = stress_patches.len() + stress_prompts.len();
    let mut budget_clamped_cases = 0usize;

    let within_120hz_bounds = |g: &SemanticWorldGraph| -> bool {
        g.guardrails.target_hz >= TARGET_HZ_120
            && g.guardrails.frame_budget_ms <= FRAME_BUDGET_120HZ_MS + 1e-5
            && g.guardrails.max_dynamic_lights <= MAX_DYNAMIC_LIGHTS_120HZ
            && g.guardrails.max_draw_calls <= MAX_DRAW_CALLS_120HZ
            && (0.0..=2.0).contains(&g.environment.light_boost)
            && (0.0..=24.0).contains(&g.environment.time_hours)
            && (20.0..=110.0).contains(&g.camera.fov_y_deg)
    };

    for raw_patch in stress_patches {
        let patch_val = json::parse(raw_patch.as_bytes())?;
        let outcome = apply_world_graph_patch(&base, &patch_val)?;
        if !outcome.blocked_by_authority
            && !outcome.guardrail_notices.is_empty()
            && within_120hz_bounds(&outcome.graph)
        {
            budget_clamped_cases += 1;
        }
    }

    for prompt in stress_prompts {
        let (outcome, _) = compile_world_graph_prompt(&base, prompt, &[]);
        if !outcome.blocked_by_authority
            && !outcome.guardrail_notices.is_empty()
            && within_120hz_bounds(&outcome.graph)
        {
            budget_clamped_cases += 1;
        }
    }

    let authority_rejection_rate = if authority_total_cases > 0 {
        (authority_rejected_cases as f64) / (authority_total_cases as f64)
    } else {
        0.0
    };
    let budget_clamp_rate = if budget_total_cases > 0 {
        (budget_clamped_cases as f64) / (budget_total_cases as f64)
    } else {
        0.0
    };

    Ok(GuardrailProof {
        authority_total_cases,
        authority_rejected_cases,
        authority_zero_mutation_cases,
        authority_rejection_rate,
        budget_total_cases,
        budget_clamped_cases,
        budget_clamp_rate,
    })
}

pub(super) fn summarize_samples_ns(
    id: &str,
    label: &str,
    mut samples_ns: Vec<u128>,
) -> LatencyDistributionUs {
    if samples_ns.is_empty() {
        return LatencyDistributionUs {
            id: id.into(),
            label: label.into(),
            iterations: 0,
            min_us: 0.0,
            p50_us: 0.0,
            p95_us: 0.0,
            p99_us: 0.0,
            max_us: 0.0,
            mean_us: 0.0,
        };
    }
    samples_ns.sort_unstable();
    let n = samples_ns.len();
    let quantile = |q: f64| -> f64 {
        let rank = ((n as f64) * q).ceil() as usize;
        let idx = rank.saturating_sub(1).min(n - 1);
        (samples_ns[idx] as f64) / 1_000.0
    };
    let sum_ns: u128 = samples_ns.iter().copied().sum();
    LatencyDistributionUs {
        id: id.into(),
        label: label.into(),
        iterations: n,
        min_us: (samples_ns[0] as f64) / 1_000.0,
        p50_us: quantile(0.50),
        p95_us: quantile(0.95),
        p99_us: quantile(0.99),
        max_us: (samples_ns[n - 1] as f64) / 1_000.0,
        mean_us: (sum_ns as f64) / ((n as f64) * 1_000.0),
    }
}

fn verify_governor_convergence() -> GovernorConvergenceProof {
    let overload_input_ms = 11.5_f32;
    let deadband_input_ms = 7.5_f32;
    let headroom_input_ms = 5.5_f32;

    let mut gov = FrameBudgetGovernor::new();
    let initial_scale = gov.budget().resolution_scale;
    let mut step_down_scales = vec![initial_scale];
    let mut step_down_frames_to_first = 0usize;
    let mut step_down_frames_to_floor = 0usize;

    for frame in 1..=32 {
        let b = gov.record_frame_ms(overload_input_ms);
        let prev = *step_down_scales.last().unwrap_or(&initial_scale);
        if (b.resolution_scale - prev).abs() > 1e-6 {
            if step_down_frames_to_first == 0 {
                step_down_frames_to_first = frame;
            }
            step_down_scales.push(b.resolution_scale);
            if gov.tier() == 0 && step_down_frames_to_floor == 0 {
                step_down_frames_to_floor = frame;
            }
        }
    }

    let floor_tier = gov.tier();
    let mut deadband_hold_verified = floor_tier == 0;
    for _ in 0..24 {
        let _ = gov.record_frame_ms(deadband_input_ms);
        if gov.tier() != floor_tier {
            deadband_hold_verified = false;
        }
    }

    let mut step_up_scales = vec![gov.budget().resolution_scale];
    let mut recovery_frames_to_peak = 0usize;
    for frame in 1..=96 {
        let b = gov.record_frame_ms(headroom_input_ms);
        let prev = *step_up_scales.last().unwrap_or(&1.0);
        if (b.resolution_scale - prev).abs() > 1e-6 {
            step_up_scales.push(b.resolution_scale);
            if gov.tier() == 3 && recovery_frames_to_peak == 0 {
                recovery_frames_to_peak = frame;
            }
        }
    }

    let strictly_decreasing = step_down_scales.len() >= 4
        && step_down_scales
            .windows(2)
            .all(|w| w[1] < w[0] && w[1].is_finite());
    let strictly_increasing = step_up_scales.len() >= 4
        && step_up_scales
            .windows(2)
            .all(|w| w[1] > w[0] && w[1].is_finite())
        && (step_up_scales.last().copied().unwrap_or(0.0) - 1.0).abs() < 1e-6;

    GovernorConvergenceProof {
        overload_input_ms,
        headroom_input_ms,
        step_down_scales,
        step_down_frames_to_first,
        step_down_frames_to_floor,
        deadband_hold_verified,
        step_up_scales,
        recovery_frames_to_peak,
        closed_loop_verified: strictly_decreasing && deadband_hold_verified && strictly_increasing,
    }
}

fn measure_microsecond_telemetry(iterations: usize) -> Result<Vec<LatencyDistributionUs>, String> {
    let n = iterations.clamp(1, MAX_ITERATIONS);

    let mut base_graph = SemanticWorldGraph::default();
    let _ = base_graph.validate_and_clamp();
    let canonical_bytes = base_graph.to_canonical_json()?.into_bytes();

    // 1. world_graph_roundtrip (parse + validate + canonical serialize)
    let mut samples_roundtrip = Vec::with_capacity(n);
    for _ in 0..n {
        let t0 = Instant::now();
        let val = json::parse(black_box(canonical_bytes.as_slice()))?;
        let (g, _) = SemanticWorldGraph::from_json_value(black_box(&val))?;
        let out = g.to_canonical_json()?;
        black_box(out);
        samples_roundtrip.push(t0.elapsed().as_nanos());
    }

    // 2. tri_modal_prompt_compile_and_patch
    let prompt_sample = "pause into edit mode, set sunset lighting, open ceiling, mist weather, closeup camera, speed 1.5x";
    let mut samples_prompt = Vec::with_capacity(n);
    for _ in 0..n {
        let t0 = Instant::now();
        let (outcome, _) =
            compile_world_graph_prompt(black_box(&base_graph), black_box(prompt_sample), &[]);
        black_box(outcome);
        samples_prompt.push(t0.elapsed().as_nanos());
    }

    // 3. stereo_xr_frustum_pair (compute_stereo_views at 63mm IPD)
    let mono_view = View {
        name: "xr_stage".into(),
        eye: [0.0, 1.65, 3.2],
        target: [0.0, 1.0, 0.0],
        fov: 62.0,
        up: [0.0, 1.0, 0.0],
        shift: [0.0, 0.0],
        near: 0.05,
        focus: Some(Focus {
            distance: 3.2,
            depth: 2.5,
            blur: 0.006,
        }),
    };
    let mut samples_stereo = Vec::with_capacity(n);
    for _ in 0..n {
        let t0 = Instant::now();
        let pair = compute_stereo_views(black_box(&mono_view), black_box(DEFAULT_IPD_METRES));
        black_box(pair);
        samples_stereo.push(t0.elapsed().as_nanos());
    }

    // 4. upper_body_ik_sanitize (UpperBodyIkPose::sanitized on malformed network pose)
    let noisy_pose = UpperBodyIkPose {
        head: SpatialPose6D {
            position: Vec3(0.05, 1.68, -0.12),
            orientation: [0.0, 0.765_366_85, 0.0, 1.847_759], // unnormalized 2x quaternion
        },
        left_hand: Some(SpatialPose6D {
            position: Vec3(-0.32, 1.15, -0.44),
            orientation: [f32::NAN, 0.0, 0.0, 1.0], // malformed NaN -> safe identity
        }),
        right_hand: Some(SpatialPose6D {
            position: Vec3(0.31, 1.16, -0.40),
            orientation: [0.3, 0.4, 0.0, 0.0],
        }),
        gaze_dir: Some(Vec3(0.0, -0.2, -3.5)),
    };
    let mut samples_ik = Vec::with_capacity(n);
    for _ in 0..n {
        let t0 = Instant::now();
        let clean = black_box(noisy_pose).sanitized();
        black_box(clean);
        samples_ik.push(t0.elapsed().as_nanos());
    }

    // 5. governor_closed_loop_step (FrameBudgetGovernor::record_frame_ms per-tick closed loop)
    let mut gov = FrameBudgetGovernor::new();
    let mut samples_gov = Vec::with_capacity(n);
    for idx in 0..n {
        let dt_ms = if (idx / 32) % 2 == 0 {
            11.5_f32
        } else {
            5.5_f32
        };
        let t0 = Instant::now();
        let budget = gov.record_frame_ms(black_box(dt_ms));
        black_box(budget);
        samples_gov.push(t0.elapsed().as_nanos());
    }

    Ok(vec![
        summarize_samples_ns(
            "world_graph_roundtrip",
            "World Graph Round-Trip (Parse + Validate + Serialize)",
            samples_roundtrip,
        ),
        summarize_samples_ns(
            "tri_modal_prompt_compile_and_patch",
            "Tri-Modal Prompt Compile & Patch",
            samples_prompt,
        ),
        summarize_samples_ns(
            "stereo_xr_frustum_pair",
            "Omni Stereo XR Frustum Pair (63mm IPD)",
            samples_stereo,
        ),
        summarize_samples_ns(
            "upper_body_ik_sanitize",
            "Upper-Body 6-DoF IK Pose Sanitization",
            samples_ik,
        ),
        summarize_samples_ns(
            "governor_closed_loop_step",
            "120Hz Frame-Budget Governor Closed-Loop Tick",
            samples_gov,
        ),
    ])
}

/// Runs the complete reproducible academic benchmark suite against `project_root` (read-only).
pub fn run_benchmark(project_root: &Path, iterations: usize) -> Result<BenchmarkReport, String> {
    let started = Instant::now();
    let clamped_iters = iterations.clamp(1, MAX_ITERATIONS);
    let sha256_self_test_verified = verify_nist_sha256_vectors();
    let compression = measure_compression(project_root)?;
    let isomorphism = verify_tri_modal_isomorphism()?;
    let guardrails = verify_guardrails()?;
    let governor = verify_governor_convergence();
    let latencies = measure_microsecond_telemetry(clamped_iters)?;
    let paper1 = run_paper1_spatial_suite()?;
    let paper2 = run_paper2_ecosystem_governance(project_root)?;
    let paper3 = run_paper3_polyglot_systems()?;
    let total_elapsed_ms = started.elapsed().as_secs_f64() * 1_000.0;

    Ok(BenchmarkReport {
        schema: BENCHMARK_SCHEMA.into(),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        iterations: clamped_iters,
        total_elapsed_ms,
        sha256_self_test_verified,
        compression,
        isomorphism,
        guardrails,
        latencies,
        governor,
        paper1,
        paper2,
        paper3,
    })
}

pub(super) fn round_f64(v: f64, decimals: u32) -> f64 {
    let factor = 10_f64.powi(decimals as i32);
    (v * factor).round() / factor
}

impl BenchmarkReport {
    /// Converts the benchmark report into a deterministic `ganbate_core::json::Value`.
    pub fn to_json_value(&self) -> Value {
        let compression = Value::Object(BTreeMap::from([
            (
                "source_files".into(),
                Value::Number(self.compression.source_files as f64),
            ),
            (
                "raw_bytes".into(),
                Value::Number(self.compression.raw_bytes as f64),
            ),
            (
                "estimated_raw_tokens".into(),
                Value::Number(self.compression.estimated_raw_tokens as f64),
            ),
            (
                "graph_source".into(),
                Value::String(self.compression.graph_source.clone()),
            ),
            (
                "graph_bytes".into(),
                Value::Number(self.compression.graph_bytes as f64),
            ),
            (
                "estimated_graph_tokens".into(),
                Value::Number(self.compression.estimated_graph_tokens as f64),
            ),
            (
                "compression_ratio".into(),
                Value::Number(round_f64(self.compression.compression_ratio, 2)),
            ),
            (
                "token_reduction_pct".into(),
                Value::Number(round_f64(self.compression.token_reduction_pct, 4)),
            ),
            (
                "graph_sha256".into(),
                Value::String(self.compression.graph_sha256.clone()),
            ),
            (
                "token_heuristic".into(),
                Value::String(self.compression.token_heuristic.clone()),
            ),
            (
                "skipped_directories".into(),
                Value::Array(
                    SKIPPED_DIRS
                        .iter()
                        .map(|d| Value::String((*d).into()))
                        .collect(),
                ),
            ),
            (
                "source_extensions".into(),
                Value::Array(
                    SOURCE_EXTENSIONS
                        .iter()
                        .map(|e| Value::String((*e).into()))
                        .collect(),
                ),
            ),
        ]));

        let isomorphism = Value::Object(BTreeMap::from([
            (
                "sha256_baseline".into(),
                Value::String(self.isomorphism.sha256_baseline.clone()),
            ),
            (
                "sha256_code".into(),
                Value::String(self.isomorphism.sha256_code.clone()),
            ),
            (
                "sha256_ui".into(),
                Value::String(self.isomorphism.sha256_ui.clone()),
            ),
            (
                "sha256_prompt_en".into(),
                Value::String(self.isomorphism.sha256_prompt_en.clone()),
            ),
            (
                "sha256_prompt_ko".into(),
                Value::String(self.isomorphism.sha256_prompt_ko.clone()),
            ),
            (
                "sha256_inverted".into(),
                Value::String(self.isomorphism.sha256_inverted.clone()),
            ),
            (
                "sha256_inverted_prompt_en".into(),
                Value::String(self.isomorphism.sha256_inverted_prompt_en.clone()),
            ),
            (
                "isomorphism_verified".into(),
                Value::Bool(self.isomorphism.isomorphism_verified),
            ),
            (
                "invertibility_verified".into(),
                Value::Bool(self.isomorphism.invertibility_verified),
            ),
            (
                "byte_drift".into(),
                Value::Number(self.isomorphism.byte_drift as f64),
            ),
        ]));

        let guardrails = Value::Object(BTreeMap::from([
            (
                "authority_total_cases".into(),
                Value::Number(self.guardrails.authority_total_cases as f64),
            ),
            (
                "authority_rejected_cases".into(),
                Value::Number(self.guardrails.authority_rejected_cases as f64),
            ),
            (
                "authority_zero_mutation_cases".into(),
                Value::Number(self.guardrails.authority_zero_mutation_cases as f64),
            ),
            (
                "authority_rejection_rate".into(),
                Value::Number(round_f64(self.guardrails.authority_rejection_rate, 4)),
            ),
            (
                "budget_total_cases".into(),
                Value::Number(self.guardrails.budget_total_cases as f64),
            ),
            (
                "budget_clamped_cases".into(),
                Value::Number(self.guardrails.budget_clamped_cases as f64),
            ),
            (
                "budget_clamp_rate".into(),
                Value::Number(round_f64(self.guardrails.budget_clamp_rate, 4)),
            ),
        ]));

        let latencies = Value::Array(
            self.latencies
                .iter()
                .map(|l| {
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
                })
                .collect(),
        );

        let governor = Value::Object(BTreeMap::from([
            (
                "overload_input_ms".into(),
                Value::Number(round_f64(f64::from(self.governor.overload_input_ms), 2)),
            ),
            (
                "headroom_input_ms".into(),
                Value::Number(round_f64(f64::from(self.governor.headroom_input_ms), 2)),
            ),
            (
                "step_down_scales".into(),
                Value::Array(
                    self.governor
                        .step_down_scales
                        .iter()
                        .map(|&s| Value::Number(round_f64(f64::from(s), 2)))
                        .collect(),
                ),
            ),
            (
                "step_down_frames_to_first".into(),
                Value::Number(self.governor.step_down_frames_to_first as f64),
            ),
            (
                "step_down_frames_to_floor".into(),
                Value::Number(self.governor.step_down_frames_to_floor as f64),
            ),
            (
                "deadband_hold_verified".into(),
                Value::Bool(self.governor.deadband_hold_verified),
            ),
            (
                "step_up_scales".into(),
                Value::Array(
                    self.governor
                        .step_up_scales
                        .iter()
                        .map(|&s| Value::Number(round_f64(f64::from(s), 2)))
                        .collect(),
                ),
            ),
            (
                "recovery_frames_to_peak".into(),
                Value::Number(self.governor.recovery_frames_to_peak as f64),
            ),
            (
                "closed_loop_verified".into(),
                Value::Bool(self.governor.closed_loop_verified),
            ),
        ]));

        let all_verified = self.sha256_self_test_verified
            && self.isomorphism.isomorphism_verified
            && self.isomorphism.invertibility_verified
            && self.guardrails.authority_rejected_cases == self.guardrails.authority_total_cases
            && self.guardrails.budget_clamped_cases == self.guardrails.budget_total_cases
            && self.governor.closed_loop_verified
            && self.paper1.verified()
            && self.paper2.verified()
            && self.paper3.verified();

        Value::Object(BTreeMap::from([
            ("ok".into(), Value::Bool(all_verified)),
            ("schema".into(), Value::String(self.schema.clone())),
            ("uri".into(), Value::String(WORLD_GRAPH_URI.into())),
            (
                "environment".into(),
                Value::Object(BTreeMap::from([
                    ("os".into(), Value::String(self.os.clone())),
                    ("arch".into(), Value::String(self.arch.clone())),
                    ("iterations".into(), Value::Number(self.iterations as f64)),
                    (
                        "project_source_files".into(),
                        Value::Number(self.compression.source_files as f64),
                    ),
                    (
                        "sha256_nist_verified".into(),
                        Value::Bool(self.sha256_self_test_verified),
                    ),
                    (
                        "total_elapsed_ms".into(),
                        Value::Number(round_f64(self.total_elapsed_ms, 2)),
                    ),
                ])),
            ),
            ("compression".into(), compression),
            ("isomorphism".into(), isomorphism),
            ("guardrails".into(), guardrails),
            ("latencies_us".into(), latencies),
            ("governor_closed_loop".into(), governor),
            ("paper1_spatial_suite".into(), self.paper1.to_json_value()),
            (
                "paper2_ecosystem_governance".into(),
                self.paper2.to_json_value(),
            ),
            (
                "paper3_polyglot_systems".into(),
                self.paper3.to_json_value(),
            ),
        ]))
    }

    /// Renders a GitHub-Flavored Markdown empirical benchmark report.
    pub fn to_markdown(&self) -> String {
        let down_chain = self
            .governor
            .step_down_scales
            .iter()
            .map(|s| format!("{s:.2}"))
            .collect::<Vec<_>>()
            .join(" -> ");
        let up_chain = self
            .governor
            .step_up_scales
            .iter()
            .map(|s| format!("{s:.2}"))
            .collect::<Vec<_>>()
            .join(" -> ");

        let mut out = String::with_capacity(3072);
        out.push_str("# Ganbate Engine — Reproducible Empirical Benchmark Report\n\n");
        out.push_str(&format!(
            "- **Environment:** `{}/{}` | **Iterations (`N`):** `{}` | **Elapsed:** `{:.2} ms` | **SHA-256 NIST Self-Test:** `{}`\n",
            self.os,
            self.arch,
            self.iterations,
            self.total_elapsed_ms,
            if self.sha256_self_test_verified { "PASS" } else { "FAIL" }
        ));
        out.push_str(&format!(
            "- **Token Estimation Heuristic:** `{}`\n\n",
            self.compression.token_heuristic
        ));

        out.push_str(
            "## 1. Neuro-Symbolic Context Compression (`ganbate://context` vs. Raw Codebase)\n\n",
        );
        out.push_str("| Metric | Measured Value |\n");
        out.push_str("| :--- | ---: |\n");
        out.push_str(&format!(
            "| Scanned Source Files | `{}` |\n",
            self.compression.source_files
        ));
        out.push_str(&format!(
            "| Raw Codebase Bytes | `{} B` |\n",
            self.compression.raw_bytes
        ));
        out.push_str(&format!(
            "| Estimated Raw Tokens | `{} tok` |\n",
            self.compression.estimated_raw_tokens
        ));
        out.push_str(&format!(
            "| Semantic World Graph Bytes (`{}`) | `{} B` |\n",
            self.compression.graph_source, self.compression.graph_bytes
        ));
        out.push_str(&format!(
            "| Estimated World Graph Tokens | `{} tok` |\n",
            self.compression.estimated_graph_tokens
        ));
        out.push_str(&format!(
            "| **Context Compression Ratio** | **`{:.2}x` (`{:.3}%` reduction)** |\n\n",
            self.compression.compression_ratio, self.compression.token_reduction_pct
        ));

        out.push_str("## 2. Tri-Modal State Isomorphism & Invertibility Proof\n\n");
        out.push_str("| Modality / Transition | Canonical SHA-256 Digest |\n");
        out.push_str("| :--- | :--- |\n");
        out.push_str(&format!(
            "| Baseline Graph `G_0` | `{}` |\n",
            self.isomorphism.sha256_baseline
        ));
        out.push_str(&format!(
            "| Path A — Direct Code JSON | `{}` |\n",
            self.isomorphism.sha256_code
        ));
        out.push_str(&format!(
            "| Path B — Visual UI Patch | `{}` |\n",
            self.isomorphism.sha256_ui
        ));
        out.push_str(&format!(
            "| Path C.1 — Natural-Language Prompt (EN) | `{}` |\n",
            self.isomorphism.sha256_prompt_en
        ));
        out.push_str(&format!(
            "| Path C.2 — Natural-Language Prompt (KO) | `{}` |\n",
            self.isomorphism.sha256_prompt_ko
        ));
        out.push_str(&format!(
            "| Inverse Patch `P^-1(P(G_0))` | `{}` |\n\n",
            self.isomorphism.sha256_inverted
        ));
        out.push_str(&format!(
            "- **Tri-Modal Isomorphism (`Code == UI == EN == KO`):** `{}`\n",
            self.isomorphism.isomorphism_verified
        ));
        out.push_str(&format!(
            "- **Exact Invertibility (`P^-1(P(G_0)) == G_0`):** `{}` (`{}` byte drift)\n\n",
            self.isomorphism.invertibility_verified, self.isomorphism.byte_drift
        ));

        out.push_str("## 3. Adversarial Authority & `120Hz` Frame-Budget Guardrails\n\n");
        out.push_str(&format!(
            "- **Server-Authority Rejection Rate:** `{}/{}` (`{:.1}%`, `{}/{}` zero state mutation)\n",
            self.guardrails.authority_rejected_cases,
            self.guardrails.authority_total_cases,
            self.guardrails.authority_rejection_rate * 100.0,
            self.guardrails.authority_zero_mutation_cases,
            self.guardrails.authority_total_cases,
        ));
        out.push_str(&format!(
            "- **`120Hz` (`8.33ms`) Budget Clamp Rate:** `{}/{}` (`{:.1}%`)\n\n",
            self.guardrails.budget_clamped_cases,
            self.guardrails.budget_total_cases,
            self.guardrails.budget_clamp_rate * 100.0,
        ));

        out.push_str(&format!(
            "## 4. Microsecond Systems Telemetry (`N = {}`)\n\n",
            self.iterations
        ));
        out.push_str(
            "| Subsystem Operation | Min (µs) | p50 (µs) | p95 (µs) | p99 (µs) | Max (µs) |\n",
        );
        out.push_str("| :--- | ---: | ---: | ---: | ---: | ---: |\n");
        for l in &self.latencies {
            out.push_str(&format!(
                "| {} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} |\n",
                l.label, l.min_us, l.p50_us, l.p95_us, l.p99_us, l.max_us
            ));
        }
        out.push('\n');
        out.push_str(&format!(
            "- **Closed-Loop `120Hz` Governor Convergence:** Overload (`{:.1} ms`) `{}` (floor in `{}` frames) | Deadband Hold: `{}` | Headroom (`{:.1} ms`) `{}` (peak in `{}` frames) | Verified: `{}`\n\n",
            self.governor.overload_input_ms,
            down_chain,
            self.governor.step_down_frames_to_floor,
            self.governor.deadband_hold_verified,
            self.governor.headroom_input_ms,
            up_chain,
            self.governor.recovery_frames_to_peak,
            self.governor.closed_loop_verified,
        ));

        // Section 5: Paper 1 (UIST / CHI / SIGGRAPH)
        out.push_str(&format!(
            "## 5. Paper 1 (UIST / CHI / SIGGRAPH) — `.gbs` Relational Spatial DSL, `Document` Algebra & `scene_verify` (`N = {}` Tasks)\n\n",
            self.paper1.total_tasks
        ));
        out.push_str("| Paper 1 Invariant | Measured Result |\n");
        out.push_str("| :--- | ---: |\n");
        out.push_str(&format!(
            "| Authoring Tasks Passed (`execute_gbs` + `verify_scene` + `Ctrl+Z`) | `{}/{}` (`{:.1}%`) |\n",
            self.paper1.tasks_passed,
            self.paper1.total_tasks,
            (self.paper1.tasks_passed as f64 / self.paper1.total_tasks.max(1) as f64) * 100.0
        ));
        out.push_str(&format!(
            "| `Document` `Op::Set`/`Op::Remove` SHA-256 `Ctrl+Z` Recovery | `{}/{}` (`{:.1}%`, `Set`+`Remove`: `{}`) |\n",
            self.paper1.op_ctrl_z_sha256_recovered,
            self.paper1.total_tasks,
            self.paper1.op_ctrl_z_sha256_recovery_rate * 100.0,
            self.paper1.op_set_and_remove_exercised
        ));
        out.push_str(&format!(
            "| Relational `.gbs` Spatial Lint-Free Rate (`snap`/`socket`/`beside`/`ring`) | `{}/{}` (`{:.1}%`) |\n",
            self.paper1.gbs_snapped_lint_free_tasks,
            self.paper1.total_tasks,
            self.paper1.gbs_snapped_spatial_lint_free_rate * 100.0
        ));
        out.push_str(&format!(
            "| Ungrounded Raw-Coordinate Baseline Lint Rate | `{}/{}` linted (`{}` floating, `{}` penetration; `{:.1}%` lint-free) |\n",
            self.paper1.ungrounded_baseline_linted_tasks,
            self.paper1.total_tasks,
            self.paper1.ungrounded_baseline_floating_lints,
            self.paper1.ungrounded_baseline_penetration_lints,
            self.paper1.ungrounded_baseline_lint_free_rate * 100.0
        ));
        out.push_str(&format!(
            "| `.gbs` Compile + Apply Latency (`p50` / `p95` / `p99`) | `{:.2}` / `{:.2}` / `{:.2} µs` |\n",
            self.paper1.gbs_compile_apply_us.p50_us,
            self.paper1.gbs_compile_apply_us.p95_us,
            self.paper1.gbs_compile_apply_us.p99_us
        ));
        out.push_str(&format!(
            "| `scene_verify` Spatial/Budget Oracle Latency (`p50` / `p95` / `max` vs. `<100ms` SLA) | `{:.2}` / `{:.2}` / `{:.2} µs` (`SLA PASS: {}`) |\n\n",
            self.paper1.scene_verify_us.p50_us,
            self.paper1.scene_verify_us.p95_us,
            self.paper1.scene_verify_us.max_us,
            self.paper1.verify_under_100ms_sla
        ));

        // Section 6: Paper 2 (ICSE / FSE / NeurIPS)
        out.push_str(
            "## 6. Paper 2 (ICSE / FSE / NeurIPS) — Zero-Dependency Cargo Audit & Ecosystem Provenance\n\n",
        );
        out.push_str("| Paper 2 Invariant | Measured Result |\n");
        out.push_str("| :--- | ---: |\n");
        out.push_str(&format!(
            "| `ganbate-engine` Workspace Crates | `{}` (`{}` internal path edges) |\n",
            self.paper2.workspace_crates, self.paper2.internal_path_dependencies
        ));
        out.push_str(&format!(
            "| External `crates.io` Dependencies | **`{}`** (`zero_external_deps_verified: {}`) |\n",
            self.paper2.external_crates_io_dependencies, self.paper2.zero_external_deps_verified
        ));
        out.push_str(&format!(
            "| `#![forbid(unsafe_code)]` Crates | `{}` |\n",
            self.paper2.forbid_unsafe_crates
        ));
        if self.paper2.xray_available {
            out.push_str(&format!(
                "| Ecosystem X-Ray (`code-surface-catalog.json`) | `{}` surfaces (`{}` code files, `{}` dependency edges, `{:.0}%` impl / `{:.0}%` integ, `{}` reclaimable LOC) |\n",
                self.paper2.indexed_surfaces,
                self.paper2.code_files,
                self.paper2.dependency_edges,
                self.paper2.avg_implementation_pct,
                self.paper2.avg_integration_pct,
                self.paper2.reclaimable_loc
            ));
        } else {
            out.push_str(
                "| Ecosystem X-Ray (`code-surface-catalog.json`) | `not present in --project` |\n",
            );
        }
        if self.paper2.pyramid_available {
            out.push_str(&format!(
                "| AI Context Pyramid (`ai-context-pyramid.json`) | Health `{:.0}/100` (`{}` clusters, L0 `{} tok`, max L1 `{} tok`, within budget: `{}`) |\n\n",
                self.paper2.health_score,
                self.paper2.cluster_count,
                self.paper2.l0_tokens,
                self.paper2.max_l1_tokens,
                self.paper2.pyramid_within_budget
            ));
        } else {
            out.push_str("| AI Context Pyramid (`ai-context-pyramid.json`) | `not present in --project` |\n\n");
        }

        // Section 7: Paper 3 (EuroSys / ASPLOS / SIGGRAPH / IEEE VR)
        out.push_str(
            "## 7. Paper 3 (EuroSys / ASPLOS / SIGGRAPH / IEEE VR) — 4-Target WGSL Compiler, `.gbew` v4 Cooker & `10,000`-Step Physics\n\n",
        );
        out.push_str("| Shader Backend (`ganbate-shader`) | Vertex (B) | Fragment (B) | Total (B) | Median Compile (µs) | Verified |\n");
        out.push_str("| :--- | ---: | ---: | ---: | ---: | :---: |\n");
        for t in &self.paper3.shader_targets {
            out.push_str(&format!(
                "| {} | `{}` | `{}` | `{}` | `{:.2}` | `{}` |\n",
                t.label, t.vertex_bytes, t.fragment_bytes, t.total_bytes, t.compile_us, t.verified
            ));
        }
        out.push('\n');
        out.push_str(&format!(
            "- **`.gbew` v{} Binary Mesh Cooker (`ganbate-assets`):** `{}` verts / `{}` tris -> Raw `{} B` vs. Cooked **`{} B`** (**`{:.2} B/triangle`**, `{:.2}x` compression, max grid error `{:.3} mm`, cook `{:.2} µs`, decode `{:.2} µs`, round-trip verified: `{}`)\n",
            self.paper3.gbew.version,
            self.paper3.gbew.vertex_count,
            self.paper3.gbew.triangle_count,
            self.paper3.gbew.raw_mesh_bytes,
            self.paper3.gbew.cooked_gbew_bytes,
            self.paper3.gbew.bytes_per_triangle,
            self.paper3.gbew.compression_ratio,
            self.paper3.gbew.max_position_error_mm,
            self.paper3.gbew.cook_us,
            self.paper3.gbew.decode_us,
            self.paper3.gbew.roundtrip_verified,
        ));
        out.push_str(&format!(
            "- **`{}`-Step `#![forbid(unsafe_code)]` Physics Determinism (`ganbate-physics`):** Run A `{}` == Run B `{}` (initial `{}`, `{:.2} µs/step`, `0` `NaN`: `{}`, bit-exact: `{}`)\n",
            self.paper3.physics.steps,
            self.paper3.physics.hash_run_a_hex,
            self.paper3.physics.hash_run_b_hex,
            self.paper3.physics.hash_initial_hex,
            self.paper3.physics.mean_us_per_step,
            self.paper3.physics.zero_nan_verified,
            self.paper3.physics.bit_exact_verified,
        ));
        out.push_str(&format!(
            "- **`90Hz VR` (`{:.2} ms`) Closed-Loop Frame Governor:** Floor in `{}` frames, recovery in `{}` frames, verified: `{}`",
            self.paper3.vr_90hz_target_ms,
            self.paper3.vr_90hz_step_down_frames,
            self.paper3.vr_90hz_recovery_frames,
            self.paper3.vr_90hz_governor_verified,
        ));
        out
    }

    /// Renders ready-to-compile academic `booktabs` LaTeX tables covering all 3 PhD proposal papers.
    pub fn to_latex(&self) -> String {
        let escape_tex = |s: &str| -> String {
            s.replace('\\', "\\textbackslash{}")
                .replace('&', "\\&")
                .replace('%', "\\%")
                .replace('_', "\\_")
                .replace('#', "\\#")
        };

        let down_tex = self
            .governor
            .step_down_scales
            .iter()
            .map(|s| format!("{s:.2}"))
            .collect::<Vec<_>>()
            .join(" \\to ");

        let paper2_prov_tex = if self.paper2.xray_available && self.paper2.pyramid_available {
            format!(
                "{} crates (0 ext) \\;\\|\\; {} surf, {} edges, {:.0}/100 health",
                self.paper2.workspace_crates,
                self.paper2.indexed_surfaces,
                self.paper2.dependency_edges,
                self.paper2.health_score
            )
        } else if self.paper2.xray_available {
            format!(
                "{} crates (0 ext) \\;\\|\\; {} surf, {} edges",
                self.paper2.workspace_crates,
                self.paper2.indexed_surfaces,
                self.paper2.dependency_edges
            )
        } else {
            format!(
                "{} crates, 0 external \\texttt{{crates.io}} deps ({} \\texttt{{forbid(unsafe)}})",
                self.paper2.workspace_crates, self.paper2.forbid_unsafe_crates
            )
        };

        let mut out = String::with_capacity(4096);
        out.push_str("\\begin{table}[t]\n");
        out.push_str("\\centering\n");
        out.push_str("\\small\n");
        out.push_str(&format!(
            "\\caption{{Three-paper empirical verification and microsecond latency distribution of \\texttt{{ganbate-engine}} ({}/{}, $N = {}$ iterations; total runtime {:.1}\\,ms; BPE token heuristic $\\lceil \\text{{bytes}}/4 \\rceil$).}}\n",
            escape_tex(&self.os),
            escape_tex(&self.arch),
            self.iterations,
            self.total_elapsed_ms
        ));
        out.push_str("\\label{tab:ganbate-empirical-bench}\n");
        out.push_str("\\begin{tabular}{lrrrr}\n");
        out.push_str("\\toprule\n");
        out.push_str(
            "Empirical Verification Pillar & \\multicolumn{4}{r}{Measured Invariant} \\\\\n",
        );
        out.push_str("\\midrule\n");
        out.push_str(&format!(
            "P1: Context Compression ({} files) & \\multicolumn{{4}}{{r}}{{${:.2}\\times$ ({} $\\to$ {} tok, {:.3}\\% reduction)}} \\\\\n",
            self.compression.source_files,
            self.compression.compression_ratio,
            self.compression.estimated_raw_tokens,
            self.compression.estimated_graph_tokens,
            self.compression.token_reduction_pct
        ));
        out.push_str(&format!(
            "P1: Tri-Modal Isomorphism \\& $P^{{-1}}$ Invertibility & \\multicolumn{{4}}{{r}}{{{} ({} byte drift)}} \\\\\n",
            if self.isomorphism.isomorphism_verified && self.isomorphism.invertibility_verified {
                "Verified ($\\text{Code} = \\text{UI} = \\text{EN} = \\text{KO}$)"
            } else {
                "Failed"
            },
            self.isomorphism.byte_drift
        ));
        out.push_str(&format!(
            "P1: \\texttt{{.gbs}} Spatial Suite ($N={}$) \\& \\texttt{{Ctrl+Z}} & \\multicolumn{{4}}{{r}}{{{}/{} lint-free ({:.0}\\% vs.\\ {:.0}\\% raw) \\;\\|\\; {}/{} SHA-256}} \\\\\n",
            self.paper1.total_tasks,
            self.paper1.gbs_snapped_lint_free_tasks,
            self.paper1.total_tasks,
            self.paper1.gbs_snapped_spatial_lint_free_rate * 100.0,
            self.paper1.ungrounded_baseline_lint_free_rate * 100.0,
            self.paper1.op_ctrl_z_sha256_recovered,
            self.paper1.total_tasks,
        ));
        out.push_str(&format!(
            "P2: Zero-Dep Cargo \\& Ecosystem Provenance & \\multicolumn{{4}}{{r}}{{{}}} \\\\\n",
            paper2_prov_tex
        ));
        out.push_str(&format!(
            "P2: Server-Authority \\& 120\\,Hz Guardrails & \\multicolumn{{4}}{{r}}{{{:.1}\\% reject ({}/{}) \\;\\|\\; {:.1}\\% clamp ({}/{})}} \\\\\n",
            self.guardrails.authority_rejection_rate * 100.0,
            self.guardrails.authority_rejected_cases,
            self.guardrails.authority_total_cases,
            self.guardrails.budget_clamp_rate * 100.0,
            self.guardrails.budget_clamped_cases,
            self.guardrails.budget_total_cases,
        ));
        out.push_str(&format!(
            "P3: 4-Target RHI, \\texttt{{.gbew}} v{} \\& 10k-Step Physics & \\multicolumn{{4}}{{r}}{{{}/4 RHI \\;\\|\\; {:.2}\\,B/tri ({:.2}$\\times$) \\;\\|\\; \\texttt{{{}}} ({:.2}\\,$\\mu$s/step)}} \\\\\n",
            self.paper3.gbew.version,
            self.paper3.shader_targets.iter().filter(|t| t.verified).count(),
            self.paper3.gbew.bytes_per_triangle,
            self.paper3.gbew.compression_ratio,
            escape_tex(&self.paper3.physics.hash_run_a_hex[..10.min(self.paper3.physics.hash_run_a_hex.len())]),
            self.paper3.physics.mean_us_per_step,
        ));
        out.push_str(&format!(
            "P3: 120\\,Hz / 90\\,Hz VR Closed-Loop Governors & \\multicolumn{{4}}{{r}}{{${}$ ({}f/{}f) \\;\\|\\; 90\\,Hz VR ({}f/{}f)}} \\\\\n",
            down_tex,
            self.governor.step_down_frames_to_floor,
            self.governor.recovery_frames_to_peak,
            self.paper3.vr_90hz_step_down_frames,
            self.paper3.vr_90hz_recovery_frames,
        ));
        out.push_str("\\midrule\n");
        out.push_str("Subsystem Operation & Min ($\\mu$s) & p50 ($\\mu$s) & p95 ($\\mu$s) & p99 ($\\mu$s) \\\\\n");
        out.push_str("\\midrule\n");
        for l in &self.latencies {
            out.push_str(&format!(
                "{} & {:.2} & {:.2} & {:.2} & {:.2} \\\\\n",
                escape_tex(&l.label),
                l.min_us,
                l.p50_us,
                l.p95_us,
                l.p99_us
            ));
        }
        for l in [
            &self.paper1.gbs_compile_apply_us,
            &self.paper1.scene_verify_us,
        ] {
            out.push_str(&format!(
                "{} & {:.2} & {:.2} & {:.2} & {:.2} \\\\\n",
                escape_tex(&l.label),
                l.min_us,
                l.p50_us,
                l.p95_us,
                l.p99_us
            ));
        }
        out.push_str("\\bottomrule\n");
        out.push_str("\\end{tabular}\n");
        out.push_str("\\end{table}\n\n");

        // Second companion booktabs table: Paper 3 Polyglot RHI & Binary Asset Systems Breakdown
        out.push_str("\\begin{table}[t]\n");
        out.push_str("\\centering\n");
        out.push_str("\\small\n");
        out.push_str(&format!(
            "\\caption{{Paper~3 zero-dependency 4-target WGSL shader toolchain, \\texttt{{.gbew}} v{} binary mesh cooker ({} triangles), and {}-step bit-exact physics verification.}}\n",
            self.paper3.gbew.version,
            self.paper3.gbew.triangle_count,
            self.paper3.physics.steps
        ));
        out.push_str("\\label{tab:ganbate-polyglot-systems}\n");
        out.push_str("\\begin{tabular}{lrrrr}\n");
        out.push_str("\\toprule\n");
        out.push_str(
            "Target / Artifact & Vertex (B) & Fragment (B) & Total (B) & Median ($\\mu$s) \\\\\n",
        );
        out.push_str("\\midrule\n");
        for t in &self.paper3.shader_targets {
            out.push_str(&format!(
                "{} & {} & {} & {} & {:.2} \\\\\n",
                escape_tex(&t.label),
                t.vertex_bytes,
                t.fragment_bytes,
                t.total_bytes,
                t.compile_us
            ));
        }
        out.push_str("\\midrule\n");
        out.push_str(&format!(
            "\\texttt{{.gbew}} v{} Cooked Torus ({}v / {}t) & \\multicolumn{{2}}{{r}}{{Raw: {}\\,B $\\to$ Cooked: {}\\,B}} & {:.2}\\,B/tri & {:.2} \\\\\n",
            self.paper3.gbew.version,
            self.paper3.gbew.vertex_count,
            self.paper3.gbew.triangle_count,
            self.paper3.gbew.raw_mesh_bytes,
            self.paper3.gbew.cooked_gbew_bytes,
            self.paper3.gbew.bytes_per_triangle,
            self.paper3.gbew.cook_us,
        ));
        out.push_str(&format!(
            "\\texttt{{ganbate-physics}} ({} steps, 60\\,Hz) & \\multicolumn{{3}}{{r}}{{\\texttt{{{}}} $=$ \\texttt{{{}}} (0 NaN)}} & {:.2}/step \\\\\n",
            self.paper3.physics.steps,
            escape_tex(&self.paper3.physics.hash_run_a_hex),
            escape_tex(&self.paper3.physics.hash_run_b_hex),
            self.paper3.physics.mean_us_per_step,
        ));
        out.push_str("\\bottomrule\n");
        out.push_str("\\end{tabular}\n");
        out.push_str("\\end{table}");
        out
    }

    /// Formats the benchmark report in the requested [`BenchFormat`].
    pub fn render(&self, format: BenchFormat, pretty_json: bool) -> Result<String, String> {
        match format {
            BenchFormat::Json => {
                let val = self.to_json_value();
                if pretty_json {
                    let mut s = json::to_canonical(&val)?;
                    if s.ends_with('\n') {
                        s.pop();
                    }
                    Ok(s)
                } else {
                    json::to_compact(&val)
                }
            }
            BenchFormat::Latex => Ok(self.to_latex()),
            BenchFormat::Markdown => Ok(self.to_markdown()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ganbate_core::id::Ids;

    struct TempDir(std::path::PathBuf);
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn benchmark_suite_verifies_compression_isomorphism_guardrails_and_formats_readonly() {
        let dir = TempDir(
            std::env::temp_dir().join(format!("ganbate-bench-{}", Ids::from_os().entity())),
        );
        fs::create_dir_all(dir.0.join("src")).unwrap();
        fs::create_dir_all(dir.0.join("node_modules/ignored_pkg")).unwrap();
        fs::write(
            dir.0.join("src/sample.rs"),
            "pub fn compute_lighting(dt: f32) -> f32 {\n    dt * 120.0\n}\n".repeat(40),
        )
        .unwrap();
        fs::write(
            dir.0.join("node_modules/ignored_pkg/index.js"),
            "console.log('ignored');".repeat(200),
        )
        .unwrap();

        let rep = run_benchmark(&dir.0, 32).expect("run benchmark");
        assert!(rep.sha256_self_test_verified);
        // Must remain strictly read-only (never creates world.semantic.json in target project).
        assert!(!dir.0.join(WORLD_GRAPH_REL_PATH).exists());

        assert_eq!(rep.compression.source_files, 1);
        assert!(rep.compression.raw_bytes > rep.compression.graph_bytes);
        assert!(rep.compression.compression_ratio > 1.0);

        assert!(rep.isomorphism.isomorphism_verified);
        assert_eq!(rep.isomorphism.sha256_code, rep.isomorphism.sha256_ui);
        assert_eq!(rep.isomorphism.sha256_ui, rep.isomorphism.sha256_prompt_en);
        assert_eq!(
            rep.isomorphism.sha256_prompt_en,
            rep.isomorphism.sha256_prompt_ko
        );
        assert!(rep.isomorphism.invertibility_verified);
        assert_eq!(rep.isomorphism.byte_drift, 0);

        assert!(rep.guardrails.authority_total_cases >= 12);
        assert_eq!(
            rep.guardrails.authority_rejected_cases,
            rep.guardrails.authority_total_cases
        );
        assert!((rep.guardrails.authority_rejection_rate - 1.0).abs() < 1e-9);
        assert!(rep.guardrails.budget_total_cases >= 6);
        assert_eq!(
            rep.guardrails.budget_clamped_cases,
            rep.guardrails.budget_total_cases
        );
        assert!((rep.guardrails.budget_clamp_rate - 1.0).abs() < 1e-9);

        assert_eq!(rep.latencies.len(), 5);
        for lat in &rep.latencies {
            assert_eq!(lat.iterations, 32);
            assert!(lat.min_us <= lat.p50_us);
            assert!(lat.p50_us <= lat.p95_us);
            assert!(lat.p95_us <= lat.p99_us);
            assert!(lat.p99_us <= lat.max_us);
        }

        assert!(rep.governor.closed_loop_verified);
        assert!(rep.governor.deadband_hold_verified);
        assert!(rep.governor.step_down_frames_to_floor > 0);
        assert!(rep.governor.recovery_frames_to_peak > 0);

        // Paper 1: 30/30 .gbs spatial tasks, 30/30 SHA-256 Ctrl+Z recovery, 100% lint-free vs. 0% raw
        assert!(rep.paper1.verified(), "paper1 failed: {:?}", rep.paper1);
        assert_eq!(rep.paper1.total_tasks, 30);
        assert_eq!(rep.paper1.tasks_passed, 30);
        assert_eq!(rep.paper1.op_ctrl_z_sha256_recovered, 30);
        assert_eq!(rep.paper1.gbs_snapped_lint_free_tasks, 30);
        assert_eq!(rep.paper1.ungrounded_baseline_linted_tasks, 30);
        assert!(rep.paper1.verify_under_100ms_sla);

        // Paper 2: 14 workspace crates & 0 external crates.io dependencies
        assert!(rep.paper2.verified(), "paper2 failed: {:?}", rep.paper2);
        assert_eq!(rep.paper2.workspace_crates, 14);
        assert_eq!(rep.paper2.external_crates_io_dependencies, 0);
        assert!(!rep.paper2.xray_available);

        // Paper 3: 4 shader targets, .gbew v4 ~7 B/tri, 10,000-step bit-exact physics
        assert!(rep.paper3.verified(), "paper3 failed: {:?}", rep.paper3);
        assert_eq!(rep.paper3.shader_targets.len(), 4);
        assert!(rep.paper3.gbew.triangle_count >= 2_048);
        assert!(
            rep.paper3.gbew.bytes_per_triangle > 3.0 && rep.paper3.gbew.bytes_per_triangle < 12.0,
            "unexpected B/tri: {}",
            rep.paper3.gbew.bytes_per_triangle
        );
        assert_eq!(
            rep.paper3.physics.hash_run_a_hex,
            rep.paper3.physics.hash_run_b_hex
        );
        assert_ne!(
            rep.paper3.physics.hash_run_a_hex,
            rep.paper3.physics.hash_initial_hex
        );

        let json_out = rep.render(BenchFormat::Json, false).unwrap();
        let parsed = json::parse(json_out.as_bytes()).unwrap();
        assert_eq!(parsed.get("ok"), Some(&Value::Bool(true)));
        assert_eq!(
            parsed
                .get("paper1_spatial_suite")
                .and_then(|p| p.get("verified")),
            Some(&Value::Bool(true))
        );
        assert_eq!(
            parsed
                .get("paper2_ecosystem_governance")
                .and_then(|p| p.get("external_crates_io_dependencies")),
            Some(&Value::Number(0.0))
        );
        assert_eq!(
            parsed
                .get("paper3_polyglot_systems")
                .and_then(|p| p.get("verified")),
            Some(&Value::Bool(true))
        );

        let latex_out = rep.render(BenchFormat::Latex, false).unwrap();
        assert!(latex_out.contains("\\begin{table}[t]"));
        assert!(latex_out.contains("\\label{tab:ganbate-empirical-bench}"));
        assert!(latex_out.contains("\\label{tab:ganbate-polyglot-systems}"));
        assert!(latex_out.contains("\\begin{tabular}{lrrrr}"));
        assert!(latex_out.contains("\\toprule"));
        assert!(latex_out.contains("\\bottomrule"));

        let md_out = rep.render(BenchFormat::Markdown, false).unwrap();
        assert!(md_out.contains("# Ganbate Engine — Reproducible Empirical Benchmark Report"));
        assert!(md_out.contains("Tri-Modal State Isomorphism"));
        assert!(md_out.contains("Paper 1 (UIST / CHI / SIGGRAPH)"));
        assert!(md_out.contains("Paper 2 (ICSE / FSE / NeurIPS)"));
        assert!(md_out.contains("Paper 3 (EuroSys / ASPLOS / SIGGRAPH / IEEE VR)"));
    }

    #[test]
    fn paper2_extracts_xray_catalog_and_context_pyramid_provenance_readonly() {
        let dir = TempDir(
            std::env::temp_dir().join(format!("ganbate-p2-prov-{}", Ids::from_os().entity())),
        );
        let studio_pub = dir.0.join("main/apps/web/public/ganbate-studio");
        fs::create_dir_all(&studio_pub).unwrap();
        fs::write(
            studio_pub.join("code-surface-catalog.json"),
            r#"{"schema":"ganbate.code-xray-catalog/2","summary":{"totalItems":3518,"codeFileCount":3159,"totalDependencyEdges":14625,"avgImplementationPct":74,"avgIntegrationPct":83,"reclaimableLoc":98154},"clusters":[]}"#,
        )
        .unwrap();
        fs::write(
            studio_pub.join("ai-context-pyramid.json"),
            r#"{"schema":"ganbate.ai-context-pyramid/1","budgets":{"actualL0Tokens":456,"maxActualL1Tokens":169,"withinBudget":true},"clusters":[{"id":"c1"},{"id":"c2"}],"optimizationMonitor":{"workspaceHealthSummary":{"healthScore":82}}}"#,
        )
        .unwrap();

        let p2 = run_paper2_ecosystem_governance(&dir.0).expect("paper2 provenance");
        assert!(p2.verified());
        assert_eq!(p2.workspace_crates, 14);
        assert_eq!(p2.external_crates_io_dependencies, 0);
        assert!(p2.xray_available);
        assert_eq!(p2.indexed_surfaces, 3518);
        assert_eq!(p2.code_files, 3159);
        assert_eq!(p2.dependency_edges, 14625);
        assert_eq!(p2.reclaimable_loc, 98154);
        assert!(p2.pyramid_available);
        assert!((p2.health_score - 82.0).abs() < 1e-6);
        assert_eq!(p2.cluster_count, 2);
        assert_eq!(p2.l0_tokens, 456);
        assert_eq!(p2.max_l1_tokens, 169);
        assert!(p2.pyramid_within_budget);
    }
}
