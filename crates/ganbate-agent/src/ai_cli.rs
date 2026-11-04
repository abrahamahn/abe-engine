//! CLI bridges for the AI-Native Agentic IDE (`M-AI.1..4`):
//! - `ganbate catalog build|search|inspect`
//! - `ganbate brick crystallize`
//! - `ganbate gbs apply`
//! - `ganbate verify`
//! - `ganbate mcp` (stdio JSON-RPC 2.0 server, in-process or forwarded to a warm `ganbate-studio` host)

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::time::Duration;

use ganbate_core::digest;
use ganbate_core::id::Ids;
use ganbate_core::json::{self as core_json, Value, to_canonical};
use ganbate_studio::ai::{bench, catalog, gbs, mcp, verify, world_graph};
use ganbate_studio::session::Studio;

use crate::args::Args;
use crate::error::{self, Error};
use crate::json::{J, Obj};
use crate::workspace::{self, about, sha256};
use crate::{Outcome, Reply, history};

pub fn catalog_build(project: &str) -> Result<Reply, Error> {
    let mut ws = workspace::open(project)?;
    ws.index()?;
    let cat = catalog::build_catalog(&ws.project, &ws.registry, true).map_err(error::invalid)?;
    Ok(Reply::ok(
        Obj::new()
            .with("path", catalog::CATALOG_REL_PATH)
            .with("count", cat.items.len())
            .with("index", cat.index_text()),
    ))
}

pub fn catalog_search(project: &str, query: Option<&str>, a: &Args) -> Result<Reply, Error> {
    let mut ws = workspace::open(project)?;
    ws.index()?;
    let kind = a.value("--kind")?;
    let limit = match a.value("--limit")? {
        Some(raw) => raw
            .parse::<usize>()
            .ok()
            .filter(|&n| (1..=512).contains(&n))
            .ok_or_else(|| error::usage("--limit must be 1..=512"))?,
        None => 32,
    };
    let cat = catalog::build_catalog(&ws.project, &ws.registry, true).map_err(error::invalid)?;
    let hits = cat.search(query, kind, limit);
    let signatures: Vec<J> = hits
        .iter()
        .map(|it| J::from(it.signature.as_str()))
        .collect();
    let items: Vec<J> = hits.into_iter().map(|it| J::from(it.to_json())).collect();
    Ok(Reply::ok(
        Obj::new()
            .with("count", items.len())
            .with("signatures", J::Arr(signatures))
            .with("items", J::Arr(items)),
    ))
}

pub fn catalog_inspect(project: &str, reference: &str) -> Result<Reply, Error> {
    let mut ws = workspace::open(project)?;
    ws.index()?;
    let cat = catalog::build_catalog(&ws.project, &ws.registry, false).map_err(error::invalid)?;
    let item = catalog::inspect_item(&cat, &ws.project, reference).map_err(error::not_found)?;
    Ok(Reply::ok(Obj::new().with("item", item)))
}

pub fn brick_crystallize(project: &str, scene_arg: &str, a: &Args) -> Result<Reply, Error> {
    let brick_id = a
        .value("--id")?
        .ok_or_else(|| error::usage("ganbate brick crystallize requires --id SLUG"))?;
    let mut ws = workspace::open(project)?;
    ws.index()?;
    let at = ws.locate(scene_arg)?;
    let file = ws.read(&at)?;

    let mut seeds: Vec<String> = a.values("--entity").to_vec();
    if seeds.is_empty() && a.switch("--from-tx") {
        seeds = catalog::entities_from_history_tx(&ws.project.root, &at.id, None)
            .map_err(error::not_found)?;
    }
    if seeds.is_empty() {
        return Err(error::usage(
            "pass at least one --entity REF or --from-tx to crystallize a brick",
        ));
    }
    let params: Vec<String> = a.values("--param").to_vec();
    let brick = catalog::crystallize_brick(
        &ws.project,
        &ws.registry,
        &file.root,
        brick_id,
        &seeds,
        &params,
    )
    .map_err(error::bad_edit)?;

    Ok(Reply::ok(
        Obj::new()
            .with("scene", about(&at, &file.root))
            .with("brick", brick),
    ))
}

fn deterministic_ids(scene_bytes: &[u8], script_bytes: &[u8]) -> Ids {
    let mut combined = Vec::with_capacity(scene_bytes.len() + script_bytes.len() + 1);
    combined.extend_from_slice(scene_bytes);
    combined.push(b'\n');
    combined.extend_from_slice(script_bytes);
    let digest_bytes = digest::sha256(&combined);
    let mut arr = [0u8; 8];
    arr.copy_from_slice(&digest_bytes[..8]);
    Ids::seeded(u64::from_le_bytes(arr))
}

pub fn gbs_apply(
    project: &str,
    scene_arg: &str,
    script_arg: &str,
    dry_run: bool,
    stdin: &mut dyn Read,
) -> Result<Reply, Error> {
    let script_bytes = if script_arg == "-" {
        workspace::read_all(stdin, "standard input")?
    } else {
        workspace::read_file(Path::new(script_arg))?
    };
    let script_text = String::from_utf8(script_bytes.clone())
        .map_err(|e| error::invalid(format!("script is not UTF-8: {e}")))?;

    let mut ws = workspace::open(project)?;
    ws.index()?;
    let at = ws.locate(scene_arg)?;
    let file = ws.read(&at)?;

    let mut ids = deterministic_ids(&file.bytes, &script_bytes);
    let outcome = gbs::execute_gbs(
        &ws.project,
        &ws.registry,
        &file.root,
        &script_text,
        None,
        &|id| ws.prefab(id),
        &mut ids,
    )
    .map_err(error::bad_edit)?;

    let old_text = to_canonical(&file.root).map_err(error::invalid)?;
    let text = to_canonical(&outcome.root).map_err(error::invalid)?;
    let mut hash = sha256(text.as_bytes());
    let written = !dry_run && text != old_text;

    if written {
        let _lock = history::Lock::take(&ws.project.root, &at.id)?;
        if workspace::read_file(&at.path)? != file.bytes {
            return Err(error::changed(format!(
                "{} changed while the .gbs script was applied; run it again",
                at.rel
            )));
        }
        let history_file = history::file(&ws.project.root, &at.id);
        let mut lines = history::read(&history_file)?;
        ws.project
            .write_document(&at.rel, at.kind, &outcome.root)
            .map_err(error::io)?;
        if let Ok(on_disk) = workspace::read_file(&at.path) {
            hash = sha256(&on_disk);
        }
        let hand_written = old_text.as_bytes() != file.bytes.as_slice();
        lines.push(
            history::Record {
                label: outcome.label.clone(),
                edits: outcome.statements,
                before: sha256(&file.bytes),
                after: hash.clone(),
                ops: outcome.ops.clone(),
                undo: outcome.undo.clone(),
                text: hand_written.then(|| String::from_utf8_lossy(&file.bytes).into_owned()),
            }
            .line(),
        );
        if let Err(e) = history::write(&history_file, &lines) {
            let _ = ganbate_assets::write_asset(&at.path, &file.bytes);
            return Err(e);
        }
    }

    let spawned = outcome.spawned_json();
    Ok(Reply::ok(
        Obj::new()
            .with("scene", about(&at, &outcome.root))
            .with("dry_run", dry_run)
            .with("written", written)
            .with("label", outcome.label)
            .with("statements", outcome.statements)
            .with("spawned", spawned)
            .with("hash", hash),
    ))
}

fn parse_size(raw: Option<&str>) -> Result<(u32, u32), Error> {
    let Some(s) = raw else {
        return Ok((320, 180));
    };
    let (w, h) = s
        .split_once('x')
        .or_else(|| s.split_once('X'))
        .ok_or_else(|| error::usage(format!("invalid --size {s:?}; expected WIDTHxHEIGHT")))?;
    let width = w
        .parse::<u32>()
        .ok()
        .filter(|&v| (64..=4096).contains(&v))
        .ok_or_else(|| error::usage("--size width must be 64..=4096"))?;
    let height = h
        .parse::<u32>()
        .ok()
        .filter(|&v| (64..=4096).contains(&v))
        .ok_or_else(|| error::usage("--size height must be 64..=4096"))?;
    Ok((width, height))
}

pub fn verify_scene_cli(project: &str, scene_arg: &str, a: &Args) -> Result<Reply, Error> {
    let mut ws = workspace::open(project)?;
    ws.index()?;
    let at = ws.locate(scene_arg)?;
    let file = ws.read(&at)?;

    let (width, height) = parse_size(a.value("--size")?)?;
    let contact_sheet = a
        .value("--contact-sheet")?
        .or(a.value("--out")?)
        .map(String::from);
    let opts = verify::VerifyOptions {
        camera_ref: a.value("--camera")?.map(String::from),
        width,
        height,
        render_contact_sheet: contact_sheet.is_some(),
        ..Default::default()
    };

    let report = verify::verify_scene(
        &ws.project,
        &ws.registry,
        &file.root,
        None,
        &|id| ws.prefab(id),
        &opts,
    )
    .map_err(error::invalid)?;

    if let Some(ref out_path) = contact_sheet
        && let Some(ref png) = report.contact_sheet_png
    {
        let p = Path::new(out_path);
        if let Some(parent) = p.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .map_err(|e| error::io(format!("{}: {e}", parent.display())))?;
        }
        ganbate_assets::write_asset(p, png)
            .map_err(|e| error::io(format!("{}: {e}", p.display())))?;
    }

    let report_val = report.to_json(contact_sheet.as_deref());
    let mut body = Obj::new().with("scene", about(&at, &file.root));
    if let Value::Object(map) = report_val {
        for (k, v) in map {
            body.put(&k, v);
        }
    }
    let err = (!report.ok).then(|| error::invalid("scene verification found errors"));
    Ok(Reply { body, error: err })
}

fn forward_mcp_http(
    studio_url: &str,
    token: Option<&str>,
    body: &[u8],
) -> Result<Option<String>, String> {
    let trimmed = studio_url
        .trim()
        .strip_prefix("http://")
        .unwrap_or(studio_url.trim());
    let host_port = trimmed.split('/').next().unwrap_or(trimmed);
    let mut stream = TcpStream::connect(host_port)
        .map_err(|e| format!("failed to connect to Studio {host_port}: {e}"))?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));

    let cookie_line = token
        .filter(|t| !t.is_empty())
        .map(|t| format!("Cookie: ganbate={t}\r\n"))
        .unwrap_or_default();
    let req_head = format!(
        "POST /api/mcp HTTP/1.1\r\nHost: {host_port}\r\nContent-Type: application/json\r\n{cookie_line}Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(req_head.as_bytes())
        .and_then(|()| stream.write_all(body))
        .and_then(|()| stream.flush())
        .map_err(|e| format!("Studio MCP write failed: {e}"))?;

    let mut raw = Vec::new();
    stream
        .read_to_end(&mut raw)
        .map_err(|e| format!("Studio MCP read failed: {e}"))?;
    let sep = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| "invalid HTTP response from Studio".to_string())?;
    let header_str = String::from_utf8_lossy(&raw[..sep]);
    if header_str.starts_with("HTTP/1.1 204") || header_str.starts_with("HTTP/1.0 204") {
        return Ok(None);
    }
    let payload = String::from_utf8_lossy(&raw[sep + 4..]).trim().to_string();
    if payload.is_empty() {
        Ok(None)
    } else {
        Ok(Some(payload))
    }
}

/// Runs the JSON-RPC 2.0 MCP server over `stdin`/`stdout` (streaming line-by-line).
pub fn serve_mcp_stream(
    project: &str,
    studio_url: Option<&str>,
    token: Option<&str>,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
) -> Result<(), Error> {
    let mut local_studio = if studio_url.is_none() {
        let (st, _) = Studio::open(Path::new(project), Ids::from_os()).map_err(error::invalid)?;
        Some(st)
    } else {
        None
    };

    let reader = BufReader::new(stdin);
    for line_res in reader.lines() {
        let line = line_res.map_err(|e| error::io(format!("mcp stdin: {e}")))?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let response_line = if let Some(url) = studio_url {
            match forward_mcp_http(url, token, trimmed.as_bytes()) {
                Ok(resp) => resp,
                Err(e) => core_json::parse(trimmed.as_bytes())
                    .ok()
                    .and_then(|v| v.get("id").cloned())
                    .map(|id| {
                        format!(
                            r#"{{"jsonrpc":"2.0","id":{},"error":{{"code":-32000,"message":{:?}}}}}"#,
                            core_json::to_compact(&id).unwrap_or_else(|_| "null".into()),
                            e
                        )
                    }),
            }
        } else if let Some(ref mut st) = local_studio {
            let (resp_opt, _) = mcp::handle_rpc_bytes(st, trimmed.as_bytes());
            resp_opt.and_then(|v| core_json::to_compact(&v).ok())
        } else {
            None
        };

        if let Some(resp) = response_line {
            writeln!(stdout, "{resp}").map_err(|e| error::io(format!("mcp stdout: {e}")))?;
            stdout
                .flush()
                .map_err(|e| error::io(format!("mcp flush: {e}")))?;
        }
    }
    Ok(())
}

/// Entrypoint used by `main.rs` when `args` starts with `mcp` (streaming line-by-line).
pub fn run_mcp_stdio(rest_args: &[String], stdin: &mut dyn Read, stdout: &mut dyn Write) -> i32 {
    let mut full = vec!["mcp".to_string()];
    full.extend_from_slice(rest_args);
    let parsed = match crate::args::parse(&full) {
        Ok(a) => a,
        Err(e) => {
            let out = crate::failure(e, false);
            let _ = writeln!(stdout, "{}", out.stdout);
            return out.status;
        }
    };
    if let Err(e) = parsed.only(&["--studio", "--token"], "ganbate mcp") {
        let out = crate::failure(e, false);
        let _ = writeln!(stdout, "{}", out.stdout);
        return out.status;
    }
    if parsed.words.len() != 2 {
        let out = crate::failure(
            error::usage(
                "usage: ganbate mcp PROJECT [--studio http://127.0.0.1:PORT] [--token TOKEN]",
            ),
            false,
        );
        let _ = writeln!(stdout, "{}", out.stdout);
        return out.status;
    }
    let project = &parsed.words[1];
    let studio_url = parsed.value("--studio").ok().flatten();
    let token = parsed.value("--token").ok().flatten();
    match serve_mcp_stream(project, studio_url, token, stdin, stdout) {
        Ok(()) => 0,
        Err(e) => {
            let out = crate::failure(e, false);
            let _ = writeln!(stdout, "{}", out.stdout);
            out.status
        }
    }
}

/// Buffered MCP execution for `ganbate_agent::run` library callers & integration tests.
pub fn run_mcp_buffered(a: &Args, stdin: &mut dyn Read) -> Result<Outcome, Error> {
    a.only(&["--studio", "--token"], "ganbate mcp")?;
    if a.words.len() != 2 {
        return Err(error::usage(
            "usage: ganbate mcp PROJECT [--studio http://127.0.0.1:PORT] [--token TOKEN]",
        ));
    }
    let project = &a.words[1];
    let studio_url = a.value("--studio")?;
    let token = a.value("--token")?;
    let mut buf = Vec::new();
    serve_mcp_stream(project, studio_url, token, stdin, &mut buf)?;
    let mut stdout = String::from_utf8_lossy(&buf).into_owned();
    if stdout.ends_with('\n') {
        stdout.pop();
    }
    Ok(Outcome { status: 0, stdout })
}

fn resolve_graph_project<'a>(
    a: &'a Args,
    rest: &[&'a str],
    expected_after: usize,
    usage_str: &str,
) -> Result<(&'a str, Vec<&'a str>), Error> {
    if let Some(proj) = a.value("--project")? {
        if rest.len() != expected_after {
            return Err(error::usage(usage_str));
        }
        Ok((proj, rest.to_vec()))
    } else if rest.len() == expected_after {
        Ok((".", rest.to_vec()))
    } else if rest.len() == expected_after + 1 {
        Ok((rest[0], rest[1..].to_vec()))
    } else {
        Err(error::usage(usage_str))
    }
}

fn graph_outcome_reply(outcome: world_graph::WorldGraphMutationOutcome) -> Reply {
    let notices: Vec<J> = outcome
        .guardrail_notices
        .iter()
        .map(|n| J::from(n.as_str()))
        .collect();
    let summary = if outcome.blocked_by_authority {
        outcome
            .guardrail_notices
            .first()
            .cloned()
            .unwrap_or_else(|| {
                "blocked by server-authoritative-readonly security guardrail".to_string()
            })
    } else if outcome.applied {
        let keys = match &outcome.patch {
            Value::Object(m) if !m.is_empty() => m.keys().cloned().collect::<Vec<_>>().join(", "),
            _ => "world.semantic.json".to_string(),
        };
        format!(
            "updated {} ({keys}; {}Hz / {:.2}ms)",
            world_graph::WORLD_GRAPH_URI,
            outcome.graph.guardrails.target_hz,
            outcome.graph.guardrails.frame_budget_ms
        )
    } else {
        outcome
            .guardrail_notices
            .first()
            .cloned()
            .unwrap_or_else(|| "no world graph changes applied".to_string())
    };
    let body = Obj::new()
        .with("uri", world_graph::WORLD_GRAPH_URI)
        .with("path", world_graph::WORLD_GRAPH_REL_PATH)
        .with("applied", outcome.applied)
        .with("blocked_by_authority", outcome.blocked_by_authority)
        .with("summary", summary.as_str())
        .with("guardrail_notices", J::Arr(notices))
        .with("patch", outcome.patch)
        .with("estimated_tokens", outcome.graph.estimated_tokens())
        .with("graph", outcome.graph.to_json_value());
    if outcome.blocked_by_authority {
        Reply {
            body,
            error: Some(error::bad_edit(summary)),
        }
    } else {
        Reply::ok(body)
    }
}

/// Outputs canonical `world.semantic.json` (`ganbate://context`) for `ganbate graph fetch`.
pub fn run_graph_fetch(a: &Args) -> Result<Outcome, Error> {
    a.only(&["--project"], "ganbate graph fetch")?;
    let rest: Vec<&str> = a.words.iter().skip(2).map(String::as_str).collect();
    let (project, _) =
        resolve_graph_project(a, &rest, 0, "usage: ganbate graph fetch [--project <path>]")?;
    let ws = workspace::open(project)?;
    let (graph, _) =
        world_graph::load_or_init_world_graph(&ws.project.root).map_err(error::invalid)?;
    let mut canonical = graph.to_canonical_json().map_err(error::invalid)?;
    if canonical.ends_with('\n') {
        canonical.pop();
    }
    Ok(Outcome {
        status: 0,
        stdout: canonical,
    })
}

/// Compiles a bilingual (EN/KO) natural-language prompt and persists to `world.semantic.json`
/// unless blocked by authority guardrails.
pub fn graph_prompt(a: &Args, rest: &[&str]) -> Result<Reply, Error> {
    a.only(&["--project"], "ganbate graph prompt")?;
    let (project, after) = resolve_graph_project(
        a,
        rest,
        1,
        "usage: ganbate graph prompt [--project <path>] \"<natural-language instruction>\"",
    )?;
    let prompt_text = after[0];
    let ws = workspace::open(project)?;
    let (current, _) =
        world_graph::load_or_init_world_graph(&ws.project.root).map_err(error::invalid)?;
    let (outcome, _) = world_graph::compile_world_graph_prompt(&current, prompt_text, &[]);
    if !outcome.blocked_by_authority && outcome.applied {
        world_graph::save_world_graph(&ws.project.root, &outcome.graph).map_err(error::io)?;
    }
    Ok(graph_outcome_reply(outcome))
}

/// Applies a validated JSON patch to `world.semantic.json` unless blocked by authority guardrails.
pub fn graph_patch(a: &Args, rest: &[&str], stdin: &mut dyn Read) -> Result<Reply, Error> {
    a.only(&["--project"], "ganbate graph patch")?;
    let (project, after) = resolve_graph_project(
        a,
        rest,
        1,
        "usage: ganbate graph patch [--project <path>] '<json-patch>'",
    )?;
    let raw_arg = after[0];
    let patch_bytes = if raw_arg == "-" {
        workspace::read_all(stdin, "standard input")?
    } else if raw_arg.trim_start().starts_with('{') {
        raw_arg.as_bytes().to_vec()
    } else if Path::new(raw_arg).is_file() {
        workspace::read_file(Path::new(raw_arg))?
    } else {
        raw_arg.as_bytes().to_vec()
    };
    let patch_val = core_json::parse(&patch_bytes)
        .map_err(|e| error::bad_edit(format!("invalid world graph JSON patch: {e}")))?;
    let ws = workspace::open(project)?;
    let (current, _) =
        world_graph::load_or_init_world_graph(&ws.project.root).map_err(error::invalid)?;
    let outcome =
        world_graph::apply_world_graph_patch(&current, &patch_val).map_err(error::bad_edit)?;
    if !outcome.blocked_by_authority && outcome.applied {
        world_graph::save_world_graph(&ws.project.root, &outcome.graph).map_err(error::io)?;
    }
    Ok(graph_outcome_reply(outcome))
}

/// Runs the reproducible academic benchmark suite (`ganbate bench [--project <path>] [--iterations <N>] [--format json|latex|markdown]`).
pub fn run_bench(a: &Args, pretty: bool) -> Result<Outcome, Error> {
    const USAGE_STR: &str =
        "usage: ganbate bench [--project <path>] [--iterations <N>] [--format json|latex|markdown]";
    a.only(&["--project", "--iterations", "--format"], "ganbate bench")?;
    let rest: Vec<&str> = a.words.iter().skip(1).map(String::as_str).collect();
    let (project, _) = resolve_graph_project(a, &rest, 0, USAGE_STR)?;

    let project_path = Path::new(project);
    if !project_path.is_dir() {
        return Err(error::not_found(format!(
            "benchmark directory not found: {project}"
        )));
    }

    let iterations = match a.value("--iterations")? {
        Some(raw) => raw
            .parse::<usize>()
            .ok()
            .filter(|&n| (1..=bench::MAX_ITERATIONS).contains(&n))
            .ok_or_else(|| {
                error::usage(format!(
                    "--iterations must be an integer in 1..={}",
                    bench::MAX_ITERATIONS
                ))
            })?,
        None => bench::DEFAULT_ITERATIONS,
    };

    let format = match a.value("--format")? {
        Some(raw) => bench::BenchFormat::parse(raw).map_err(error::usage)?,
        None => bench::BenchFormat::Markdown,
    };

    let report = bench::run_benchmark(project_path, iterations).map_err(error::invalid)?;
    let stdout = report.render(format, pretty).map_err(error::invalid)?;
    Ok(Outcome { status: 0, stdout })
}
