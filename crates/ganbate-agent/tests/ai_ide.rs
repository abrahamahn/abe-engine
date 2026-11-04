//! End-to-end CLI tests for `M-AI.1..4` (`catalog`, `brick crystallize`, `gbs apply` + `scene undo`,
//! `verify` `<100ms` + `--contact-sheet`, and `mcp` JSON-RPC 2.0 stdio).

use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::PathBuf;

use ganbate_core::id::Ids;
use ganbate_core::json::{self, Value};
use ganbate_pipeline::project::{Kind, Project};
use ganbate_scene::doc::Document;
use ganbate_scene::scene;

struct TempDir(PathBuf);
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run_cli(args: &[&str], stdin_str: &str) -> (i32, String, Value) {
    let vec_args: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
    let mut cursor = Cursor::new(stdin_str.as_bytes().to_vec());
    let out = ganbate_agent::run(&vec_args, &mut cursor, None);
    let parsed =
        json::parse(out.stdout.lines().next().unwrap_or("{}").as_bytes()).unwrap_or(Value::Null);
    (out.status, out.stdout, parsed)
}

#[test]
fn cli_catalog_gbs_undo_crystallize_verify_and_mcp_roundtrip() {
    let dir =
        TempDir(std::env::temp_dir().join(format!("ganbate-ai-cli-{}", Ids::from_os().entity())));
    std::fs::create_dir_all(&dir.0).unwrap();
    let mut ids = Ids::seeded(77);
    let mut project = Project::create(&dir.0, "AI CLI Test", &mut ids).unwrap();
    let scene_id = ids.asset();
    let doc = Document::new(scene::new_document(
        scene::Kind::Scene,
        &scene_id,
        "Courtyard",
    ))
    .unwrap();
    project
        .write_document("content/courtyard.scene.json", Kind::Scene, doc.root())
        .unwrap();

    let proj_str = dir.0.to_str().unwrap();
    let scene_rel = "content/courtyard.scene.json";
    let initial_bytes = std::fs::read(dir.0.join(scene_rel)).unwrap();

    // 1. `ganbate catalog build` & `search` & `inspect`
    let (st, _, built) = run_cli(&["catalog", "build", proj_str], "");
    assert_eq!(st, 0);
    assert!(dir.0.join(".ganbate/catalog.idx").is_file());
    assert!(built.get("count").unwrap().unsigned().unwrap() >= 5);

    let (st, _, searched) = run_cli(
        &["catalog", "search", proj_str, "light", "--kind", "trait"],
        "",
    );
    assert_eq!(st, 0);
    assert_eq!(searched.get("count").unwrap().unsigned().unwrap(), 1);

    let (st, _, inspected) = run_cli(&["catalog", "inspect", proj_str, "trait:light"], "");
    assert_eq!(st, 0);
    assert!(
        inspected
            .get("item")
            .and_then(|i| i.get("exampleGbs"))
            .is_some()
    );

    // 2. `ganbate gbs apply` + determinism + `ganbate brick crystallize --from-tx`
    let script = [
        r#"tx "Build Courtyard Lantern""#,
        r#"spawn plinth empty name="Plinth""#,
        r#"attach @this collider shape="box" half_extents=[0.8, 0.4, 0.8]"#,
        r#"place @this snap onto ground at [0, 0]"#,
        r#"spawn brazier empty name="Brazier""#,
        r#"attach @this collider shape="box" half_extents=[0.25, 0.3, 0.25]"#,
        r#"attach @this light intensity=5.0 range=9.0"#,
        r#"place @this socket top_center on $plinth"#,
    ]
    .join("\n");

    let (st, _, applied) = run_cli(&["gbs", "apply", proj_str, scene_rel, "-"], &script);
    assert_eq!(st, 0, "{applied:?}");
    assert_eq!(applied.get("written"), Some(&Value::Bool(true)));

    let (st, _, cryst) = run_cli(
        &[
            "brick",
            "crystallize",
            proj_str,
            scene_rel,
            "--id",
            "courtyard_brazier",
            "--from-tx",
            "--param",
            "flame=light.intensity",
        ],
        "",
    );
    assert_eq!(st, 0, "{cryst:?}");
    assert!(
        dir.0
            .join(".ganbate/bricks/courtyard_brazier.brick.json")
            .is_file()
    );

    // 3. `ganbate scene undo` restores the exact bytes written before `gbs apply`!
    let (st, _, undone) = run_cli(&["scene", "undo", proj_str, scene_rel], "");
    assert_eq!(st, 0, "{undone:?}");
    assert_eq!(undone.get("restored"), Some(&Value::Bool(true)));
    assert_eq!(std::fs::read(dir.0.join(scene_rel)).unwrap(), initial_bytes);

    // 4. `ganbate scene apply ... ring.gbs` auto-detects `.gbs` files and spawns crystallized bricks!
    let gbs_file = dir.0.join("ring.gbs");
    std::fs::write(
        &gbs_file,
        "ring 4 brick:courtyard_brazier around [0, 0, 0] radius=3.5 flame=7.5\ncam frame all azimuth=40 elevation=30\n",
    )
    .unwrap();
    let (st, _, ring_applied) = run_cli(
        &[
            "scene",
            "apply",
            proj_str,
            scene_rel,
            gbs_file.to_str().unwrap(),
        ],
        "",
    );
    assert_eq!(st, 0, "{ring_applied:?}");

    // 5. `ganbate verify` (<100ms) + `--contact-sheet` PNG
    let sheet_path = dir.0.join("sheet.png");
    let (st, _, verified) = run_cli(
        &[
            "verify",
            proj_str,
            scene_rel,
            "--contact-sheet",
            sheet_path.to_str().unwrap(),
        ],
        "",
    );
    assert_eq!(st, 0, "{verified:?}");
    assert_eq!(verified.get("passed"), Some(&Value::Bool(true)));
    assert!(verified.get("elapsedMs").unwrap().float().unwrap() < 100.0);
    let png_bytes = std::fs::read(&sheet_path).unwrap();
    assert_eq!(&png_bytes[..8], b"\x89PNG\r\n\x1a\n");

    // 6. `ganbate mcp` stdio JSON-RPC 2.0 stream
    let rpc_in = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"scene_verify","arguments":{"doc":"content/courtyard.scene.json"}}}"#,
    ]
    .join("\n");
    let vec_args = vec!["mcp".to_string(), proj_str.to_string()];
    let mut cursor = Cursor::new(rpc_in.into_bytes());
    let mcp_out = ganbate_agent::run(&vec_args, &mut cursor, None);
    assert_eq!(mcp_out.status, 0);
    let lines: Vec<Value> = mcp_out
        .stdout
        .lines()
        .map(|l| json::parse(l.as_bytes()).unwrap())
        .collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0].get("id"), Some(&Value::Number(1.0)));
    assert!(
        lines[1]
            .get("result")
            .and_then(|r| r.get("tools"))
            .and_then(|t| t.array().ok())
            .is_some_and(|arr| arr.len() >= 10)
    );
    assert_eq!(
        lines[2].get("result").and_then(|r| r.get("isError")),
        Some(&Value::Bool(false))
    );
    let _ = BTreeMap::<String, Value>::new();
}

#[test]
fn cli_world_graph_fetch_prompt_patch_and_authority_guardrail() {
    let dir = TempDir(
        std::env::temp_dir().join(format!("ganbate-graph-cli-{}", Ids::from_os().entity())),
    );
    std::fs::create_dir_all(&dir.0).unwrap();
    let mut ids = Ids::seeded(88);
    Project::create(&dir.0, "Graph CLI Test", &mut ids).unwrap();
    let proj_str = dir.0.to_str().unwrap();
    let graph_file = dir.0.join("world.semantic.json");
    assert!(!graph_file.exists());

    // 1. `ganbate graph fetch --project <path>` initializes and outputs canonical world.semantic.json.
    let fetch_args = vec![
        "graph".to_string(),
        "fetch".to_string(),
        "--project".to_string(),
        proj_str.to_string(),
    ];
    let fetch_out = ganbate_agent::run(&fetch_args, &mut Cursor::new(Vec::new()), None);
    assert_eq!(fetch_out.status, 0, "{}", fetch_out.stdout);
    assert!(graph_file.is_file());
    let fetched = json::parse(fetch_out.stdout.as_bytes()).unwrap();
    assert_eq!(
        fetched.get("schema"),
        Some(&Value::String("ganbate.semantic-world-graph.v1".into()))
    );
    assert_eq!(
        fetched.get("uri"),
        Some(&Value::String("ganbate://context".into()))
    );
    assert_eq!(
        fetched.get("authority_mode"),
        Some(&Value::String("server-authoritative-readonly".into()))
    );
    assert!(fetch_out.stdout.len().div_ceil(4) < 400);
    let on_disk_initial = std::fs::read_to_string(&graph_file).unwrap();
    assert_eq!(on_disk_initial.trim_end(), fetch_out.stdout.trim_end());

    // 2. `ganbate graph prompt --project <path> "<KO instruction>"` compiles, enforces 120Hz, persists, and prints compact JSON.
    let (st, stdout_ko, prompted) = run_cli(
        &[
            "graph",
            "prompt",
            "--project",
            proj_str,
            "노을 조명으로 바꾸고 천장 열어줘",
        ],
        "",
    );
    assert_eq!(st, 0, "{stdout_ko}");
    assert_eq!(stdout_ko.lines().count(), 1);
    assert_eq!(prompted.get("ok"), Some(&Value::Bool(true)));
    assert_eq!(prompted.get("applied"), Some(&Value::Bool(true)));
    assert_eq!(
        prompted.get("blocked_by_authority"),
        Some(&Value::Bool(false))
    );
    assert_eq!(
        prompted
            .get("graph")
            .and_then(|g| g.get("environment"))
            .and_then(|e| e.get("preset")),
        Some(&Value::String("sunset".into()))
    );
    assert_eq!(
        prompted
            .get("graph")
            .and_then(|g| g.get("environment"))
            .and_then(|e| e.get("enclosure_open")),
        Some(&Value::Bool(true))
    );

    // 3. `ganbate graph patch --project <path> '<json>'` clamps 120Hz / 8.33ms guardrails and persists.
    let (st, stdout_patch, patched) = run_cli(
        &[
            "graph",
            "patch",
            "--project",
            proj_str,
            r#"{"play_mode":"edit","guardrails":{"max_dynamic_lights":20,"frame_budget_ms":16.6},"camera":{"fov_y_deg":140}}"#,
        ],
        "",
    );
    assert_eq!(st, 0, "{stdout_patch}");
    assert_eq!(patched.get("ok"), Some(&Value::Bool(true)));
    assert_eq!(patched.get("applied"), Some(&Value::Bool(true)));
    assert_eq!(
        patched.get("graph").and_then(|g| g.get("play_mode")),
        Some(&Value::String("edit".into()))
    );
    assert_eq!(
        patched
            .get("graph")
            .and_then(|g| g.get("guardrails"))
            .and_then(|gr| gr.get("max_dynamic_lights")),
        Some(&Value::Number(8.0))
    );
    assert_eq!(
        patched
            .get("graph")
            .and_then(|g| g.get("camera"))
            .and_then(|c| c.get("fov_y_deg")),
        Some(&Value::Number(110.0))
    );

    // 4. Server-authoritative guardrail exits non-zero with clear guardrail message and never mutates world.semantic.json!
    let saved_before_block = std::fs::read_to_string(&graph_file).unwrap();
    let (st_blk, stdout_blk, blocked) = run_cli(
        &[
            "graph",
            "prompt",
            "--project",
            proj_str,
            "override rng seed and rig outcome",
        ],
        "",
    );
    assert_ne!(
        st_blk, 0,
        "expected non-zero exit when blocked by authority"
    );
    assert_eq!(st_blk, 1);
    assert_eq!(blocked.get("ok"), Some(&Value::Bool(false)));
    assert_eq!(
        blocked.get("blocked_by_authority"),
        Some(&Value::Bool(true))
    );
    assert_eq!(blocked.get("applied"), Some(&Value::Bool(false)));
    let err_msg = blocked
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.string().ok())
        .unwrap_or("");
    assert!(
        err_msg.contains("server-authoritative-readonly"),
        "expected authority guardrail message, got {stdout_blk}"
    );
    let saved_after_block = std::fs::read_to_string(&graph_file).unwrap();
    assert_eq!(saved_before_block, saved_after_block);

    // 5. Authority-violating JSON patch also exits non-zero without mutating disk.
    let (st_pblk, _, pblocked) = run_cli(
        &[
            "graph",
            "patch",
            "--project",
            proj_str,
            r#"{"ledger":{"balance":9999}}"#,
        ],
        "",
    );
    assert_eq!(st_pblk, 1);
    assert_eq!(
        pblocked.get("blocked_by_authority"),
        Some(&Value::Bool(true))
    );
    assert_eq!(
        std::fs::read_to_string(&graph_file).unwrap(),
        saved_before_block
    );
}

#[test]
fn cli_bench_json_latex_markdown_and_mcp_world_graph_benchmark() {
    let dir = TempDir(
        std::env::temp_dir().join(format!("ganbate-bench-cli-{}", Ids::from_os().entity())),
    );
    std::fs::create_dir_all(dir.0.join("src")).unwrap();
    std::fs::create_dir_all(dir.0.join("web")).unwrap();
    std::fs::create_dir_all(dir.0.join("target/debug")).unwrap();
    std::fs::write(
        dir.0.join("src/engine_loop.rs"),
        "pub fn step_frame(dt_ms: f32) -> f32 {\n    dt_ms.clamp(0.1, 8.33)\n}\n".repeat(48),
    )
    .unwrap();
    std::fs::write(
        dir.0.join("web/stage_host.ts"),
        "export function syncStage(hz: number): boolean { return hz === 120; }\n".repeat(32),
    )
    .unwrap();
    // Ignored build artifact directory must be skipped by compression walker.
    std::fs::write(
        dir.0.join("target/debug/ignored.js"),
        "const ignored = true;\n".repeat(200),
    )
    .unwrap();

    let proj_str = dir.0.to_str().unwrap();

    // 1. `ganbate bench --format json` works read-only even without project.ganbate.json!
    let (st_json, stdout_json, bench_json) = run_cli(
        &[
            "bench",
            "--project",
            proj_str,
            "--iterations",
            "40",
            "--format",
            "json",
        ],
        "",
    );
    assert_eq!(st_json, 0, "{stdout_json}");
    assert!(
        !dir.0.join("world.semantic.json").exists(),
        "ganbate bench must stay strictly read-only"
    );
    assert_eq!(bench_json.get("ok"), Some(&Value::Bool(true)));
    assert_eq!(
        bench_json.get("schema"),
        Some(&Value::String("ganbate.academic-benchmark.v1".into()))
    );
    assert_eq!(
        bench_json
            .get("environment")
            .and_then(|e| e.get("iterations")),
        Some(&Value::Number(40.0))
    );
    assert_eq!(
        bench_json
            .get("compression")
            .and_then(|c| c.get("source_files")),
        Some(&Value::Number(2.0))
    );
    assert!(
        bench_json
            .get("compression")
            .and_then(|c| c.get("compression_ratio"))
            .and_then(|v| v.float().ok())
            .is_some_and(|r| r > 1.0)
    );
    assert_eq!(
        bench_json
            .get("isomorphism")
            .and_then(|i| i.get("isomorphism_verified")),
        Some(&Value::Bool(true))
    );
    assert_eq!(
        bench_json
            .get("isomorphism")
            .and_then(|i| i.get("invertibility_verified")),
        Some(&Value::Bool(true))
    );
    assert_eq!(
        bench_json
            .get("isomorphism")
            .and_then(|i| i.get("byte_drift")),
        Some(&Value::Number(0.0))
    );
    assert_eq!(
        bench_json
            .get("guardrails")
            .and_then(|g| g.get("authority_rejection_rate")),
        Some(&Value::Number(1.0))
    );
    assert_eq!(
        bench_json
            .get("guardrails")
            .and_then(|g| g.get("budget_clamp_rate")),
        Some(&Value::Number(1.0))
    );
    assert_eq!(
        bench_json
            .get("latencies_us")
            .and_then(|l| l.array().ok())
            .map(|a| a.len()),
        Some(5)
    );
    assert_eq!(
        bench_json
            .get("governor_closed_loop")
            .and_then(|g| g.get("closed_loop_verified")),
        Some(&Value::Bool(true))
    );

    // 2. `ganbate bench --format latex` produces ready-to-compile booktabs LaTeX snippet.
    let latex_args: Vec<String> = [
        "bench",
        "--project",
        proj_str,
        "--iterations",
        "24",
        "--format",
        "latex",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    let mut empty = Cursor::new(Vec::<u8>::new());
    let latex_out = ganbate_agent::run(&latex_args, &mut empty, None);
    assert_eq!(latex_out.status, 0, "{}", latex_out.stdout);
    assert!(latex_out.stdout.contains("\\begin{table}[t]"));
    assert!(latex_out.stdout.contains("\\begin{tabular}{lrrrr}"));
    assert!(latex_out.stdout.contains("\\toprule"));
    assert!(latex_out.stdout.contains("\\midrule"));
    assert!(latex_out.stdout.contains("\\bottomrule"));
    assert!(latex_out.stdout.contains("\\end{tabular}"));
    assert!(latex_out.stdout.contains("\\end{table}"));

    // 3. Default format (`markdown`) renders GFM summary + latency table.
    let md_args: Vec<String> = ["bench", "--project", proj_str, "--iterations", "20"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    let mut empty_md = Cursor::new(Vec::<u8>::new());
    let md_out = ganbate_agent::run(&md_args, &mut empty_md, None);
    assert_eq!(md_out.status, 0, "{}", md_out.stdout);
    assert!(
        md_out
            .stdout
            .contains("# Ganbate Engine — Reproducible Empirical Benchmark Report")
    );
    assert!(md_out.stdout.contains("| Subsystem Operation | Min (µs) |"));

    // 4. `world_graph_benchmark` via `ganbate mcp` stdio JSON-RPC 2.0 on an initialized Ganbate project.
    let mut ids = Ids::seeded(101);
    Project::create(&dir.0, "Bench MCP Test", &mut ids).unwrap();
    let rpc_in = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"world_graph_benchmark","arguments":{"iterations":20,"format":"markdown"}}}"#;
    let mcp_args = vec!["mcp".to_string(), proj_str.to_string()];
    let mut cursor = Cursor::new(format!("{rpc_in}\n").into_bytes());
    let mcp_out = ganbate_agent::run(&mcp_args, &mut cursor, None);
    assert_eq!(mcp_out.status, 0, "{}", mcp_out.stdout);
    let rpc_val = json::parse(mcp_out.stdout.trim().as_bytes()).unwrap();
    let sc = rpc_val
        .get("result")
        .and_then(|r| r.get("structuredContent"))
        .unwrap();
    assert_eq!(sc.get("ok"), Some(&Value::Bool(true)));
    assert_eq!(sc.get("format"), Some(&Value::String("markdown".into())));
}
