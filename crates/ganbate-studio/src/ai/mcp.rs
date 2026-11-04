//! Model Context Protocol (MCP) JSON-RPC 2.0 dispatcher (`M-AI.1`).
//!
//! Runs directly against a live [`Studio`] session (either via `POST /api/mcp` on `ganbate-studio`
//! or over `stdio` in `ganbate mcp`), returning both the JSON-RPC response and any [`Outbox`]
//! broadcasts so connected Studio viewports update in `<5ms` with full Undo/Redo support.

use std::collections::BTreeMap;

use ganbate_core::json::{self, Value};

use crate::session::{Outbox, Studio};

pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

fn s(v: &str) -> Value {
    Value::String(v.into())
}

fn obj(pairs: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn rpc_ok(id: Value, result: Value) -> Value {
    obj([("jsonrpc", s("2.0")), ("id", id), ("result", result)])
}

fn rpc_err(id: Value, code: i64, message: &str) -> Value {
    obj([
        ("jsonrpc", s("2.0")),
        ("id", id),
        (
            "error",
            obj([
                ("code", Value::Number(code as f64)),
                ("message", s(message)),
            ]),
        ),
    ])
}

fn tool_def(
    name: &'static str,
    description: &'static str,
    props: Value,
    required: &[&str],
) -> Value {
    obj([
        ("name", s(name)),
        ("description", s(description)),
        (
            "inputSchema",
            obj([
                ("type", s("object")),
                ("properties", props),
                (
                    "required",
                    Value::Array(required.iter().map(|r| s(r)).collect()),
                ),
            ]),
        ),
    ])
}

fn prop_str(desc: &'static str) -> Value {
    obj([("type", s("string")), ("description", s(desc))])
}

fn prop_bool(desc: &'static str) -> Value {
    obj([("type", s("boolean")), ("description", s(desc))])
}

fn prop_num(desc: &'static str) -> Value {
    obj([("type", s("number")), ("description", s(desc))])
}

fn prop_arr_str(desc: &'static str) -> Value {
    obj([
        ("type", s("array")),
        ("description", s(desc)),
        ("items", obj([("type", s("string"))])),
    ])
}

pub fn tools_list() -> Value {
    Value::Array(vec![
        tool_def(
            "project_context",
            "Read the <800-token L0/L1 AI Context Digest of the entire open game project (scenes, schemas, workspaces, @selection, top bricks, and .gbs grammar).",
            obj([(
                "limit",
                prop_num("Max top brick signatures to include (default 24)"),
            )]),
            &[],
        ),
        tool_def(
            "studio_status",
            "Inspect the open Ganbate project, open documents, dirty flags, and connected Studio clients.",
            obj([]),
            &[],
        ),
        tool_def(
            "studio_selection",
            "Read the live @selection from Ganbate Studio (active document, selected entities, active camera, 3D cursor).",
            obj([]),
            &[],
        ),
        tool_def(
            "catalog_search",
            "Search the token-compact Brick, Prefab, Model, Schema, and Look catalog (.ganbate/catalog.idx).",
            obj([
                ("query", prop_str("Optional search keywords")),
                (
                    "kind",
                    prop_str(
                        "Optional kind filter: brick, prefab, model, trait, schema, look, material, data",
                    ),
                ),
                ("limit", prop_num("Max results (default 32)")),
            ]),
            &[],
        ),
        tool_def(
            "catalog_inspect",
            "Inspect full parameters, AABB, sockets, and example .gbs snippet for a catalog item.",
            obj([(
                "ref",
                prop_str("Catalog key (e.g. brick:ring_lanterns, prefab:lantern) or asset id"),
            )]),
            &["ref"],
        ),
        tool_def(
            "brick_crystallize",
            "Crystallize selected or named scene entities into a reusable parameterized Brick in .ganbate/bricks/<id>.brick.json.",
            obj([
                (
                    "doc",
                    prop_str("Scene/Prefab document id or path (defaults to Studio active doc)"),
                ),
                ("id", prop_str("Lowercase brick slug, e.g. lantern_cluster")),
                (
                    "entities",
                    prop_arr_str("Entity IDs or names (defaults to @selection)"),
                ),
                (
                    "fromTx",
                    prop_bool("Extract entities from latest .ganbate/history transaction"),
                ),
                (
                    "params",
                    prop_arr_str("Optional param bindings: name=component.field"),
                ),
            ]),
            &["id"],
        ),
        tool_def(
            "scene_gbs",
            "Apply a .gbs relational spatial patch script (spawn, place snap/socket/beside, ring, attach, cam frame, set, delete) as one undoable Studio transaction.",
            obj([
                (
                    "doc",
                    prop_str("Target document id or path (defaults to Studio active doc)"),
                ),
                ("script", prop_str("Multi-line .gbs spatial patch script")),
                (
                    "dryRun",
                    prop_bool("Validate and solve without mutating Studio state"),
                ),
            ]),
            &["script"],
        ),
        tool_def(
            "scene_verify",
            "Run the <100ms spatial, screen-box ([u0,v0,u1,v1] + occlusion), determinism hash, and GPU-budget oracle.",
            obj([
                (
                    "doc",
                    prop_str("Scene/Prefab document id or path (defaults to Studio active doc)"),
                ),
                ("camera", prop_str("Optional camera entity name/id")),
                (
                    "contactSheetPath",
                    prop_str("Optional project-relative path to write a 2x2 diagnostic AOV PNG"),
                ),
            ]),
            &[],
        ),
        tool_def(
            "doc_open",
            "Open a scene, prefab, Look, data, material, soundbank, character, seq, or vfx document.",
            obj([
                ("doc", prop_str("16-hex document ID")),
                ("path", prop_str("Or project-relative file path")),
            ]),
            &[],
        ),
        tool_def(
            "doc_edit",
            "Apply raw JSON document ops ({set, value} or {remove}) as one undoable transaction.",
            obj([
                ("doc", prop_str("Document ID or path")),
                ("label", prop_str("Undo label")),
                (
                    "ops",
                    obj([
                        ("type", s("array")),
                        ("items", obj([("type", s("object"))])),
                    ]),
                ),
            ]),
            &["ops"],
        ),
        tool_def(
            "doc_undo",
            "Undo the last Studio transaction on a document.",
            obj([(
                "doc",
                prop_str("Document ID or path (defaults to active doc)"),
            )]),
            &[],
        ),
        tool_def(
            "doc_redo",
            "Redo the next Studio transaction on a document.",
            obj([(
                "doc",
                prop_str("Document ID or path (defaults to active doc)"),
            )]),
            &[],
        ),
        tool_def(
            "doc_save",
            "Save an open Studio document canonically to disk.",
            obj([
                (
                    "doc",
                    prop_str("Document ID or path (defaults to active doc)"),
                ),
                ("force", prop_bool("Overwrite even if changed on disk")),
            ]),
            &[],
        ),
        tool_def(
            "world_graph_fetch",
            "Read the <400-token Tri-Modal Semantic World Graph (ganbate://context) persisted at world.semantic.json.",
            obj([]),
            &[],
        ),
        tool_def(
            "world_graph_patch",
            "Apply a structured JSON patch to the Semantic World Graph (ganbate://context) under 120Hz (8.33ms) and server-authoritative-readonly guardrails.",
            obj([(
                "patch",
                obj([
                    ("type", s("object")),
                    (
                        "description",
                        s(
                            "Partial SemanticWorldGraph JSON patch (play_mode, guardrails, environment, camera, entities)",
                        ),
                    ),
                ]),
            )]),
            &["patch"],
        ),
        tool_def(
            "world_graph_prompt",
            "Deterministically compile a genre-agnostic bilingual (EN/KO) natural-language prompt into a validated Semantic World Graph patch and broadcast via Outbox.",
            obj([(
                "prompt",
                prop_str("Natural-language instruction in English or Korean"),
            )]),
            &["prompt"],
        ),
    ])
}

/// Dispatches an MCP tool call against `studio`, populating `outbox` with any live viewport broadcasts.
pub fn call_tool(
    studio: &mut Studio,
    name: &str,
    args: &Value,
    outbox: &mut Outbox,
) -> Result<Value, String> {
    studio.call_mcp_tool(name, args, outbox)
}

fn wrap_tool_result(payload: Value, is_error: bool) -> Value {
    let text = json::to_compact(&payload).unwrap_or_else(|_| "{}".into());
    obj([
        (
            "content",
            Value::Array(vec![obj([
                ("type", s("text")),
                ("text", Value::String(text)),
            ])]),
        ),
        ("structuredContent", payload),
        ("isError", Value::Bool(is_error)),
    ])
}

/// Processes a single JSON-RPC 2.0 request payload against `studio`.
/// Returns `None` for JSON-RPC notifications (no `id` field), or `Some(response_json)` for calls,
/// along with any `Outbox` messages to broadcast to connected WebSocket Studio clients.
pub fn handle_rpc_bytes(studio: &mut Studio, body: &[u8]) -> (Option<Value>, Outbox) {
    let parsed = match json::parse(body) {
        Ok(v) => v,
        Err(e) => {
            return (
                Some(rpc_err(Value::Null, -32700, &format!("parse error: {e}"))),
                Vec::new(),
            );
        }
    };
    handle_rpc_value(studio, &parsed)
}

pub fn handle_rpc_value(studio: &mut Studio, req: &Value) -> (Option<Value>, Outbox) {
    let is_notification = req.get("id").is_none();
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let Some(method) = req.get("method").and_then(|m| m.string().ok()) else {
        return (
            Some(rpc_err(
                id,
                -32600,
                "invalid JSON-RPC request: missing method",
            )),
            Vec::new(),
        );
    };
    let params = req
        .get("params")
        .cloned()
        .unwrap_or_else(|| Value::Object(BTreeMap::new()));

    if is_notification {
        return (None, Vec::new());
    }

    match method {
        "initialize" => (
            Some(rpc_ok(
                id,
                obj([
                    ("protocolVersion", s(MCP_PROTOCOL_VERSION)),
                    (
                        "capabilities",
                        obj([("tools", obj([])), ("resources", obj([]))]),
                    ),
                    (
                        "serverInfo",
                        obj([
                            ("name", s("ganbate-studio")),
                            ("version", s(env!("CARGO_PKG_VERSION"))),
                        ]),
                    ),
                ]),
            )),
            Vec::new(),
        ),
        "ping" => (Some(rpc_ok(id, obj([]))), Vec::new()),
        "tools/list" => (Some(rpc_ok(id, obj([("tools", tools_list())]))), Vec::new()),
        "resources/list" => {
            let resources = Value::Array(vec![
                obj([
                    ("uri", s("ganbate://context")),
                    (
                        "name",
                        s("Tri-Modal Semantic World Graph (<400 tokens, 120Hz guardrailed)"),
                    ),
                    ("mimeType", s("application/json")),
                ]),
                obj([
                    ("uri", s("ganbate://project-context")),
                    ("name", s("Token-Compact L0/L1 Project Context Digest")),
                    ("mimeType", s("text/markdown")),
                ]),
                obj([
                    ("uri", s("ganbate://selection")),
                    ("name", s("Live Studio @selection")),
                    ("mimeType", s("application/json")),
                ]),
                obj([
                    ("uri", s("ganbate://catalog")),
                    ("name", s("Token-Compact Brick & Schema Catalog")),
                    ("mimeType", s("text/plain")),
                ]),
            ]);
            (
                Some(rpc_ok(id, obj([("resources", resources)]))),
                Vec::new(),
            )
        }
        "resources/read" => {
            let uri = params
                .get("uri")
                .and_then(|v| v.string().ok())
                .unwrap_or("");
            match uri {
                "ganbate://context" => match studio.world_graph_read_or_init() {
                    Ok((graph, _)) => match graph.to_canonical_json() {
                        Ok(text) => {
                            let res = obj([(
                                "contents",
                                Value::Array(vec![obj([
                                    ("uri", s(uri)),
                                    ("mimeType", s("application/json")),
                                    ("text", Value::String(text)),
                                ])]),
                            )]);
                            (Some(rpc_ok(id, res)), Vec::new())
                        }
                        Err(e) => (Some(rpc_err(id, -32603, &e)), Vec::new()),
                    },
                    Err(e) => (Some(rpc_err(id, -32603, &e)), Vec::new()),
                },
                "ganbate://project-context" => match studio.mcp_project_context(24) {
                    Ok(ctx) => {
                        let text = ctx
                            .get("digestText")
                            .and_then(|v| v.string().ok())
                            .unwrap_or("")
                            .to_string();
                        let res = obj([(
                            "contents",
                            Value::Array(vec![obj([
                                ("uri", s(uri)),
                                ("mimeType", s("text/markdown")),
                                ("text", Value::String(text)),
                            ])]),
                        )]);
                        (Some(rpc_ok(id, res)), Vec::new())
                    }
                    Err(e) => (Some(rpc_err(id, -32603, &e)), Vec::new()),
                },
                "ganbate://selection" => {
                    let sel = studio.mcp_selection_details();
                    let text = json::to_compact(&sel).unwrap_or_default();
                    let res = obj([(
                        "contents",
                        Value::Array(vec![obj([
                            ("uri", s(uri)),
                            ("mimeType", s("application/json")),
                            ("text", Value::String(text)),
                        ])]),
                    )]);
                    (Some(rpc_ok(id, res)), Vec::new())
                }
                "ganbate://catalog" => {
                    match super::catalog::build_catalog(&studio.project, &studio.registry, true) {
                        Ok(cat) => {
                            let res = obj([(
                                "contents",
                                Value::Array(vec![obj([
                                    ("uri", s(uri)),
                                    ("mimeType", s("text/plain")),
                                    ("text", Value::String(cat.index_text())),
                                ])]),
                            )]);
                            (Some(rpc_ok(id, res)), Vec::new())
                        }
                        Err(e) => (Some(rpc_err(id, -32603, &e)), Vec::new()),
                    }
                }
                other => (
                    Some(rpc_err(
                        id,
                        -32602,
                        &format!("unknown resource URI {other:?}"),
                    )),
                    Vec::new(),
                ),
            }
        }
        "tools/call" => {
            let Some(name) = params.get("name").and_then(|v| v.string().ok()) else {
                return (
                    Some(rpc_err(id, -32602, "tools/call requires params.name")),
                    Vec::new(),
                );
            };
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| Value::Object(BTreeMap::new()));
            let mut outbox = Vec::new();
            match call_tool(studio, name, &args, &mut outbox) {
                Ok(payload) => {
                    let blocked = payload.get("blocked_by_authority") == Some(&Value::Bool(true));
                    (Some(rpc_ok(id, wrap_tool_result(payload, blocked))), outbox)
                }
                Err(err_msg) => {
                    let err_payload = obj([
                        ("ok", Value::Bool(false)),
                        ("error", Value::String(err_msg)),
                    ]);
                    (
                        Some(rpc_ok(id, wrap_tool_result(err_payload, true))),
                        outbox,
                    )
                }
            }
        }
        other => (
            Some(rpc_err(id, -32601, &format!("method not found: {other}"))),
            Vec::new(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::world_graph::{WORLD_GRAPH_REL_PATH, WORLD_GRAPH_SCHEMA, WORLD_GRAPH_URI};
    use ganbate_core::id::Ids;
    use ganbate_pipeline::project::Project;

    struct TempDir(std::path::PathBuf);
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn open_test_studio() -> (Studio, TempDir) {
        let dir = std::env::temp_dir().join(format!("ganbate-mcp-wg-{}", Ids::from_os().entity()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut ids = Ids::seeded(19);
        Project::create(&dir, "MCP World Graph Test", &mut ids).unwrap();
        let (studio, _) = Studio::open(&dir, ids).unwrap();
        (studio, TempDir(dir))
    }

    #[test]
    fn mcp_ganbate_context_resource_and_world_graph_tools_sync_and_guardrail() {
        let (mut studio, dir) = open_test_studio();
        let client_a = studio.connect("living-canvas-a");
        let client_b = studio.connect("living-canvas-b");

        // 1. resources/list advertises ganbate://context as application/json.
        let (list_resp, _) = handle_rpc_bytes(
            &mut studio,
            br#"{"jsonrpc":"2.0","id":1,"method":"resources/list","params":{}}"#,
        );
        let resources = list_resp
            .unwrap()
            .get("result")
            .and_then(|r| r.get("resources"))
            .and_then(|r| r.array().ok())
            .unwrap()
            .to_vec();
        assert!(
            resources
                .iter()
                .any(|r| r.get("uri") == Some(&s(WORLD_GRAPH_URI))
                    && r.get("mimeType") == Some(&s("application/json")))
        );

        // 2. resources/read on ganbate://context initializes world.semantic.json on first read (<400 tokens).
        assert!(!dir.0.join(WORLD_GRAPH_REL_PATH).exists());
        let (read_resp, _) = handle_rpc_bytes(
            &mut studio,
            br#"{"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":"ganbate://context"}}"#,
        );
        assert!(dir.0.join(WORLD_GRAPH_REL_PATH).is_file());
        let text = read_resp
            .unwrap()
            .get("result")
            .and_then(|r| r.get("contents"))
            .and_then(|c| c.array().ok())
            .and_then(|a| a.first())
            .and_then(|item| item.get("text"))
            .and_then(|t| t.string().ok())
            .unwrap()
            .to_string();
        assert!(text.len().div_ceil(4) < 400);
        let parsed_ctx = json::parse(text.as_bytes()).unwrap();
        assert_eq!(parsed_ctx.get("schema"), Some(&s(WORLD_GRAPH_SCHEMA)));
        assert_eq!(parsed_ctx.get("uri"), Some(&s(WORLD_GRAPH_URI)));

        // 3. tools/list includes world_graph_fetch, world_graph_patch, and world_graph_prompt.
        let tools = tools_list();
        let tool_names: Vec<&str> = tools
            .array()
            .unwrap()
            .iter()
            .filter_map(|t| t.get("name").and_then(|n| n.string().ok()))
            .collect();
        for expected in [
            "world_graph_fetch",
            "world_graph_patch",
            "world_graph_prompt",
        ] {
            assert!(tool_names.contains(&expected), "missing tool {expected}");
        }

        // 4. world_graph_prompt compiles bilingual Korean prompt, persists, and broadcasts to both clients in <5ms.
        let t0 = std::time::Instant::now();
        let (prompt_resp, outbox) = handle_rpc_bytes(
            &mut studio,
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"world_graph_prompt","arguments":{"prompt":"노을 조명으로 바꾸고 천장 열어줘"}}}"#.as_bytes(),
        );
        let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
        assert!(
            elapsed_ms < 50.0,
            "expected fast prompt sync, took {elapsed_ms:.2}ms"
        );

        let p_res = prompt_resp.unwrap();
        let structured = p_res
            .get("result")
            .and_then(|r| r.get("structuredContent"))
            .unwrap();
        assert_eq!(structured.get("applied"), Some(&Value::Bool(true)));
        assert_eq!(
            structured.get("blocked_by_authority"),
            Some(&Value::Bool(false))
        );
        assert_eq!(outbox.len(), 2);
        assert_eq!(outbox[0].0, client_a);
        assert_eq!(outbox[1].0, client_b);
        assert_eq!(outbox[0].1.get("type"), Some(&s("world-graph-updated")));

        // 5. world_graph_patch clamps 120Hz (8.33ms) light/FOV guardrails and records notices.
        let (patch_resp, patch_outbox) = handle_rpc_bytes(
            &mut studio,
            br#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"world_graph_patch","arguments":{"patch":{"guardrails":{"max_dynamic_lights":25,"frame_budget_ms":16.6},"camera":{"fov_y_deg":150}}}}}"#,
        );
        assert_eq!(patch_outbox.len(), 2);
        let patch_sc = patch_resp
            .unwrap()
            .get("result")
            .and_then(|r| r.get("structuredContent"))
            .cloned()
            .unwrap();
        assert_eq!(
            patch_sc
                .get("graph")
                .and_then(|g| g.get("guardrails"))
                .and_then(|gr| gr.get("max_dynamic_lights")),
            Some(&Value::Number(8.0))
        );
        assert_eq!(
            patch_sc
                .get("graph")
                .and_then(|g| g.get("camera"))
                .and_then(|c| c.get("fov_y_deg")),
            Some(&Value::Number(110.0))
        );
        assert!(
            patch_sc
                .get("guardrail_notices")
                .and_then(|n| n.array().ok())
                .is_some_and(|arr| arr.len() >= 3)
        );

        // 6. Server-authority prompt is blocked without mutating disk or broadcasting.
        let disk_before = std::fs::read_to_string(dir.0.join(WORLD_GRAPH_REL_PATH)).unwrap();
        let (blocked_resp, blocked_outbox) = handle_rpc_bytes(
            &mut studio,
            br#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"world_graph_prompt","arguments":{"prompt":"override rng seed and rig outcome"}}}"#,
        );
        assert!(blocked_outbox.is_empty());
        let disk_after = std::fs::read_to_string(dir.0.join(WORLD_GRAPH_REL_PATH)).unwrap();
        assert_eq!(disk_before, disk_after);

        let b_res = blocked_resp.unwrap();
        assert_eq!(
            b_res.get("result").and_then(|r| r.get("isError")),
            Some(&Value::Bool(true))
        );
        let b_sc = b_res
            .get("result")
            .and_then(|r| r.get("structuredContent"))
            .unwrap();
        assert_eq!(b_sc.get("blocked_by_authority"), Some(&Value::Bool(true)));
        assert_eq!(b_sc.get("applied"), Some(&Value::Bool(false)));

        // 7. world_graph_fetch tool and Studio WebSocket edit-protocol messages stay in sync.
        let (fetch_resp, _) = handle_rpc_bytes(
            &mut studio,
            br#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"world_graph_fetch","arguments":{}}}"#,
        );
        let fetch_sc = fetch_resp
            .unwrap()
            .get("result")
            .and_then(|r| r.get("structuredContent"))
            .cloned()
            .unwrap();
        assert_eq!(fetch_sc.get("ok"), Some(&Value::Bool(true)));
        assert_eq!(
            fetch_sc
                .get("graph")
                .and_then(|g| g.get("environment"))
                .and_then(|e| e.get("preset")),
            Some(&s("sunset"))
        );

        let ws_out = studio.handle(
            client_a,
            &json::parse(
                br#"{"type":"world-graph-prompt","prompt":"pause into edit mode","req":"wg-1"}"#,
            )
            .unwrap(),
        );
        assert!(
            ws_out.iter().any(
                |(cid, m)| *cid == client_b && m.get("type") == Some(&s("world-graph-updated"))
            )
        );
        assert!(
            ws_out
                .iter()
                .any(|(cid, m)| *cid == client_a
                    && m.get("type") == Some(&s("world-graph-prompted")))
        );
    }
}
