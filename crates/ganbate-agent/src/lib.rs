//! Ganbate's agent-facing command line, `ganbate`: AI agents (and people) inspect, validate, edit,
//! diff, undo and render scenes without touching engine source.
//!
//! Every command prints one line of JSON: `{"ok":true,...}`, or
//! `{"ok":false,"error":{"code":"...","message":"..."}}` with exit code 1 (2 for a wrong
//! command line). `--pretty` indents it. Error codes are stable: `not-found`, `ambiguous-ref`,
//! `bad-edit`, `invalid`, `changed-since`, `io`, `cook`, `render` and `usage`. The same inputs
//! give the same bytes.
//!
//! Edits go through the same machinery as Ganbate Studio's edit protocol: `ganbate_scene` ops, the
//! studio's validation rule (an edit may not add a problem the document did not have) and the
//! project's canonical writer, so a document edited here reads exactly as if it had been
//! edited and saved in the studio. Each written transaction is recorded with its inverse in
//! `PROJECT/.ganbate/history/<scene id>.jsonl` for `ganbate scene undo`.
//!
//! [`run`] is the whole command line as a function (the binary adds the GPU renderer as a
//! [`render::Backend`]).
use std::io::Read;
use std::path::Path;

use ganbate_core::json::{Value, to_canonical};
use ganbate_scene::doc::Document;
use ganbate_scene::scene;

mod args;
mod diff;
mod edit;
pub mod error;
mod history;
mod inspect;
pub mod json;
mod refs;
pub mod render;
mod validate;
mod workspace;

pub use error::Error;
use json::{J, Obj};
use refs::Tree;
use workspace::{about, sha256};

/// What a command printed and how it exits.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub status: i32,
    pub stdout: String,
}

/// A command's answer; a failure may still report (validation's issues).
pub(crate) struct Reply {
    pub body: Obj,
    pub error: Option<Error>,
}

impl Reply {
    pub fn ok(body: Obj) -> Self {
        Self { body, error: None }
    }
}

/// The commands, their operands and options, and what they do.
const COMMANDS: [(&str, &str, &str); 7] = [
    (
        "scene inspect",
        "ganbate scene inspect PROJECT SCENE [--entity REF] [--component KIND]... [--depth N] [--defaults]",
        "entities in tree order: id, name, path, parent, depth, children, components",
    ),
    (
        "scene validate",
        "ganbate scene validate PROJECT SCENE",
        "issues against the project's schemas; exit 1 when any is an error",
    ),
    (
        "scene apply",
        "ganbate scene apply PROJECT SCENE EDITS|- [--dry-run]",
        "apply edits (set, remove, create, delete, reparent, duplicate) as one transaction",
    ),
    (
        "scene undo",
        "ganbate scene undo PROJECT SCENE [--dry-run]",
        "take back the last transaction ganbate recorded, if the document is as it left it",
    ),
    (
        "scene diff",
        "ganbate scene diff A|- B|-",
        "semantic diff of two scene documents by entity id",
    ),
    (
        "render",
        "ganbate render PROJECT SCENE --out FILE.png [--camera REF] [--size WxH] [--time H] [--weather W] [--samples N]",
        "cook the scene in memory and render a still with the offscreen renderer",
    ),
    ("help", "ganbate help", "this list"),
];

fn commands_json() -> J {
    J::Arr(
        COMMANDS
            .iter()
            .map(|(_, usage, does)| Obj::new().with("usage", *usage).with("does", *does).into())
            .collect(),
    )
}

/// Runs one command line (`args` without the program name). `stdin` feeds `EDITS` or a diff
/// side given as `-`; `backend` draws `ganbate render` (without one, rendering fails).
pub fn run(
    args: &[String],
    stdin: &mut dyn Read,
    backend: Option<&dyn render::Backend>,
) -> Outcome {
    let pretty = args.iter().any(|a| a == "--pretty");
    match dispatch(args, stdin, backend) {
        Ok(reply) => answer(reply.body, reply.error, pretty),
        Err(e) => answer(Obj::new(), Some(e), pretty),
    }
}

/// The answer for a failure found before [`run`] could start (the binary's own checks).
pub fn failure(error: Error, pretty: bool) -> Outcome {
    answer(Obj::new(), Some(error), pretty)
}

fn answer(body: Obj, error: Option<Error>, pretty: bool) -> Outcome {
    let status = match &error {
        None => 0,
        Some(e) if e.code == error::USAGE => 2,
        Some(_) => 1,
    };
    let mut out = Obj::new().with("ok", error.is_none());
    if let Some(e) = &error {
        out.put("error", e.json());
    }
    out.extend(body);
    Outcome {
        status,
        stdout: json::write(&out.into(), pretty),
    }
}

fn operands<'a, const N: usize>(rest: &[&'a str], command: &str) -> Result<[&'a str; N], Error> {
    <[&str; N]>::try_from(rest).map_err(|_| {
        let usage = COMMANDS
            .iter()
            .find(|(c, _, _)| *c == command)
            .map_or("", |(_, u, _)| u);
        error::usage(format!("usage: {usage}"))
    })
}

fn dispatch(
    args: &[String],
    stdin: &mut dyn Read,
    backend: Option<&dyn render::Backend>,
) -> Result<Reply, Error> {
    let a = args::parse(args)?;
    let words: Vec<&str> = a.words.iter().map(String::as_str).collect();
    if a.switch("--help") || words.first() == Some(&"help") {
        return Ok(help());
    }
    match words.as_slice() {
        ["scene", "inspect", rest @ ..] => {
            let [project, scene] = operands(rest, "scene inspect")?;
            a.only(
                &["--entity", "--component", "--depth", "--defaults"],
                "ganbate scene inspect",
            )?;
            let mut ws = workspace::open(project)?;
            let at = ws.locate(scene)?;
            let file = ws.read(&at)?;
            inspect::run(&ws, &at, &file, &a)
        }
        ["scene", "validate", rest @ ..] => {
            let [project, scene] = operands(rest, "scene validate")?;
            a.only(&[], "ganbate scene validate")?;
            let mut ws = workspace::open(project)?;
            let at = ws.locate(scene)?;
            ws.index()?;
            let file = ws.read(&at)?;
            Ok(validate::run(&ws, &at, &file))
        }
        ["scene", "apply", rest @ ..] => {
            let [project, scene, edits] = operands(rest, "scene apply")?;
            a.only(&["--dry-run"], "ganbate scene apply")?;
            apply(project, scene, edits, a.switch("--dry-run"), stdin)
        }
        ["scene", "undo", rest @ ..] => {
            let [project, scene] = operands(rest, "scene undo")?;
            a.only(&["--dry-run"], "ganbate scene undo")?;
            undo(project, scene, a.switch("--dry-run"))
        }
        ["scene", "diff", rest @ ..] => {
            let [x, y] = operands(rest, "scene diff")?;
            a.only(&[], "ganbate scene diff")?;
            diff(x, y, stdin)
        }
        ["render", rest @ ..] => {
            let [project, scene] = operands(rest, "render")?;
            a.only(
                &[
                    "--out",
                    "--camera",
                    "--size",
                    "--time",
                    "--weather",
                    "--samples",
                ],
                "ganbate render",
            )?;
            let mut ws = workspace::open(project)?;
            let at = ws.locate(scene)?;
            render::run(&mut ws, &at, &a, backend)
        }
        [] => Err(error::usage("no command; try ganbate help").with("commands", commands_json())),
        _ => Err(
            error::usage(format!("unknown command {:?}", words.join(" ")))
                .with("commands", commands_json()),
        ),
    }
}

fn help() -> Reply {
    Reply::ok(
        Obj::new()
            .with("version", env!("CARGO_PKG_VERSION"))
            .with("commands", commands_json())
            .with(
                "refs",
                "REF: an entity's 16-hex id, its unique name, or a /-separated path of names from a root",
            )
            .with(
                "edits",
                "EDITS: {\"edits\":[{\"op\":\"set\",\"entity\":REF,\"path\":\"components.light.intensity\",\"value\":2}, remove {entity,path}, create {name,parent?,components?,id?}, delete {entity}, reparent {entity,parent|null}, duplicate {entity,name?}]}",
            )
            .with(
                "codes",
                J::Arr(error::CODES.iter().map(|c| J::from(*c)).collect()),
            )
            .with("options", "--pretty indents the answer"),
    )
}

fn canonical(v: &Value) -> Result<String, Error> {
    to_canonical(v).map_err(error::invalid)
}

fn apply(
    project: &str,
    scene_arg: &str,
    edits_arg: &str,
    dry_run: bool,
    stdin: &mut dyn Read,
) -> Result<Reply, Error> {
    let bytes = if edits_arg == "-" {
        workspace::read_all(stdin, "standard input")?
    } else {
        workspace::read_file(Path::new(edits_arg))?
    };
    let (label, list) = edit::parse(&bytes)?;
    if list.is_empty() {
        return Err(error::bad_edit("EDITS has no edits"));
    }
    let mut ws = workspace::open(project)?;
    let at = ws.locate(scene_arg)?;
    let file = ws.read(&at)?;
    let applied = edit::run(
        &file.root,
        &list,
        &ws.registry,
        &|id| ws.prefab(id),
        edit::seed(&file.root, &list),
    )?;
    let old_text = canonical(&file.root)?;
    let text = canonical(&applied.root)?;
    let mut hash = sha256(text.as_bytes());
    let written = !dry_run && text != old_text;
    if written {
        // Only now, with a transaction that holds, is anything under the project touched.
        let _lock = history::Lock::take(&ws.project.root, &at.id)?;
        if workspace::read_file(&at.path)? != file.bytes {
            return Err(error::changed(format!(
                "{} changed while the edits were applied; run them again",
                at.rel
            )));
        }
        let history_file = history::file(&ws.project.root, &at.id);
        let mut lines = history::read(&history_file)?;
        ws.project
            .write_document(&at.rel, at.kind, &applied.root)
            .map_err(error::io)?;
        if let Ok(on_disk) = workspace::read_file(&at.path) {
            hash = sha256(&on_disk);
        }
        let hand_written = old_text.as_bytes() != file.bytes.as_slice();
        lines.push(
            history::Record {
                label: label.unwrap_or_else(|| edit::label(&list)),
                edits: list.len(),
                before: sha256(&file.bytes),
                after: hash.clone(),
                ops: applied.ops.clone(),
                undo: applied.undo.clone(),
                text: hand_written.then(|| String::from_utf8_lossy(&file.bytes).into_owned()),
            }
            .line(),
        );
        if let Err(e) = history::write(&history_file, &lines) {
            // Without its record the change could not be undone: put the document back.
            let _ = ganbate_assets::write_asset(&at.path, &file.bytes);
            return Err(e);
        }
    }
    let tree = Tree::new(&applied.root);
    let issues: Vec<J> = scene::check(&applied.root, &ws.registry)
        .iter()
        .map(|i| validate::issue(&tree, Some("error"), &i.path, &i.message).into())
        .collect();
    let created: Vec<J> = applied
        .created
        .iter()
        .map(|(i, id)| {
            Obj::new()
                .with("edit", *i)
                .with("entity", id.as_str())
                .with("name", tree.name(id))
                .into()
        })
        .collect();
    let body = Obj::new()
        .with("scene", about(&at, &applied.root))
        .with("dry_run", dry_run)
        .with("written", written)
        .with("edits", list.len())
        .with(
            "changes",
            J::Arr(applied.changes.iter().map(edit::Change::json).collect()),
        )
        .with("created", created)
        .with("issues", issues)
        .with("hash", hash);
    Ok(Reply::ok(body))
}

fn undo(project: &str, scene_arg: &str, dry_run: bool) -> Result<Reply, Error> {
    let mut ws = workspace::open(project)?;
    let at = ws.locate(scene_arg)?;
    let history_file = history::file(&ws.project.root, &at.id);
    let mut lines = history::read(&history_file)?;
    let all = lines.clone();
    let Some(last) = lines.pop() else {
        return Err(error::not_found(format!(
            "nothing to undo: ganbate has recorded no transaction for {}",
            at.rel
        )));
    };
    let record = history::Record::parse(&last).map_err(|e| {
        error::invalid(format!(
            "{}: the last transaction is damaged ({e})",
            history_file.display()
        ))
    })?;
    let bytes = workspace::read_file(&at.path)?;
    let found = sha256(&bytes);
    if found != record.after {
        return Err(error::changed(format!(
            "{} changed since ganbate recorded {:?}; undo refused",
            at.rel, record.label
        ))
        .with("expected", record.after.as_str())
        .with("found", found));
    }
    let root = workspace::parse_document(&bytes, &at.rel)?;
    let mut doc = Document::new(root).map_err(error::invalid)?;
    let mut changes = vec![];
    edit::apply_ops(&mut doc, &record.undo, None, &mut changes)
        .map_err(|e| error::invalid(format!("the recorded inverse does not apply: {e}")))?;
    let restored = canonical(doc.root())?;
    // A hand-written original comes back as it was written.
    let original = record.text.as_ref().filter(|t| {
        ganbate_core::json::parse(t.as_bytes())
            .ok()
            .and_then(|v| to_canonical(&v).ok())
            .is_some_and(|c| c == restored)
    });
    let text = original.cloned().unwrap_or(restored);
    let hash = sha256(text.as_bytes());
    if !dry_run {
        let _lock = history::Lock::take(&ws.project.root, &at.id)?;
        if workspace::read_file(&at.path)? != bytes || history::read(&history_file)? != all {
            return Err(error::changed(format!(
                "{} or its history changed during the undo; run it again",
                at.rel
            )));
        }
        let wrote = match original {
            Some(_) => ganbate_assets::write_asset(&at.path, text.as_bytes())
                .map_err(|e| format!("{}: {e}", at.rel)),
            None => ws.project.write_document(&at.rel, at.kind, doc.root()),
        };
        wrote.map_err(error::io)?;
        if let Err(e) = history::write(&history_file, &lines) {
            let _ = ganbate_assets::write_asset(&at.path, &bytes);
            return Err(e);
        }
    }
    let body = Obj::new()
        .with("scene", about(&at, doc.root()))
        .with("dry_run", dry_run)
        .with("written", !dry_run)
        .with(
            "undone",
            Obj::new()
                .with("label", record.label.as_str())
                .with("edits", record.edits),
        )
        .with(
            "changes",
            J::Arr(changes.iter().map(edit::Change::json).collect()),
        )
        .with("restored", hash == record.before)
        .with("remaining", lines.len())
        .with("hash", hash);
    Ok(Reply::ok(body))
}

fn read_side(arg: &str, stdin: &mut dyn Read) -> Result<Value, Error> {
    let bytes = if arg == "-" {
        workspace::read_all(stdin, "standard input")?
    } else {
        workspace::read_file(Path::new(arg))?
    };
    workspace::parse_document(&bytes, arg)
}

fn diff(a: &str, b: &str, stdin: &mut dyn Read) -> Result<Reply, Error> {
    if a == "-" && b == "-" {
        return Err(error::usage(
            "only one of A and B can be - (standard input)",
        ));
    }
    let (va, vb) = (read_side(a, stdin)?, read_side(b, stdin)?);
    let side = |file: &str, v: &Value| -> J {
        Obj::new()
            .with("file", file)
            .with("id", v.get("id"))
            .with("name", v.get("name"))
            .into()
    };
    let mut body = Obj::new().with("a", side(a, &va)).with("b", side(b, &vb));
    body.extend(diff::diff(&va, &vb));
    Ok(Reply::ok(body))
}
