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
