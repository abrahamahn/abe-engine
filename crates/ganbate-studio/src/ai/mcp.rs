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
            obj([("limit", prop_num("Max top brick signatures to include (default 24)"))]),
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
    ])
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
                "ganbate://context" => match studio.mcp_project_context(24) {
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
            match studio.call_mcp_tool(name, &args, &mut outbox) {
                Ok(payload) => (Some(rpc_ok(id, wrap_tool_result(payload, false))), outbox),
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
