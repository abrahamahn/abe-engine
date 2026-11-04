//! The studio's state and the edit protocol. Every message is a JSON object with a `type`;
//! `Studio::handle` takes one from a client and returns the messages to send (to that client
//! and to others watching the same document), so the protocol is tested without sockets.
//!
//! | Client sends | Host answers |
//! | --- | --- |
//! | `hello {client}` | `welcome {project, schemas, assets}` |
//! | `rescan` | all clients receive the updated assets, a scan report, and refreshed documents they watch; on a failed scan, the last valid index stays in use |
//! | `open {doc}` | `doc {doc, rev, path, kind, root, dirty, undo, redo, issues}` (a document open with no unsaved edits is read again if its file changed on disk, and everyone watching it gets the new `doc`) |
//! | `close {doc}` | nothing |
//! | `edit {doc, rev, label, merge?, ops, req?}` | everyone watching: `ops {doc, rev, ops, label, by, req}`; on failure the sender gets `error {req, message}` (and `doc` when its revision was stale) |
//! | `delete {doc, rev, ids, label?, req?}` | as `edit`, with the ops that remove `ids` and everything beneath them, including what was added under the entities a deleted prefab instance shows |
//! | `undo {doc}` / `redo {doc}` | everyone watching: `ops ...` |
//! | `save {doc, force?}` | everyone watching: `saved {doc, rev}`; if the file changed on disk since it was opened or saved, the sender gets `error {code: "changed-on-disk"}` instead, unless `force` |
//! | `ids {count}` | `ids {ids}`: fresh entity ids |
//! | `create {kind, path, name}` | `created {doc, path}` and everyone: `assets` |
//! | `import {model, path, name}` | `created {doc, path}` and everyone: `assets` |
//! | `asset-usage {id?, req?}` | `asset-usage {id, uses, req}` or project-wide `{counts, req}` |
//! | `asset-move {id, to, req?}` | `asset-moved {id, kind, from, to, req}`, everyone: `assets`, watchers: `doc` |
//! | `asset-import {path, dataBase64, req?}` | `asset-imported {id, kind, path, req}` and everyone: `assets` |
//! | `cook {doc}` | `cooked {doc, rev, file, entities, warnings}` (the open, unsaved document) |
//! | `preview {doc, ...}` / `select {doc, ...}` | relayed to the others watching `doc` |
//!
//! An op is `{"set": [path...], "value": v}` or `{"remove": [path...]}`. In scenes and prefabs
//! only `entities` and `name` may change; an edit that adds a validation problem (including
//! the problems only expanding its prefabs shows, such as a parent cycle through what an
//! instance shows) is refused.
mod assets;
mod rescan;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use ganbate_core::id::{Ids, is_asset_id};
use ganbate_core::json::{self, Value};
use ganbate_pipeline::cook;
use ganbate_pipeline::project::{Kind, Project};
use ganbate_scene::doc::{Document, History, Op};
use ganbate_scene::scene;
use ganbate_scene::schema::{Registry, engine_registry};

pub type ClientId = u64;
/// Messages to send: to whom, and what.
pub type Outbox = Vec<(ClientId, Value)>;

const HISTORY_LIMIT: usize = 512;
const MAX_IDS: usize = 4096;
/// Deepest folder nesting below a content folder for a new document: the project scan
/// (`ganbate_pipeline::project`) refuses a project with deeper folders, so one more would leave a
/// project that no longer opens.
pub const MAX_FOLDER_DEPTH: usize = 32;
/// Cooked previews kept per document: the latest few, since a viewer may still be loading the
/// one before when the next arrives.
pub const PREVIEWS_KEPT: usize = 4;

struct Open {
    kind: Kind,
    path: String,
    doc: Document,
    history: History,
    watchers: BTreeSet<ClientId>,
    /// Problems the document had before (they never block an edit; new ones do).
    known_issues: BTreeSet<(String, String)>,
    /// The document as last read from or written to its file, to notice another tool (`ganbate`,
    /// git, a second host) changing the file meanwhile.
    on_disk: Value,
    unavailable: Option<String>,
    changed_on_disk: bool,
}

struct CustomOpen {
    kind: &'static str,
    path: String,
    doc: Document,
    history: History,
    watchers: BTreeSet<ClientId>,
    on_disk: Value,
    changed_on_disk: bool,
}

const CUSTOM_DOC_SUFFIXES: [(&str, &str); 4] = [
    (".soundbank.json", "soundbank"),
    (".character.json", "character"),
    (".seq.json", "seq"),
    (".vfx.json", "vfx"),
];

pub fn custom_kind_of_path(path: &str) -> Option<&'static str> {
    let lower = path.to_ascii_lowercase();
    CUSTOM_DOC_SUFFIXES
        .iter()
        .find_map(|(suf, kind)| lower.ends_with(suf).then_some(*kind))
}

pub fn custom_doc_id(rel_path: &str) -> String {
    let digest = ganbate_core::digest::sha256(format!("custom:{rel_path}").as_bytes());
    ganbate_core::digest::hex(&digest)[..ganbate_core::id::ASSET_ID_LEN].to_string()
}

pub struct Studio {
    pub project: Project,
    pub registry: Registry,
    ids: Ids,
    open: BTreeMap<String, Open>,
    custom_open: BTreeMap<String, CustomOpen>,
    clients: BTreeMap<ClientId, String>,
    next_client: ClientId,
    /// Live `@selection` reported per Studio client (`selection-sync`), plus the most recently active client.
    selections: BTreeMap<ClientId, crate::ai::SelectionContext>,
    active_client: Option<ClientId>,
    /// Entities touched by the latest in-memory transaction per document (for `brick crystallize --from-tx`).
    last_tx_entities: BTreeMap<String, Vec<String>>,
    /// Where cooked previews are written (inside the project's `.ganbate` folder).
    cache_rel: String,
    /// The project folder with every symlink resolved: whatever the host reads or writes in
    /// the project must resolve inside it.
    real_root: PathBuf,
    /// Each document's latest cooked previews (file names in the cache folder), oldest first.
    previews: BTreeMap<String, VecDeque<String>>,
}

fn obj(pairs: Vec<(&str, Value)>) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}
fn s(v: &str) -> Value {
    Value::String(v.into())
}
fn n(v: u64) -> Value {
    Value::Number(v as f64)
}
fn error(req: Option<&Value>, message: &str) -> Value {
    let mut m = vec![("type", s("error")), ("message", s(message))];
    if let Some(r) = req {
        m.push(("req", r.clone()));
    }
    obj(m)
}

/// The refusal a save gets when its file changed on disk since the host read or wrote it.
const CHANGED_ON_DISK: &str = "changed-on-disk";

pub fn op_from_json(v: &Value) -> Result<Op, String> {
    let path = |p: &Value| -> Result<Vec<String>, String> {
        let a = p.array()?;
        if a.is_empty() || a.len() > 64 {
            return Err("an op's path has 1 to 64 keys".into());
        }
        a.iter().map(|k| k.string().map(String::from)).collect()
    };
    match (v.get("set"), v.get("remove")) {
        (Some(p), None) => Ok(Op::Set {
            path: path(p)?,
            value: v.get("value").cloned().ok_or("a set op needs a value")?,
        }),
        (None, Some(p)) => Ok(Op::Remove { path: path(p)? }),
        _ => Err("an op is {\"set\": path, \"value\": v} or {\"remove\": path}".into()),
    }
}

pub fn op_to_json(op: &Op) -> Value {
    let path = |p: &[String]| Value::Array(p.iter().map(|k| s(k)).collect());
    match op {
        Op::Set { path: p, value } => obj(vec![("set", path(p)), ("value", value.clone())]),
        Op::Remove { path: p } => obj(vec![("remove", path(p))]),
    }
}

/// A Look uses the renderer's format, with a sidecar id and optional sections.
fn validate_look(root: &Value) -> Result<(), String> {
    for section in [
        "shading", "post", "sky", "grass", "terrain", "time", "weather",
    ] {
        if root
            .get(section)
            .is_some_and(|v| !matches!(v, Value::Object(_)))
        {
            return Err(format!("{section}: expected an object"));
        }
    }
    ganbate_engine::render::LookFile::from_value(root).map(|_| ())
}

/// Canonical empty `.data.json` document for a registered non-engine `schema`.
pub fn new_data_document(id: &str, schema: &str, name: &str) -> Value {
    Value::Object(BTreeMap::from([
        ("ganbate".into(), Value::String("data".into())),
        ("version".into(), Value::Number(1.0)),
        ("id".into(), Value::String(id.into())),
        ("schema".into(), Value::String(schema.into())),
        ("name".into(), Value::String(name.into())),
        ("values".into(), Value::Object(BTreeMap::new())),
    ]))
}

/// Validates a `Kind::Data` (`*.data.json`) document header and its `values` against `registry`.
pub fn validate_data(root: &Value, registry: &Registry) -> BTreeSet<(String, String)> {
    let mut issues = BTreeSet::new();
    let Value::Object(map) = root else {
        issues.insert(("".into(), "a data document is an object".into()));
        return issues;
    };
    for key in map.keys() {
        if !matches!(
            key.as_str(),
            "ganbate" | "version" | "id" | "schema" | "name" | "values"
        ) {
            issues.insert((key.clone(), format!("unknown data document key {key:?}")));
        }
    }
    if map.get("ganbate").and_then(|v| v.string().ok()) != Some("data") {
        issues.insert(("ganbate".into(), "expected \"ganbate\": \"data\"".into()));
    }
    if map.get("version").and_then(|v| v.unsigned().ok()) != Some(1) {
        issues.insert(("version".into(), "expected \"version\": 1".into()));
    }
    if !map
        .get("id")
        .and_then(|v| v.string().ok())
        .is_some_and(is_asset_id)
    {
        issues.insert(("id".into(), "expected a 16-hex-digit asset id".into()));
    }
    if !map
        .get("name")
        .is_some_and(|v| matches!(v, Value::String(_)))
    {
        issues.insert(("name".into(), "expected a string".into()));
    }
    let valid_schema = match map.get("schema").and_then(|v| v.string().ok()) {
        Some(s) if ganbate_scene::schema::valid_kind_name(s, false) => Some(s),
        Some(s) => {
            issues.insert(("schema".into(), format!("invalid data schema name {s:?}")));
            None
        }
        None => {
            issues.insert(("schema".into(), "expected a schema name".into()));
            None
        }
    };
    let values_obj = match map.get("values") {
        Some(v @ Value::Object(_)) => Some(v),
        _ => {
            issues.insert(("values".into(), "expected an object".into()));
            None
        }
    };
    if let Some(schema) = valid_schema {
        if registry.get(schema).is_some() {
            if let Some(values) = values_obj {
                for issue in registry.check_component(schema, values, "values") {
                    issues.insert((issue.path, issue.message));
                }
            }
        } else {
            issues.insert(("schema".into(), format!("unknown schema {schema:?}")));
        }
    }
    issues
}

/// Every problem of `root`, including what only expanding its prefabs shows (a parent cycle
/// through what an instance shows, a missing or broken prefab).
fn issue_set(
    kind: Kind,
    root: &Value,
    registry: &Registry,
    load: &dyn Fn(&str) -> Option<Value>,
) -> BTreeSet<(String, String)> {
    if kind == Kind::Look {
        return validate_look(root)
            .err()
            .map(|error| ("look".into(), error))
            .into_iter()
            .collect();
    }
    if kind == Kind::Data {
        return validate_data(root, registry);
    }
    if kind == Kind::Material {
        return BTreeSet::new();
    }
    scene::check_with_prefabs(root, registry, load)
        .into_iter()
        .map(|i| (i.path, i.message))
        .collect()
}

/// A prefab for expansion: as edited if it is open, otherwise as saved (and only from inside
/// the project).
fn load_prefab(
    open: &BTreeMap<String, Value>,
    project: &Project,
    real_root: &Path,
    id: &str,
) -> Option<Value> {
    open.get(id).cloned().or_else(|| {
        let e = project.entry(id)?;
        inside(&project.root, real_root, &e.path).ok()?;
        project.read_document(id).ok()
    })
}

impl Studio {
    /// Opens the project in `root`: scans it (writing missing sidecars) and loads the engine's
    /// and the game's component schemas.
    pub fn open(root: &Path, mut ids: Ids) -> Result<(Self, Vec<String>), String> {
        let real_root =
            std::fs::canonicalize(root).map_err(|e| format!("{}: {e}", root.display()))?;
        let mut project = Project::open(root)?;
        let report = project.scan(Some(&mut ids))?;
        let mut registry = engine_registry();
        let mut notes: Vec<String> = report.problems;
        for c in &report.created {
            notes.push(format!("gave {c} an id (wrote its .meta.json)"));
        }
        for (path, file) in project.schema_files()? {
            registry
                .add(&file, false)
                .map_err(|e| format!("{path}: {e}"))?;
        }
        let studio = Self {
            project,
            registry,
            ids,
            open: BTreeMap::new(),
            custom_open: BTreeMap::new(),
            clients: BTreeMap::new(),
            next_client: 1,
            selections: BTreeMap::new(),
            active_client: None,
            last_tx_entities: BTreeMap::new(),
            cache_rel: ".ganbate/cooked".into(),
            real_root,
            previews: BTreeMap::new(),
        };
        // Previews left by an earlier session are never asked for again.
        if studio.inside(&studio.cache_rel).is_ok() {
            sweep_previews(&studio.cache_dir());
        }
        Ok((studio, notes))
    }

    /// The project folder with every symlink resolved.
    pub fn real_root(&self) -> &Path {
        &self.real_root
    }

    /// Refuses `rel` (a path in the project, existing or about to be made) when the part of it
    /// that exists resolves outside the project through a symlink.
    fn inside(&self, rel: &str) -> Result<(), String> {
        inside(&self.project.root, &self.real_root, rel)
    }

    pub fn connect(&mut self, name: &str) -> ClientId {
        let id = self.next_client;
        self.next_client += 1;
        self.clients.insert(id, name.into());
        id
    }

    pub fn disconnect(&mut self, client: ClientId) {
        self.clients.remove(&client);
        self.selections.remove(&client);
        if self.active_client == Some(client) {
            self.active_client = self.selections.keys().next_back().copied();
        }
        for o in self.open.values_mut() {
            o.watchers.remove(&client);
        }
        for o in self.custom_open.values_mut() {
            o.watchers.remove(&client);
        }
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.project.root.join(&self.cache_rel)
    }

    pub(super) fn game_workspaces(&self) -> Value {
        Value::Array(
            self.project
                .manifest
                .editor
                .iter()
                .map(|dir| format!("{dir}/studio-game.json"))
                .filter(|path| self.inside(path).is_ok() && self.project.root.join(path).is_file())
                .map(|path| s(&path))
                .collect(),
        )
    }

    /// Discovers custom workbench JSON documents (`*.soundbank.json`, `*.character.json`,
    /// `*.seq.json`, `*.vfx.json`) under `manifest.content` and `manifest.editor`.
    fn scan_custom_docs(&self) -> BTreeMap<String, (&'static str, String)> {
        fn walk(
            studio: &Studio,
            dir: &Path,
            rel_prefix: &str,
            depth: usize,
            out: &mut BTreeMap<String, (&'static str, String)>,
        ) {
            if depth > MAX_FOLDER_DEPTH {
                return;
            }
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            let mut sorted: Vec<_> = entries.flatten().collect();
            sorted.sort_by_key(|e| e.file_name());
            for entry in sorted {
                let fname = entry.file_name();
                let Some(name) = fname.to_str() else {
                    continue;
                };
                if name.starts_with('.') || name.ends_with(".meta.json") {
                    continue;
                }
                let rel = format!("{rel_prefix}/{name}");
                if studio.inside(&rel).is_err() {
                    continue;
                }
                let full = entry.path();
                let Ok(ft) = entry.file_type() else {
                    continue;
                };
                if ft.is_dir() {
                    walk(studio, &full, &rel, depth + 1, out);
                } else if ft.is_file()
                    && let Some(kind) = custom_kind_of_path(&rel)
                {
                    let id = custom_doc_id(&rel);
                    if studio.project.entry(&id).is_none() {
                        out.insert(id, (kind, rel));
                    }
                }
            }
        }

        let mut out = BTreeMap::new();
        for folder in self
            .project
            .manifest
            .content
            .iter()
            .chain(self.project.manifest.editor.iter())
        {
            if self.inside(folder).is_ok() {
                walk(self, &self.project.root.join(folder), folder, 0, &mut out);
            }
        }
        out
    }

    fn assets(&self) -> Value {
        let mut items: Vec<Value> = self
            .project
            .entries()
            .map(|e| {
                obj(vec![
                    ("id", s(&e.id)),
                    ("kind", s(e.kind.name())),
                    ("path", s(&e.path)),
                    ("bytes", n(self.asset_bytes(&e.path))),
                ])
            })
            .collect();
        for (id, (kind, rel)) in self.scan_custom_docs() {
            items.push(obj(vec![
                ("id", s(&id)),
                ("kind", s(kind)),
                ("path", s(&rel)),
                ("bytes", n(self.asset_bytes(&rel))),
            ]));
        }
        Value::Array(items)
    }

    fn custom_doc_message(&self, id: &str) -> Value {
        let o = &self.custom_open[id];
        let label = |l: Option<&str>| l.map_or(Value::Null, s);
        obj(vec![
            ("type", s("doc")),
            ("doc", s(id)),
            ("rev", n(o.doc.revision())),
            ("path", s(&o.path)),
            ("kind", s(o.kind)),
            ("root", o.doc.root().clone()),
            ("dirty", Value::Bool(o.history.dirty())),
            ("unavailable", Value::Null),
            ("changedOnDisk", Value::Bool(o.changed_on_disk)),
            ("undo", label(o.history.can_undo())),
            ("redo", label(o.history.can_redo())),
            ("issues", Value::Array(Vec::new())),
        ])
    }

    fn broadcast_custom_ops(
        &self,
        id: &str,
        ops: &[Op],
        label: &str,
        by: ClientId,
        req: Option<&Value>,
    ) -> Outbox {
        let o = &self.custom_open[id];
        let label_or = |l: Option<&str>| l.map_or(Value::Null, s);
        let mut m = vec![
            ("type", s("ops")),
            ("doc", s(id)),
            ("rev", n(o.doc.revision())),
            ("label", s(label)),
            ("ops", Value::Array(ops.iter().map(op_to_json).collect())),
            ("by", n(by)),
            ("dirty", Value::Bool(o.history.dirty())),
            ("undo", label_or(o.history.can_undo())),
            ("redo", label_or(o.history.can_redo())),
            ("issues", Value::Array(Vec::new())),
        ];
        if let Some(r) = req {
            m.push(("req", r.clone()));
        }
        let msg = obj(m);
        o.watchers.iter().map(|w| (*w, msg.clone())).collect()
    }

    fn doc_message(&self, id: &str) -> Value {
        let o = &self.open[id];
        let label = |l: Option<&str>| l.map_or(Value::Null, s);
        let open = self.open_prefabs();
        let load = |a: &str| load_prefab(&open, &self.project, &self.real_root, a);
        let issues = issue_set(o.kind, o.doc.root(), &self.registry, &load);
        obj(vec![
            ("type", s("doc")),
            ("doc", s(id)),
            ("rev", n(o.doc.revision())),
            ("path", s(&o.path)),
            ("kind", s(o.kind.name())),
            ("root", o.doc.root().clone()),
            ("dirty", Value::Bool(o.history.dirty())),
            (
                "unavailable",
                o.unavailable.as_deref().map_or(Value::Null, s),
            ),
            ("changedOnDisk", Value::Bool(o.changed_on_disk)),
            ("undo", label(o.history.can_undo())),
            ("redo", label(o.history.can_redo())),
            (
                "issues",
                Value::Array(
                    issues
                        .iter()
                        .map(|(path, message)| {
                            obj(vec![("path", s(path)), ("message", s(message))])
                        })
                        .collect(),
                ),
            ),
        ])
    }

    /// The state messages after a change: `ops` to every watcher.
    fn broadcast_ops(
        &self,
        id: &str,
        ops: &[Op],
        label: &str,
        by: ClientId,
        req: Option<&Value>,
    ) -> Outbox {
        let o = &self.open[id];
        let label_or = |l: Option<&str>| l.map_or(Value::Null, s);
        let mut m = vec![
            ("type", s("ops")),
            ("doc", s(id)),
            ("rev", n(o.doc.revision())),
            ("label", s(label)),
            ("ops", Value::Array(ops.iter().map(op_to_json).collect())),
            ("by", n(by)),
            ("dirty", Value::Bool(o.history.dirty())),
            ("undo", label_or(o.history.can_undo())),
            ("redo", label_or(o.history.can_redo())),
        ];
        if matches!(o.kind, Kind::Look | Kind::Data | Kind::Material) {
            m.push((
                "issues",
                Value::Array(
                    o.known_issues
                        .iter()
                        .map(|(path, message)| {
                            obj(vec![("path", s(path)), ("message", s(message))])
                        })
                        .collect(),
                ),
            ));
        }
        if let Some(r) = req {
            m.push(("req", r.clone()));
        }
        let msg = obj(m);
        o.watchers.iter().map(|w| (*w, msg.clone())).collect()
    }

    /// The open prefabs as edited, for expanding instances.
    fn open_prefabs(&self) -> BTreeMap<String, Value> {
        self.open
            .iter()
            .filter(|(_, o)| o.kind == Kind::Prefab && o.unavailable.is_none())
            .map(|(k, o)| (k.clone(), o.doc.root().clone()))
            .collect()
    }

    fn read_custom_json(&self, rel: &str) -> Result<Value, String> {
        self.inside(rel)?;
        let bytes = ganbate_assets::read_asset(&self.project.root.join(rel))?;
        json::parse(&bytes).map_err(|e| format!("{rel}: {e}"))
    }

    fn ensure_custom_open(&mut self, id: &str, hint_path: Option<&str>) -> Result<bool, String> {
        if let Some(o) = self.custom_open.get(id) {
            if o.history.dirty() {
                return Ok(false);
            }
            let disk = self.read_custom_json(&o.path)?;
            if disk == o.on_disk {
                return Ok(false);
            }
            let o = self.custom_open.get_mut(id).expect("custom_open");
            o.doc.reload(disk.clone())?;
            o.history = History::new(HISTORY_LIMIT);
            o.on_disk = disk;
            o.changed_on_disk = false;
            return Ok(true);
        }
        let scanned = self.scan_custom_docs();
        let (kind, rel) = if let Some((k, p)) = scanned.get(id) {
            (*k, p.clone())
        } else if let Some(p) = hint_path
            && let Some(k) = custom_kind_of_path(p)
            && custom_doc_id(p) == id
            && self.inside(p).is_ok()
            && self.project.root.join(p).is_file()
        {
            (k, p.to_string())
        } else {
            return Err(format!("no asset {id}"));
        };
        let root = self.read_custom_json(&rel)?;
        self.custom_open.insert(
            id.to_string(),
            CustomOpen {
                kind,
                path: rel,
                doc: Document::new(root.clone())?,
                history: History::new(HISTORY_LIMIT),
                watchers: BTreeSet::new(),
                on_disk: root,
                changed_on_disk: false,
            },
        );
        Ok(false)
    }

    /// Opens `id` if it is not open yet. A document already open with no unsaved edits is read
    /// again when its file changed on disk (another tool edited it); returns whether it was.
    fn ensure_open(&mut self, id: &str) -> Result<bool, String> {
        if let Some(o) = self.open.get(id) {
            if o.history.dirty() || o.unavailable.is_some() {
                return Ok(false);
            }
            self.inside(&o.path)?;
            let disk = self.project.read_document(id)?;
            if disk == o.on_disk {
                return Ok(false);
            }
            let open = self.open_prefabs();
            let load = |a: &str| load_prefab(&open, &self.project, &self.real_root, a);
            let known_issues = issue_set(o.kind, &disk, &self.registry, &load);
            let o = self.open.get_mut(id).expect("open");
            o.doc.reload(disk.clone())?;
            o.history = History::new(HISTORY_LIMIT);
            o.known_issues = known_issues;
            o.on_disk = disk;
            o.changed_on_disk = false;
            return Ok(true);
        }
        let entry = self
            .project
            .entry(id)
            .ok_or_else(|| format!("no asset {id}"))?
            .clone();
        if !matches!(
            entry.kind,
            Kind::Scene | Kind::Prefab | Kind::Look | Kind::Data | Kind::Material
        ) {
            return Err(format!("{} is not an editable document", entry.path));
        }
        self.inside(&entry.path)?;
        let root = self.project.read_document(id)?;
        let open = self.open_prefabs();
        let load = |a: &str| load_prefab(&open, &self.project, &self.real_root, a);
        let known_issues = issue_set(entry.kind, &root, &self.registry, &load);
        self.open.insert(
            id.to_string(),
            Open {
                kind: entry.kind,
                path: entry.path,
                doc: Document::new(root.clone())?,
                history: History::new(HISTORY_LIMIT),
                watchers: BTreeSet::new(),
                known_issues,
                on_disk: root,
                unavailable: None,
                changed_on_disk: false,
            },
        );
        Ok(false)
    }

    /// Handles one message from `from`; returns what to send.
    pub fn handle(&mut self, from: ClientId, msg: &Value) -> Outbox {
        let req = msg.get("req");
        match self.dispatch(from, msg) {
            Ok(out) => out,
            Err(e) => vec![(from, error(req, &e))],
        }
    }

    fn resolve_doc_ref(&self, raw: &str) -> Result<(String, Option<String>), String> {
        let trimmed = raw.trim();
        if is_asset_id(trimmed) {
            return Ok((trimmed.to_string(), None));
        }
        if let Some(entry) = self.project.find_path(trimmed) {
            return Ok((entry.id.clone(), Some(entry.path.clone())));
        }
        if custom_kind_of_path(trimmed).is_some() {
            return Ok((custom_doc_id(trimmed), Some(trimmed.to_string())));
        }
        Err("missing or invalid doc id".into())
    }

    fn doc_target(&self, msg: &Value) -> Result<(String, Option<String>), String> {
        if let Some(d) = msg.get("doc").and_then(|v| v.string().ok())
            && !d.trim().is_empty()
        {
            let mut res = self.resolve_doc_ref(d)?;
            if res.1.is_none()
                && let Some(p) = msg.get("path").and_then(|v| v.string().ok())
                && !p.trim().is_empty()
            {
                res.1 = Some(p.to_string());
            }
            return Ok(res);
        }
        if let Some(p) = msg.get("path").and_then(|v| v.string().ok())
            && !p.trim().is_empty()
        {
            return self.resolve_doc_ref(p);
        }
        Err("missing or invalid doc id".into())
    }

    fn resolve_active_or_explicit_doc(
        &self,
        msg: &Value,
        client_hint: Option<ClientId>,
    ) -> Result<String, String> {
        if (msg
            .get("doc")
            .and_then(|v| v.string().ok())
            .is_some_and(|s| !s.trim().is_empty()))
            || (msg
                .get("path")
                .and_then(|v| v.string().ok())
                .is_some_and(|s| !s.trim().is_empty()))
        {
            return self.doc_target(msg).map(|(id, _)| id);
        }
        if let Some(cid) = client_hint.or(self.active_client)
            && let Some(sel) = self.selections.get(&cid)
            && let Some(doc_ref) = &sel.doc
            && let Ok((id, _)) = self.resolve_doc_ref(doc_ref)
        {
            return Ok(id);
        }
        if let Some((id, _)) = self
            .open
            .iter()
            .find(|(_, o)| matches!(o.kind, Kind::Scene | Kind::Prefab) && o.unavailable.is_none())
            .or_else(|| self.open.iter().find(|(_, o)| o.unavailable.is_none()))
        {
            return Ok(id.clone());
        }
        if let Some((id, _)) = self.custom_open.iter().next() {
            return Ok(id.clone());
        }
        if let Some(startup) = &self.project.manifest.startup
            && self.project.entry(startup).is_some()
        {
            return Ok(startup.clone());
        }
        let scenes: Vec<_> = self
            .project
            .entries()
            .filter(|e| e.kind == Kind::Scene)
            .collect();
        if scenes.len() == 1 {
            return Ok(scenes[0].id.clone());
        }
        Err("specify doc or open a document in Studio first".into())
    }

    fn active_selection_for(
        &self,
        client_hint: Option<ClientId>,
    ) -> Option<&crate::ai::SelectionContext> {
        client_hint
            .and_then(|c| self.selections.get(&c))
            .or_else(|| self.active_client.and_then(|c| self.selections.get(&c)))
            .or_else(|| self.selections.values().next_back())
    }

    fn is_custom_target(&self, id: &str, hint_path: Option<&str>) -> bool {
        if self.custom_open.contains_key(id) {
            return true;
        }
        if self.project.entry(id).is_some() {
            return false;
        }
        if hint_path.is_some_and(|p| custom_kind_of_path(p).is_some()) {
            return true;
        }
        self.scan_custom_docs().contains_key(id)
    }

    fn dispatch(&mut self, from: ClientId, msg: &Value) -> Result<Outbox, String> {
        let ty = msg
            .get("type")
            .and_then(|t| t.string().ok())
            .ok_or("a message has a type")?;
        match ty {
            "hello" => {
                if let Some(name) = msg.get("client").and_then(|c| c.string().ok()) {
                    self.clients.insert(from, name.into());
                }
                Ok(vec![(
                    from,
                    obj(vec![
                        ("type", s("welcome")),
                        ("client", n(from)),
                        ("project", self.project.manifest.to_value()),
                        (
                            "toolCatalogs",
                            Value::Array(
                                self.project
                                    .manifest
                                    .editor
                                    .iter()
                                    .map(|dir| format!("{dir}/project-tools.json"))
                                    .filter(|path| {
                                        self.inside(path).is_ok()
                                            && self.project.root.join(path).is_file()
                                    })
                                    .map(|path| s(&path))
                                    .collect(),
                            ),
                        ),
                        ("gameWorkspaces", self.game_workspaces()),
                        ("schemas", Value::Array(self.registry.sources().to_vec())),
                        ("assets", self.assets()),
                        ("drafts", self.unavailable_drafts()),
                    ]),
                )])
            }
            "rescan" => Ok(self.rescan()),
            "open" => {
                let (id, hint_path) = self.doc_target(msg)?;
                if self.is_custom_target(&id, hint_path.as_deref()) {
                    let reread = self.ensure_custom_open(&id, hint_path.as_deref())?;
                    self.custom_open
                        .get_mut(&id)
                        .expect("custom_open")
                        .watchers
                        .insert(from);
                    let mut doc = self.custom_doc_message(&id);
                    if let (Some(r), Value::Object(m)) = (msg.get("req"), &mut doc) {
                        m.insert("req".into(), r.clone());
                    }
                    if reread {
                        return Ok(self.custom_open[&id]
                            .watchers
                            .iter()
                            .map(|w| (*w, doc.clone()))
                            .collect());
                    }
                    return Ok(vec![(from, doc)]);
                }
                let reread = self.ensure_open(&id)?;
                self.open.get_mut(&id).expect("open").watchers.insert(from);
                let mut doc = self.doc_message(&id);
                if let (Some(r), Value::Object(m)) = (msg.get("req"), &mut doc) {
                    m.insert("req".into(), r.clone());
                }
                if reread {
                    // Everyone watching it had the old text.
                    return Ok(self.open[&id]
                        .watchers
                        .iter()
                        .map(|w| (*w, doc.clone()))
                        .collect());
                }
                Ok(vec![(from, doc)])
            }
            "close" => {
                let (id, _) = self.doc_target(msg)?;
                if let Some(o) = self.open.get_mut(&id) {
                    o.watchers.remove(&from);
                }
                if let Some(o) = self.custom_open.get_mut(&id) {
                    o.watchers.remove(&from);
                }
                Ok(vec![])
            }
            "edit" => self.edit(from, msg),
            "delete" => {
                // Entities are deleted through the expanded hierarchy (scene::delete), so what
                // was added under the entities a prefab instance shows goes with the instance.
                let (id, _) = self.doc_target(msg)?;
                if self.custom_open.contains_key(&id) {
                    return Err("a custom workbench document has no scene entities".into());
                }
                let o = self.open.get(&id).ok_or("open the document first")?;
                if o.kind == Kind::Look {
                    return Err("a Look has no entities".into());
                }
                if o.kind == Kind::Data {
                    return Err("a data document has no entities".into());
                }
                if o.kind == Kind::Material {
                    return Err("a material document has no entities".into());
                }
                let ids = msg
                    .get("ids")
                    .ok_or("a delete has ids")?
                    .array()?
                    .iter()
                    .map(|v| v.string().map(String::from))
                    .collect::<Result<Vec<_>, _>>()?;
                let open = self.open_prefabs();
                let load = |a: &str| load_prefab(&open, &self.project, &self.real_root, a);
                let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
                let ops = scene::delete(o.doc.root(), &refs, &load);
                if ops.is_empty() {
                    return Err("nothing to delete".into());
                }
                let Value::Object(m) = msg else {
                    return Err("a message is an object".into());
                };
                let mut m = m.clone();
                m.insert("type".into(), s("edit"));
                m.insert(
                    "ops".into(),
                    Value::Array(ops.iter().map(op_to_json).collect()),
                );
                m.entry("label".into()).or_insert_with(|| s("Delete"));
                self.edit(from, &Value::Object(m))
            }
            "undo" | "redo" => {
                let (id, _) = self.doc_target(msg)?;
                if let Some(o) = self.custom_open.get_mut(&id) {
                    let before = o.doc.revision();
                    let done = if ty == "undo" {
                        o.history.undo(&mut o.doc)
                    } else {
                        o.history.redo(&mut o.doc)
                    };
                    let done = match done {
                        Ok(done) => done,
                        Err(e) => {
                            o.doc.rollback(&[], before)?;
                            return Err(e);
                        }
                    };
                    return match done {
                        Some((label, ops)) => {
                            Ok(self.broadcast_custom_ops(&id, &ops, &label, from, msg.get("req")))
                        }
                        None => Ok(vec![]),
                    };
                }
                let o = self.open.get_mut(&id).ok_or("open the document first")?;
                let before = o.doc.revision();
                let done = if ty == "undo" {
                    o.history.undo(&mut o.doc)
                } else {
                    o.history.redo(&mut o.doc)
                };
                let done = match done {
                    Ok(done) => done,
                    Err(e) => {
                        // A step that does not apply is taken back whole, and nobody hears of
                        // it: the revision must not move either.
                        o.doc.rollback(&[], before)?;
                        return Err(e);
                    }
                };
                match done {
                    Some((label, ops)) => {
                        let root = o.doc.root().clone();
                        let open = self.open_prefabs();
                        let load = |a: &str| load_prefab(&open, &self.project, &self.real_root, a);
                        let known = issue_set(self.open[&id].kind, &root, &self.registry, &load);
                        self.open.get_mut(&id).expect("open").known_issues = known;
                        Ok(self.broadcast_ops(&id, &ops, &label, from, msg.get("req")))
                    }
                    None => Ok(vec![]),
                }
            }
            "save" => {
                let (id, _) = self.doc_target(msg)?;
                if self.custom_open.contains_key(&id) {
                    let (path, root, on_disk) = {
                        let o = &self.custom_open[&id];
                        (o.path.clone(), o.doc.root().clone(), o.on_disk.clone())
                    };
                    self.inside(&path)?;
                    if !self.project.root.join(&path).is_file() {
                        return Err(format!("{path} is missing; rescan before saving"));
                    }
                    let force = msg.get("force") == Some(&Value::Bool(true));
                    let disk = self.read_custom_json(&path);
                    let changed = disk.as_ref().ok() != Some(&on_disk);
                    if changed && !force {
                        let mut e = error(
                            msg.get("req"),
                            &format!(
                                "{path} changed on disk since it was opened; save again with force to overwrite it, or reopen it"
                            ),
                        );
                        if let Value::Object(m) = &mut e {
                            m.insert("code".into(), s(CHANGED_ON_DISK));
                            m.insert("doc".into(), s(&id));
                        }
                        return Ok(vec![(from, e)]);
                    }
                    ganbate_pipeline::project::write_json(&self.project.root.join(&path), &root)?;
                    let o = self.custom_open.get_mut(&id).expect("custom_open");
                    o.on_disk = root;
                    o.changed_on_disk = false;
                    o.history.mark_saved();
                    let saved_msg = obj(vec![
                        ("type", s("saved")),
                        ("by", n(from)),
                        ("req", msg.get("req").cloned().unwrap_or(Value::Null)),
                        ("doc", s(&id)),
                        ("rev", n(o.doc.revision())),
                        ("path", s(&path)),
                    ]);
                    return Ok(o.watchers.iter().map(|w| (*w, saved_msg.clone())).collect());
                }
                let o = self.open.get(&id).ok_or("open the document first")?;
                if let Some(error) = &o.unavailable {
                    return Err(error.clone());
                }
                let (path, kind, root) = (o.path.clone(), o.kind, o.doc.root().clone());
                self.inside(&path)?;
                if !self.project.root.join(&path).is_file() {
                    return Err(format!("{path} is missing; rescan before saving"));
                }
                let force = msg.get("force") == Some(&Value::Bool(true));
                // A file another tool changed since this host read or wrote it is not
                // overwritten without being asked to (`force`).
                let disk = self.project.read_document(&id);
                if kind.self_identified()
                    && disk
                        .as_ref()
                        .ok()
                        .and_then(|d| d.get("id"))
                        .and_then(|id| id.string().ok())
                        != Some(&id)
                {
                    return Err(format!(
                        "{path} no longer identifies this document; rescan before saving"
                    ));
                }
                let changed = disk.as_ref().ok() != Some(&o.on_disk);
                if changed && !force {
                    let mut e = error(
                        msg.get("req"),
                        &format!(
                            "{path} changed on disk since it was opened (another tool, git or \
                             editor wrote it); save again with force to overwrite it, or \
                             reopen it to see the change"
                        ),
                    );
                    if let Value::Object(m) = &mut e {
                        m.insert("code".into(), s(CHANGED_ON_DISK));
                        m.insert("doc".into(), s(&id));
                    }
                    return Ok(vec![(from, e)]);
                }
                if kind == Kind::Look {
                    self.inside(&format!("{path}.meta.json"))?;
                    validate_look(&root)?;
                    self.project.save_look(&id, &root)?;
                } else {
                    self.project.write_document(&path, kind, &root)?;
                }
                let o = self.open.get_mut(&id).expect("open");
                o.on_disk = root;
                o.changed_on_disk = false;
                o.history.mark_saved();
                let msg = obj(vec![
                    ("type", s("saved")),
                    ("by", n(from)),
                    ("req", msg.get("req").cloned().unwrap_or(Value::Null)),
                    ("doc", s(&id)),
                    ("rev", n(o.doc.revision())),
                    ("path", s(&path)),
                ]);
                Ok(o.watchers.iter().map(|w| (*w, msg.clone())).collect())
            }
            "ids" => {
                let count = msg
                    .get("count")
                    .and_then(|c| c.integer().ok())
                    .unwrap_or(1)
                    .clamp(1, MAX_IDS);
                let ids = (0..count).map(|_| s(&self.ids.entity())).collect();
                Ok(vec![(
                    from,
                    obj(vec![("type", s("ids")), ("ids", Value::Array(ids))]),
                )])
            }
            "create" => {
                let kind_str = msg.get("kind").and_then(|k| k.string().ok());
                if kind_str == Some("look") {
                    let path = self.new_path(msg, "look")?;
                    if let Some(parent) = self.project.root.join(&path).parent() {
                        std::fs::create_dir_all(parent)
                            .map_err(|e| format!("{}: {e}", parent.display()))?;
                    }
                    self.inside(&format!("{path}.meta.json"))?;
                    let id = self.project.create_look(&path, &self.ids.asset())?;
                    return Ok(self.created_with_req(from, &id, &path, msg.get("req")));
                }
                if kind_str == Some("material") {
                    let path = self.new_path(msg, "material")?;
                    if let Some(parent) = self.project.root.join(&path).parent() {
                        std::fs::create_dir_all(parent)
                            .map_err(|e| format!("{}: {e}", parent.display()))?;
                    }
                    self.inside(&path)?;
                    let name = msg
                        .get("name")
                        .and_then(|v| v.string().ok())
                        .unwrap_or("Untitled Material");
                    let id = self.fresh_asset_id();
                    let mut root_map = match msg.get("initial") {
                        Some(Value::Object(m)) => m.clone(),
                        _ => BTreeMap::new(),
                    };
                    root_map.insert("ganbate".into(), s("material"));
                    root_map.insert("version".into(), Value::Number(1.0));
                    root_map.insert("id".into(), s(&id));
                    root_map
                        .entry("name".to_string())
                        .or_insert_with(|| s(name));
                    let root = Value::Object(root_map);
                    self.project.write_document(&path, Kind::Material, &root)?;
                    return Ok(self.created_with_req(from, &id, &path, msg.get("req")));
                }
                if let Some(ck) =
                    kind_str.filter(|k| matches!(*k, "soundbank" | "character" | "seq" | "vfx"))
                {
                    let path = self.new_custom_path(msg, ck)?;
                    if let Some(parent) = self.project.root.join(&path).parent() {
                        std::fs::create_dir_all(parent)
                            .map_err(|e| format!("{}: {e}", parent.display()))?;
                    }
                    self.inside(&path)?;
                    let name = msg
                        .get("name")
                        .and_then(|v| v.string().ok())
                        .unwrap_or("Untitled");
                    let id = custom_doc_id(&path);
                    let mut root_map = match msg.get("initial") {
                        Some(Value::Object(m)) => m.clone(),
                        _ => BTreeMap::new(),
                    };
                    root_map.insert("ganbate".into(), s(ck));
                    root_map.insert("version".into(), Value::Number(1.0));
                    root_map.insert("id".into(), s(&id));
                    root_map
                        .entry("name".to_string())
                        .or_insert_with(|| s(name));
                    ganbate_pipeline::project::write_json(
                        &self.project.root.join(&path),
                        &Value::Object(root_map),
                    )?;
                    return Ok(self.created_custom(from, ck, &id, &path, msg.get("req")));
                }
                if kind_str == Some("data") {
                    let schema = msg
                        .get("schema")
                        .and_then(|v| v.string().ok())
                        .ok_or("create data needs a schema")?;
                    if !ganbate_scene::schema::valid_kind_name(schema, false) {
                        return Err(format!("invalid data schema name {schema:?}"));
                    }
                    if self.registry.get(schema).is_none() {
                        return Err(format!("unknown schema {schema:?}"));
                    }
                    let path = self.new_path(msg, "data")?;
                    if let Some(parent) = self.project.root.join(&path).parent() {
                        std::fs::create_dir_all(parent)
                            .map_err(|e| format!("{}: {e}", parent.display()))?;
                    }
                    self.inside(&path)?;
                    let name = msg
                        .get("name")
                        .and_then(|v| v.string().ok())
                        .unwrap_or("Untitled");
                    let id = self.fresh_asset_id();
                    let root = new_data_document(&id, schema, name);
                    self.project.write_document(&path, Kind::Data, &root)?;
                    return Ok(self.created_with_req(from, &id, &path, msg.get("req")));
                }
                let kind = match kind_str {
                    Some("scene") => scene::Kind::Scene,
                    Some("prefab") => scene::Kind::Prefab,
                    _ => return Err("create a scene, prefab, Look or data document".into()),
                };
                let path = self.new_path(msg, kind.name())?;
                if let Some(parent) = self.project.root.join(&path).parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("{}: {e}", parent.display()))?;
                }
                let name = msg
                    .get("name")
                    .and_then(|v| v.string().ok())
                    .unwrap_or("Untitled");
                let id = self.ids.asset();
                let mut root = scene::new_document(kind, &id, name);
                if kind == scene::Kind::Prefab {
                    // A prefab starts with its one root entity.
                    let e = self.ids.entity();
                    let mut d = Document::new(root)?;
                    d.apply_all(&scene::create(&e, name, None, None, BTreeMap::new()))?;
                    root = d.root().clone();
                }
                let pk = if kind == scene::Kind::Scene {
                    Kind::Scene
                } else {
                    Kind::Prefab
                };
                self.project.write_document(&path, pk, &root)?;
                Ok(self.created_with_req(from, &id, &path, msg.get("req")))
            }
            "import" => {
                let model = msg
                    .get("model")
                    .and_then(|m| m.string().ok())
                    .ok_or("import needs a model asset id")?;
                let entry = self
                    .project
                    .entry(model)
                    .filter(|e| e.kind == Kind::Model)
                    .ok_or_else(|| format!("{model} is not a model"))?
                    .clone();
                let path = self.new_path(msg, "scene")?;
                self.inside(&entry.path)?;
                let bytes = ganbate_assets::read_asset(&self.project.root.join(&entry.path))?;
                let nodes = ganbate_assets::glb::import_nodes(&bytes, true)?;
                let name = msg
                    .get("name")
                    .and_then(|v| v.string().ok())
                    .unwrap_or(&entry.path)
                    .to_string();
                let id = self.ids.asset();
                let root = cook::scene_from_model(&nodes, model, &id, &name, &mut self.ids);
                self.project.write_document(&path, Kind::Scene, &root)?;
                Ok(self.created_with_req(from, &id, &path, msg.get("req")))
            }
            "asset-usage" => self.asset_usage(from, msg),
            "asset-move" => self.asset_move(from, msg),
            "asset-import" => self.asset_import(from, msg),
            "cook" => self.cook(from, msg),
            "selection-sync" => {
                let sel = crate::ai::SelectionContext::from_message(msg);
                self.selections.insert(from, sel);
                self.active_client = Some(from);
                if let Some(r) = msg.get("req") {
                    Ok(vec![(
                        from,
                        obj(vec![("type", s("selection-synced")), ("req", r.clone())]),
                    )])
                } else {
                    Ok(vec![])
                }
            }
            "project-context" => {
                let limit = msg
                    .get("limit")
                    .and_then(|v| v.unsigned().ok())
                    .unwrap_or(24) as usize;
                let ctx = self.mcp_project_context(limit)?;
                let mut resp = vec![("type", s("project-context-result")), ("context", ctx)];
                if let Some(r) = msg.get("req") {
                    resp.push(("req", r.clone()));
                }
                Ok(vec![(from, obj(resp))])
            }
            "catalog-query" => {
                let cat = crate::ai::catalog::build_catalog(&self.project, &self.registry, true)?;
                let query = msg.get("query").and_then(|v| v.string().ok());
                let kind = msg.get("kind").and_then(|v| v.string().ok());
                let limit = msg
                    .get("limit")
                    .and_then(|v| v.unsigned().ok())
                    .unwrap_or(64) as usize;
                let items: Vec<Value> = cat
                    .search(query, kind, limit)
                    .into_iter()
                    .map(crate::ai::catalog::CatalogItem::to_json)
                    .collect();
                let mut resp = vec![
                    ("type", s("catalog-results")),
                    ("items", Value::Array(items)),
                    ("indexText", s(&cat.index_text())),
                ];
                if let Some(r) = msg.get("req") {
                    resp.push(("req", r.clone()));
                }
                Ok(vec![(from, obj(resp))])
            }
            "catalog-inspect" => {
                let reference = msg
                    .get("ref")
                    .and_then(|v| v.string().ok())
                    .ok_or("catalog-inspect requires ref")?;
                let cat = crate::ai::catalog::build_catalog(&self.project, &self.registry, false)?;
                let item = crate::ai::catalog::inspect_item(&cat, &self.project, reference)?;
                let mut resp = vec![("type", s("catalog-inspected")), ("item", item)];
                if let Some(r) = msg.get("req") {
                    resp.push(("req", r.clone()));
                }
                Ok(vec![(from, obj(resp))])
            }
            "gbs-apply" => {
                let doc_id = self.resolve_active_or_explicit_doc(msg, Some(from))?;
                let script = msg
                    .get("script")
                    .and_then(|v| v.string().ok())
                    .ok_or("gbs-apply requires script")?;
                let dry_run = msg.get("dryRun") == Some(&Value::Bool(true));
                let mut outbox = Vec::new();
                let payload = self.apply_gbs_internal(
                    from,
                    &doc_id,
                    script,
                    dry_run,
                    msg.get("req"),
                    &mut outbox,
                )?;
                let mut reply = match payload {
                    Value::Object(m) => m,
                    _ => BTreeMap::new(),
                };
                reply.insert("type".into(), s("gbs-applied"));
                if let Some(r) = msg.get("req") {
                    reply.insert("req".into(), r.clone());
                }
                outbox.push((from, Value::Object(reply)));
                Ok(outbox)
            }
            "scene-verify" => {
                let doc_id = self.resolve_active_or_explicit_doc(msg, Some(from))?;
                let cam = msg
                    .get("camera")
                    .and_then(|v| v.string().ok())
                    .map(String::from);
                let width = msg
                    .get("width")
                    .and_then(|v| v.unsigned().ok())
                    .unwrap_or(320) as u32;
                let height = msg
                    .get("height")
                    .and_then(|v| v.unsigned().ok())
                    .unwrap_or(180) as u32;
                let want_sheet = msg.get("contactSheet") == Some(&Value::Bool(true));
                let contact_path = msg
                    .get("contactSheetPath")
                    .and_then(|v| v.string().ok())
                    .map(String::from);
                let report = self.verify_doc_internal(
                    &doc_id,
                    Some(from),
                    cam,
                    width,
                    height,
                    want_sheet || contact_path.is_some(),
                    contact_path.as_deref(),
                )?;
                let mut resp = vec![
                    ("type", s("scene-verified")),
                    ("doc", s(&doc_id)),
                    ("report", report),
                ];
                if let Some(r) = msg.get("req") {
                    resp.push(("req", r.clone()));
                }
                Ok(vec![(from, obj(resp))])
            }
            "brick-crystallize" => {
                let doc_id = self.resolve_active_or_explicit_doc(msg, Some(from))?;
                let brick_id = msg
                    .get("id")
                    .and_then(|v| v.string().ok())
                    .ok_or("brick-crystallize requires id")?;
                let from_tx = msg.get("fromTx") == Some(&Value::Bool(true));
                let explicit_entities: Vec<String> = msg
                    .get("entities")
                    .and_then(|v| v.array().ok())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|x| x.string().ok().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let params: Vec<String> = msg
                    .get("params")
                    .and_then(|v| v.array().ok())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|x| x.string().ok().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let brick = self.crystallize_doc_internal(
                    &doc_id,
                    Some(from),
                    brick_id,
                    &explicit_entities,
                    from_tx,
                    &params,
                )?;
                let mut resp = vec![("type", s("brick-crystallized")), ("brick", brick)];
                if let Some(r) = msg.get("req") {
                    resp.push(("req", r.clone()));
                }
                Ok(vec![(from, obj(resp))])
            }
            "world-graph-fetch" => {
                let mut payload = match self.world_graph_fetch()? {
                    Value::Object(m) => m,
                    _ => BTreeMap::new(),
                };
                payload.insert("type".into(), s("world-graph-result"));
                if let Some(r) = msg.get("req") {
                    payload.insert("req".into(), r.clone());
                }
                Ok(vec![(from, Value::Object(payload))])
            }
            "world-graph-patch" => {
                let patch = msg.get("patch").ok_or("world-graph-patch requires patch")?;
                let mut outbox = Vec::new();
                let res = self.world_graph_patch(patch, &mut outbox)?;
                let mut reply = match res {
                    Value::Object(m) => m,
                    _ => BTreeMap::new(),
                };
                reply.insert("type".into(), s("world-graph-patched"));
                if let Some(r) = msg.get("req") {
                    reply.insert("req".into(), r.clone());
                }
                outbox.push((from, Value::Object(reply)));
                Ok(outbox)
            }
            "world-graph-prompt" => {
                let prompt = msg
                    .get("prompt")
                    .and_then(|v| v.string().ok())
                    .ok_or("world-graph-prompt requires prompt")?;
                let mut outbox = Vec::new();
                let res = self.world_graph_prompt(prompt, &mut outbox)?;
                let mut reply = match res {
                    Value::Object(m) => m,
                    _ => BTreeMap::new(),
                };
                reply.insert("type".into(), s("world-graph-prompted"));
                if let Some(r) = msg.get("req") {
                    reply.insert("req".into(), r.clone());
                }
                outbox.push((from, Value::Object(reply)));
                Ok(outbox)
            }
            "world-graph-benchmark" => {
                let res = self.world_graph_benchmark(msg)?;
                let mut reply = match res {
                    Value::Object(m) => m,
                    _ => BTreeMap::new(),
                };
                reply.insert("type".into(), s("world-graph-benchmark-result"));
                if let Some(r) = msg.get("req") {
                    reply.insert("req".into(), r.clone());
                }
                Ok(vec![(from, Value::Object(reply))])
            }
            "preview" | "select" => {
                let (id, _) = self.doc_target(msg)?;
                let watchers = if let Some(o) = self.open.get(&id) {
                    &o.watchers
                } else if let Some(o) = self.custom_open.get(&id) {
                    &o.watchers
                } else {
                    return Err("open the document first".into());
                };
                let mut relayed = msg.clone();
                if let Value::Object(m) = &mut relayed {
                    m.insert("by".into(), n(from));
                }
                Ok(watchers
                    .iter()
                    .filter(|w| **w != from)
                    .map(|w| (*w, relayed.clone()))
                    .collect())
            }
            other => Err(format!("unknown message type {other:?}")),
        }
    }

    fn new_path(&self, msg: &Value, kind: &str) -> Result<String, String> {
        let path = msg
            .get("path")
            .and_then(|p| p.string().ok())
            .ok_or("give the new document a path")?;
        let suffix = format!(".{kind}.json");
        if !path.ends_with(&suffix) {
            return Err(format!("a {kind} file ends in {suffix}"));
        }
        // Plain names the project scan finds again: it skips hidden files and folders.
        if path
            .split('/')
            .any(|p| p.is_empty() || p.starts_with('.') || p.contains(['\\', ':', '\0']))
        {
            return Err(format!(
                "{path:?} is not a plain path (names separated by /, none starting with a dot)"
            ));
        }
        let Some(folder) = self
            .project
            .manifest
            .content
            .iter()
            .find(|c| path.starts_with(&format!("{c}/")))
        else {
            return Err("put new documents in a content folder".into());
        };
        let depth = path[folder.len() + 1..].matches('/').count();
        if depth > MAX_FOLDER_DEPTH {
            return Err(format!(
                "{path} is {depth} folders deep in {folder}; a project keeps to {MAX_FOLDER_DEPTH}"
            ));
        }
        if std::fs::symlink_metadata(self.project.root.join(path)).is_ok() {
            return Err(format!("{path} already exists"));
        }
        self.inside(path)?;
        Ok(path.to_string())
    }

    fn new_custom_path(&self, msg: &Value, kind: &str) -> Result<String, String> {
        let path = msg
            .get("path")
            .and_then(|p| p.string().ok())
            .ok_or("give the new document a path")?;
        let suffix = format!(".{kind}.json");
        if !path.ends_with(&suffix) {
            return Err(format!("a {kind} file ends in {suffix}"));
        }
        if path
            .split('/')
            .any(|p| p.is_empty() || p.starts_with('.') || p.contains(['\\', ':', '\0']))
        {
            return Err(format!(
                "{path:?} is not a plain path (names separated by /, none starting with a dot)"
            ));
        }
        let Some(folder) = self
            .project
            .manifest
            .content
            .iter()
            .chain(self.project.manifest.editor.iter())
            .find(|c| path.starts_with(&format!("{c}/")))
        else {
            return Err("put new workbench documents in a content or editor folder".into());
        };
        let depth = path[folder.len() + 1..].matches('/').count();
        if depth > MAX_FOLDER_DEPTH {
            return Err(format!(
                "{path} is {depth} folders deep in {folder}; a project keeps to {MAX_FOLDER_DEPTH}"
            ));
        }
        if std::fs::symlink_metadata(self.project.root.join(path)).is_ok() {
            return Err(format!("{path} already exists"));
        }
        self.inside(path)?;
        Ok(path.to_string())
    }

    fn created_with_req(
        &self,
        from: ClientId,
        id: &str,
        path: &str,
        req: Option<&Value>,
    ) -> Outbox {
        let mut pairs = vec![
            ("type", s("created")),
            (
                "kind",
                s(self.project.entry(id).expect("created asset").kind.name()),
            ),
            ("doc", s(id)),
            ("path", s(path)),
        ];
        if let Some(r) = req {
            pairs.push(("req", r.clone()));
        }
        let mut out = vec![(from, obj(pairs))];
        let assets = obj(vec![("type", s("assets")), ("assets", self.assets())]);
        out.extend(self.clients.keys().map(|c| (*c, assets.clone())));
        out
    }

    fn created_custom(
        &self,
        from: ClientId,
        kind: &str,
        id: &str,
        path: &str,
        req: Option<&Value>,
    ) -> Outbox {
        let mut pairs = vec![
            ("type", s("created")),
            ("kind", s(kind)),
            ("doc", s(id)),
            ("path", s(path)),
        ];
        if let Some(r) = req {
            pairs.push(("req", r.clone()));
        }
        let mut out = vec![(from, obj(pairs))];
        let assets = obj(vec![("type", s("assets")), ("assets", self.assets())]);
        out.extend(self.clients.keys().map(|c| (*c, assets.clone())));
        out
    }

    fn edit(&mut self, from: ClientId, msg: &Value) -> Result<Outbox, String> {
        let (id, _) = self.doc_target(msg)?;
        let req = msg.get("req");
        if self.custom_open.contains_key(&id) {
            let rev = msg.get("rev").and_then(|r| r.unsigned().ok());
            if rev != Some(self.custom_open[&id].doc.revision()) {
                return Ok(vec![
                    (
                        from,
                        error(req, "the document changed; edit refused, here it is again"),
                    ),
                    (from, self.custom_doc_message(&id)),
                ]);
            }
            let ops = msg
                .get("ops")
                .ok_or("an edit has ops")?
                .array()?
                .iter()
                .map(op_from_json)
                .collect::<Result<Vec<_>, _>>()?;
            if ops.is_empty() {
                return Err("an edit has at least one op".into());
            }
            for op in &ops {
                if matches!(op.path()[0].as_str(), "id" | "ganbate" | "version") {
                    return Err(format!("{:?} cannot be edited", op.path()[0]));
                }
            }
            let label = msg
                .get("label")
                .and_then(|l| l.string().ok())
                .unwrap_or("Edit");
            let merge = msg.get("merge").and_then(|m| m.string().ok());
            let o = self.custom_open.get_mut(&id).expect("custom_open");
            let before = o.doc.revision();
            let undo = match o.doc.apply_all(&ops) {
                Ok(undo) => undo,
                Err(e) => {
                    o.doc.rollback(&[], before)?;
                    return Err(e);
                }
            };
            let applied = ops.clone();
            o.history.record(label, merge, ops, undo);
            return Ok(self.broadcast_custom_ops(&id, &applied, label, from, req));
        }

        if !self.open.contains_key(&id) {
            return Err("open the document first".into());
        }
        let rev = msg.get("rev").and_then(|r| r.unsigned().ok());
        if rev != Some(self.open[&id].doc.revision()) {
            // The sender is behind: give it the document as it is.
            return Ok(vec![
                (
                    from,
                    error(req, "the document changed; edit refused, here it is again"),
                ),
                (from, self.doc_message(&id)),
            ]);
        }
        let ops = msg
            .get("ops")
            .ok_or("an edit has ops")?
            .array()?
            .iter()
            .map(op_from_json)
            .collect::<Result<Vec<_>, _>>()?;
        if ops.is_empty() {
            return Err("an edit has at least one op".into());
        }
        for op in &ops {
            match self.open[&id].kind {
                Kind::Look => {}
                Kind::Material => {
                    if matches!(op.path()[0].as_str(), "id" | "ganbate" | "version") {
                        return Err(format!("{:?} cannot be edited", op.path()[0]));
                    }
                }
                Kind::Data => {
                    if !matches!(op.path()[0].as_str(), "values" | "name") {
                        return Err(format!(
                            "{:?} cannot be edited (only values and name)",
                            op.path()[0]
                        ));
                    }
                }
                _ => {
                    if !matches!(op.path()[0].as_str(), "entities" | "name") {
                        return Err(format!(
                            "{:?} cannot be edited (only entities and name)",
                            op.path()[0]
                        ));
                    }
                }
            }
        }
        let label = msg
            .get("label")
            .and_then(|l| l.string().ok())
            .unwrap_or("Edit");
        let merge = msg.get("merge").and_then(|m| m.string().ok());
        let open = self.open_prefabs();
        let (project, real_root, registry) = (&self.project, &self.real_root, &self.registry);
        let load = |a: &str| load_prefab(&open, project, real_root, a);
        let o = self.open.get_mut(&id).expect("open");
        let before = o.doc.revision();
        let undo = match o.doc.apply_all(&ops) {
            Ok(undo) => undo,
            Err(e) => {
                // The batch was taken back whole and nobody hears of it, so the revision stays
                // where it was: every other client's next edit is still current.
                o.doc.rollback(&[], before)?;
                return Err(e);
            }
        };
        let after = issue_set(o.kind, o.doc.root(), registry, &load);
        let fresh: Vec<&(String, String)> = after.difference(&o.known_issues).collect();
        if let Some((path, message)) = fresh.first() {
            let e = format!("{path}: {message}");
            // Taken back before it enters the history, so undo and redo are untouched.
            o.doc.rollback(&undo, before)?;
            return Err(e);
        }
        let mut touched = BTreeSet::new();
        for op in &ops {
            if let Op::Set { path, .. } = op
                && path.len() >= 2
                && path[0] == "entities"
            {
                touched.insert(path[1].clone());
            }
        }
        let applied = ops.clone();
        o.history.record(label, merge, ops, undo);
        o.known_issues = after;
        if !touched.is_empty() {
            self.last_tx_entities
                .insert(id.clone(), touched.into_iter().collect());
        }
        Ok(self.broadcast_ops(&id, &applied, label, from, req))
    }

    /// Records `name` as `doc`'s latest preview and deletes its previews before the last
    /// `PREVIEWS_KEPT`.
    fn keep_preview(&mut self, doc: &str, name: &str) {
        let dir = self.cache_dir();
        let kept = self.previews.entry(doc.to_string()).or_default();
        kept.retain(|n| n != name);
        kept.push_back(name.to_string());
        while kept.len() > PREVIEWS_KEPT {
            if let Some(old) = kept.pop_front() {
                let _ = std::fs::remove_file(dir.join(old));
            }
        }
    }

    fn cook(&mut self, from: ClientId, msg: &Value) -> Result<Outbox, String> {
        let (id, hint_path) = self.doc_target(msg)?;
        if self.is_custom_target(&id, hint_path.as_deref()) {
            return Err(
                "a custom workbench document cannot be cooked; cook a scene or prefab".into(),
            );
        }
        if let Some(asset) = self.project.entry(&id) {
            if asset.kind == Kind::Look {
                return Err("cook a scene or prefab to preview a Look".into());
            }
            if asset.kind == Kind::Data {
                return Err("a data document cannot be cooked; cook a scene or prefab".into());
            }
            if asset.kind == Kind::Material {
                return Err("a material document cannot be cooked; cook a scene or prefab".into());
            }
        }
        // A document read again from disk goes to everyone watching it before the cook.
        let mut out: Outbox = if self.ensure_open(&id)? {
            let doc = self.doc_message(&id);
            self.open[&id]
                .watchers
                .iter()
                .map(|w| (*w, doc.clone()))
                .collect()
        } else {
            vec![]
        };
        let o = &self.open[&id];
        if let Some(error) = &self.open[&id].unavailable {
            return Err(error.clone());
        }
        let root = o.doc.root().clone();
        let rev = o.doc.revision();
        // Open prefabs are cooked as edited, others as saved.
        let open_docs: BTreeMap<String, Value> = self
            .open
            .iter()
            .map(|(k, v)| (k.clone(), v.doc.root().clone()))
            .collect();
        let (project, real_root) = (&self.project, &self.real_root);
        let load = |a: &str| {
            load_prefab(&open_docs, project, real_root, a)
                .ok_or_else(|| format!("missing prefab {a}"))
        };
        // Prefabs with problems (a parent cycle, a second root) are refused with their name.
        let resolved = cook::flatten_checked(&root, &self.registry, &load)?;
        let cooked = cook::cook_resolved(&self.project, &self.registry, &root, &resolved, true)?;
        let bytes = ganbate_assets::cooked::encode_scene(&cooked.world, &cooked.table, false)?;
        let name = cook::content_name(&id, "gbew", &bytes);
        let dir = self.cache_dir();
        self.inside(&self.cache_rel)?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let file = dir.join(&name);
        if !file.exists() {
            ganbate_assets::write_asset(&file, &bytes).map_err(|e| e.to_string())?;
        }
        self.keep_preview(&id, &name);
        // Each entity's own geometry's world bounds, for selection boxes.
        let bounds = |e: &ganbate_core::world::EntityRecord| -> Value {
            let mut lo = [f32::INFINITY; 3];
            let mut hi = [f32::NEG_INFINITY; 3];
            for i in e.instances.clone() {
                let inst = &cooked.world.instances[i as usize];
                for p in &cooked.world.meshes[inst.mesh as usize].positions {
                    let w = inst.transform.point(*p);
                    for (k, v) in [w.0, w.1, w.2].into_iter().enumerate() {
                        lo[k] = lo[k].min(v);
                        hi[k] = hi[k].max(v);
                    }
                }
            }
            if lo[0] > hi[0] {
                return Value::Null;
            }
            let arr = |v: [f32; 3]| {
                Value::Array(v.iter().map(|x| Value::Number(f64::from(*x))).collect())
            };
            Value::Array(vec![arr(lo), arr(hi)])
        };
        let entities = Value::Array(
            cooked
                .table
                .entities
                .iter()
                .map(|e| {
                    obj(vec![
                        ("bounds", bounds(e)),
                        ("id", s(&format!("{:016x}", e.id))),
                        ("name", s(&e.name)),
                        ("parent", e.parent.map_or(Value::Null, |p| n(u64::from(p)))),
                        (
                            "transform",
                            Value::Array(
                                e.transform
                                    .0
                                    .iter()
                                    .map(|v| Value::Number(f64::from(*v)))
                                    .collect(),
                            ),
                        ),
                        (
                            "instances",
                            Value::Array(vec![
                                n(u64::from(e.instances.start)),
                                n(u64::from(e.instances.end - e.instances.start)),
                            ]),
                        ),
                        (
                            "lights",
                            Value::Array(vec![
                                n(u64::from(e.lights.start)),
                                n(u64::from(e.lights.end - e.lights.start)),
                            ]),
                        ),
                    ])
                })
                .collect(),
        );
        out.push((
            from,
            obj(vec![
                ("type", s("cooked")),
                ("doc", s(&id)),
                ("rev", n(rev)),
                ("file", s(&format!("{}/{name}", self.cache_rel))),
                ("entities", entities),
                (
                    "lamps",
                    Value::Array(
                        cooked
                            .world
                            .lights
                            .iter()
                            .map(|l| {
                                Value::Array(
                                    [l.position.0, l.position.1, l.position.2]
                                        .iter()
                                        .map(|v| Value::Number(f64::from(*v)))
                                        .collect(),
                                )
                            })
                            .collect(),
                    ),
                ),
                (
                    "warnings",
                    Value::Array(cooked.warnings.iter().map(|w| s(w)).collect()),
                ),
                ("data", json::parse(cooked.table.data.as_bytes())?),
            ]),
        ));
        Ok(out)
    }

    fn apply_gbs_internal(
        &mut self,
        by: ClientId,
        doc_id: &str,
        script: &str,
        dry_run: bool,
        req: Option<&Value>,
        outbox: &mut Outbox,
    ) -> Result<Value, String> {
        let reread = self.ensure_open(doc_id)?;
        if reread {
            let doc_msg = self.doc_message(doc_id);
            outbox.extend(
                self.open[doc_id]
                    .watchers
                    .iter()
                    .map(|w| (*w, doc_msg.clone())),
            );
        }
        let kind = self.open[doc_id].kind;
        if !matches!(kind, Kind::Scene | Kind::Prefab | Kind::Look) {
            return Err(format!(
                ".gbs spatial scripts apply to scene, prefab, or Look documents (not {})",
                kind.name()
            ));
        }
        let root = self.open[doc_id].doc.root().clone();
        let sel = self.active_selection_for((by != 0).then_some(by)).cloned();
        let open_prefs = self.open_prefabs();
        let outcome = {
            let (project, real_root, registry) = (&self.project, &self.real_root, &self.registry);
            let load = |a: &str| load_prefab(&open_prefs, project, real_root, a);
            crate::ai::gbs::execute_gbs(
                project,
                registry,
                &root,
                script,
                sel.as_ref(),
                &load,
                &mut self.ids,
            )?
        };
        let spawned_json = outcome.spawned_json();
        let outcome_label = outcome.label.clone();
        let outcome_statements = outcome.statements;
        let rev = if !dry_run {
            let open_prefs = self.open_prefabs();
            let (project, real_root, registry) = (&self.project, &self.real_root, &self.registry);
            let load = |a: &str| load_prefab(&open_prefs, project, real_root, a);
            let o = self.open.get_mut(doc_id).expect("open");
            let before = o.doc.revision();
            let undo = match o.doc.apply_all(&outcome.ops) {
                Ok(u) => u,
                Err(e) => {
                    o.doc.rollback(&[], before)?;
                    return Err(e);
                }
            };
            let after = issue_set(o.kind, o.doc.root(), registry, &load);
            let fresh: Vec<&(String, String)> = after.difference(&o.known_issues).collect();
            if let Some((p, m)) = fresh.first() {
                let err_str = format!("{p}: {m}");
                o.doc.rollback(&undo, before)?;
                return Err(err_str);
            }
            let applied = outcome.ops.clone();
            o.history.record(&outcome_label, None, outcome.ops, undo);
            o.known_issues = after;
            let new_rev = o.doc.revision();
            let mut touched: BTreeSet<String> =
                outcome.spawned.iter().map(|(id, _)| id.clone()).collect();
            for op in &applied {
                if let Op::Set { path, .. } = op
                    && path.len() >= 2
                    && path[0] == "entities"
                {
                    touched.insert(path[1].clone());
                }
            }
            if !touched.is_empty() {
                self.last_tx_entities
                    .insert(doc_id.to_string(), touched.into_iter().collect());
            }
            outbox.extend(self.broadcast_ops(doc_id, &applied, &outcome_label, by, req));
            new_rev
        } else {
            self.open[doc_id].doc.revision()
        };

        Ok(obj(vec![
            ("ok", Value::Bool(true)),
            ("doc", s(doc_id)),
            ("rev", n(rev)),
            ("label", s(&outcome_label)),
            ("statements", n(outcome_statements as u64)),
            ("spawned", spawned_json),
            ("dryRun", Value::Bool(dry_run)),
        ]))
    }

    #[allow(clippy::too_many_arguments)]
    fn verify_doc_internal(
        &mut self,
        doc_id: &str,
        client_hint: Option<ClientId>,
        camera_ref: Option<String>,
        width: u32,
        height: u32,
        render_sheet: bool,
        contact_sheet_rel: Option<&str>,
    ) -> Result<Value, String> {
        let _ = self.ensure_open(doc_id)?;
        let kind = self.open[doc_id].kind;
        if !matches!(kind, Kind::Scene | Kind::Prefab) {
            return Err("scene verification applies to scene or prefab documents".into());
        }
        let root = self.open[doc_id].doc.root().clone();
        let sel = self.active_selection_for(client_hint).cloned();
        let open_prefs = self.open_prefabs();
        let opts = crate::ai::verify::VerifyOptions {
            camera_ref,
            width,
            height,
            render_contact_sheet: render_sheet || contact_sheet_rel.is_some(),
            ..Default::default()
        };
        let report = {
            let (project, real_root, registry) = (&self.project, &self.real_root, &self.registry);
            let load = |a: &str| load_prefab(&open_prefs, project, real_root, a);
            crate::ai::verify::verify_scene(project, registry, &root, sel.as_ref(), &load, &opts)?
        };
        let mut written_rel = None;
        if let Some(rel) = contact_sheet_rel
            && let Some(png) = &report.contact_sheet_png
        {
            self.inside(rel)?;
            let full = self.project.root.join(rel);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("{}: {e}", parent.display()))?;
            }
            ganbate_assets::write_asset(&full, png).map_err(|e| format!("{rel}: {e}"))?;
            written_rel = Some(rel);
        }
        let mut out = report.to_json(written_rel);
        if render_sheet
            && contact_sheet_rel.is_none()
            && let (Some(png), Value::Object(m)) = (&report.contact_sheet_png, &mut out)
        {
            m.insert(
                "contactSheetBase64".into(),
                Value::String(ganbate_core::digest::base64(png)),
            );
        }
        Ok(out)
    }

    fn crystallize_doc_internal(
        &mut self,
        doc_id: &str,
        client_hint: Option<ClientId>,
        brick_id: &str,
        explicit_entities: &[String],
        from_tx: bool,
        params: &[String],
    ) -> Result<Value, String> {
        let _ = self.ensure_open(doc_id)?;
        let kind = self.open[doc_id].kind;
        if !matches!(kind, Kind::Scene | Kind::Prefab) {
            return Err("brick crystallize requires a scene or prefab document".into());
        }
        let root = self.open[doc_id].doc.root().clone();
        let seeds: Vec<String> = if !explicit_entities.is_empty() {
            explicit_entities.to_vec()
        } else if from_tx {
            if let Some(in_mem) = self.last_tx_entities.get(doc_id)
                && !in_mem.is_empty()
            {
                in_mem.clone()
            } else {
                crate::ai::catalog::entities_from_history_tx(&self.project.root, doc_id, None)?
            }
        } else if let Some(sel) = self.active_selection_for(client_hint)
            && !sel.entity_ids.is_empty()
        {
            sel.entity_ids.clone()
        } else {
            Vec::new()
        };
        crate::ai::catalog::crystallize_brick(
            &self.project,
            &self.registry,
            &root,
            brick_id,
            &seeds,
            params,
        )
    }

    /// Returns enriched live `@selection` details for MCP `studio_selection` and `ganbate://selection`.
    pub fn mcp_selection_details(&self) -> Value {
        let Some(sel) = self.active_selection_for(None) else {
            return obj(vec![
                ("ok", Value::Bool(true)),
                ("active", Value::Bool(false)),
                ("doc", Value::Null),
                ("entityIds", Value::Array(Vec::new())),
                ("entities", Value::Array(Vec::new())),
            ]);
        };
        let mut base = match sel.to_json() {
            Value::Object(m) => m,
            _ => BTreeMap::new(),
        };
        base.insert("ok".into(), Value::Bool(true));
        base.insert("active".into(), Value::Bool(true));
        let mut resolved_entities = Vec::new();
        if let Some(doc_ref) = &sel.doc
            && let Ok((doc_id, _)) = self.resolve_doc_ref(doc_ref)
            && let Some(o) = self.open.get(&doc_id)
            && let Some(Value::Object(ents)) = o.doc.root().get("entities")
        {
            base.insert("docPath".into(), s(&o.path));
            for eid in &sel.entity_ids {
                if let Some(ent) = ents.get(eid) {
                    resolved_entities.push(obj(vec![
                        ("id", s(eid)),
                        ("name", ent.get("name").cloned().unwrap_or_else(|| s(""))),
                        (
                            "components",
                            ent.get("components")
                                .cloned()
                                .unwrap_or_else(|| obj(vec![])),
                        ),
                    ]));
                }
            }
        }
        base.insert("entities".into(), Value::Array(resolved_entities));
        Value::Object(base)
    }

    /// Builds a token-compact L0/L1 AI Project Context digest (`<800 tokens`) for any game project
    /// opened in Ganbate Studio so LLMs and external MCP agents understand scenes, schemas,
    /// reusable bricks, workspaces, and `@selection` without reading raw multi-MB files.
    pub fn mcp_project_context(&self, brick_limit: usize) -> Result<Value, String> {
        let cat = crate::ai::catalog::build_catalog(&self.project, &self.registry, true)?;
        let assets_val = self.assets();
        let mut counts: BTreeMap<String, u64> = BTreeMap::new();
        let mut scenes = Vec::new();
        if let Ok(arr) = assets_val.array() {
            for item in arr {
                let k = item
                    .get("kind")
                    .and_then(|v| v.string().ok())
                    .unwrap_or("unknown");
                *counts.entry(k.to_string()).or_insert(0) += 1;
                if k == "scene" {
                    let pid = item.get("id").and_then(|v| v.string().ok()).unwrap_or("");
                    let ppath = item.get("path").and_then(|v| v.string().ok()).unwrap_or("");
                    scenes.push(obj(vec![("id", s(pid)), ("path", s(ppath))]));
                }
            }
        }

        let mut schema_ids = Vec::new();
        for src in self.registry.sources() {
            if let Some(kind_id) = src.get("kind").and_then(|v| v.string().ok()) {
                schema_ids.push(kind_id.to_string());
            } else if let Some(sid) = src.get("id").and_then(|v| v.string().ok()) {
                schema_ids.push(sid.to_string());
            }
        }

        let top_hits = cat.search(None, None, brick_limit.clamp(1, 64));
        let top_sigs: Vec<String> = top_hits.iter().map(|it| it.signature.clone()).collect();
        let sel = self.mcp_selection_details();
        let active_doc_path = sel
            .get("docPath")
            .and_then(|v| v.string().ok())
            .unwrap_or("none");
        let active_wb = sel
            .get("workbench")
            .and_then(|v| v.string().ok())
            .unwrap_or("world");

        let counts_line = counts
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(", ");
        let scenes_line = scenes
            .iter()
            .filter_map(|sc| sc.get("path").and_then(|v| v.string().ok()))
            .collect::<Vec<_>>()
            .join(", ");

        let gbs_grammar = "tx \"Label\" | spawn <alias> <brick|prefab|model|empty> [name=\"...\"] | \
place <target> at [x,y,z] | place <target> snap onto <ground|entity> [at [x,z]] | \
place <target> socket <sock> on <host> | place <target> beside <peer> <left|right|front|back|above> [gap=0.1] | \
ring <N> <source> around <center|[x,y,z]> [radius=2.0] [face=in|out] [snap=ground] | \
attach <target> <schema_kind> [field=val] | rotate <target> yaw=<deg> | look_at <target> <focus> | \
cam frame <subject|@selection|all> [azimuth=<deg>] [elevation=<deg>]";

        let mut digest = String::new();
        digest.push_str(&format!(
            "# Ganbate AI Context Digest: {}\n",
            self.project.manifest.name
        ));
        digest.push_str(&format!(
            "ActiveDoc: {active_doc_path} | Workbench: {active_wb}\n"
        ));
        digest.push_str(&format!("Inventory: {counts_line}\n"));
        digest.push_str(&format!(
            "Scenes: {}\n",
            if scenes_line.is_empty() {
                "none"
            } else {
                &scenes_line
            }
        ));
        digest.push_str(&format!("Schemas: {}\n", schema_ids.join(", ")));
        digest.push_str(&format!("GBS_DSL: {gbs_grammar}\n"));
        if !top_sigs.is_empty() {
            digest.push_str("Catalog_Top:\n");
            for sig in &top_sigs {
                digest.push_str("  ");
                digest.push_str(sig);
                digest.push('\n');
            }
        }

        let est_tokens = digest.len().div_ceil(4) as u64;
        let counts_obj = Value::Object(counts.into_iter().map(|(k, v)| (k, n(v))).collect());

        Ok(obj(vec![
            ("ok", Value::Bool(true)),
            ("projectName", s(&self.project.manifest.name)),
            ("estimatedTokens", n(est_tokens)),
            ("digestText", s(&digest)),
            ("countsByKind", counts_obj),
            ("scenes", Value::Array(scenes)),
            (
                "schemas",
                Value::Array(schema_ids.into_iter().map(|x| s(&x)).collect()),
            ),
            ("gameWorkspaces", self.game_workspaces()),
            ("selection", sel),
            (
                "topSignatures",
                Value::Array(top_sigs.into_iter().map(|x| s(&x)).collect()),
            ),
            ("gbsGrammar", s(gbs_grammar)),
        ]))
    }

    /// Dispatches an MCP `tools/call` invocation against this `Studio` session (`M-AI.1`).
    pub fn call_mcp_tool(
        &mut self,
        name: &str,
        args: &Value,
        outbox: &mut Outbox,
    ) -> Result<Value, String> {
        const MCP_CLIENT: ClientId = 0;
        match name {
            "project_context" => {
                let limit = args
                    .get("limit")
                    .and_then(|v| v.unsigned().ok())
                    .unwrap_or(24) as usize;
                self.mcp_project_context(limit)
            }
            "studio_status" => {
                let mut open_docs = Vec::new();
                for (id, o) in &self.open {
                    open_docs.push(obj(vec![
                        ("id", s(id)),
                        ("kind", s(o.kind.name())),
                        ("path", s(&o.path)),
                        ("rev", n(o.doc.revision())),
                        ("dirty", Value::Bool(o.history.dirty())),
                    ]));
                }
                for (id, o) in &self.custom_open {
                    open_docs.push(obj(vec![
                        ("id", s(id)),
                        ("kind", s(o.kind)),
                        ("path", s(&o.path)),
                        ("rev", n(o.doc.revision())),
                        ("dirty", Value::Bool(o.history.dirty())),
                    ]));
                }
                Ok(obj(vec![
                    ("ok", Value::Bool(true)),
                    ("project", self.project.manifest.to_value()),
                    ("openDocuments", Value::Array(open_docs)),
                    ("connectedClients", n(self.clients.len() as u64)),
                    ("selection", self.mcp_selection_details()),
                ]))
            }
            "studio_selection" => Ok(self.mcp_selection_details()),
            "catalog_search" => {
                let cat = crate::ai::catalog::build_catalog(&self.project, &self.registry, true)?;
                let query = args.get("query").and_then(|v| v.string().ok());
                let kind = args.get("kind").and_then(|v| v.string().ok());
                let limit = args
                    .get("limit")
                    .and_then(|v| v.unsigned().ok())
                    .unwrap_or(32) as usize;
                let hits = cat.search(query, kind, limit);
                let lines: Vec<Value> = hits.iter().map(|it| s(&it.signature)).collect();
                let items: Vec<Value> = hits
                    .into_iter()
                    .map(crate::ai::catalog::CatalogItem::to_json)
                    .collect();
                Ok(obj(vec![
                    ("ok", Value::Bool(true)),
                    ("count", n(items.len() as u64)),
                    ("signatures", Value::Array(lines)),
                    ("items", Value::Array(items)),
                ]))
            }
            "catalog_inspect" => {
                let r = args
                    .get("ref")
                    .and_then(|v| v.string().ok())
                    .ok_or("catalog_inspect requires ref")?;
                let cat = crate::ai::catalog::build_catalog(&self.project, &self.registry, false)?;
                crate::ai::catalog::inspect_item(&cat, &self.project, r)
            }
            "brick_crystallize" => {
                let doc_id = self.resolve_active_or_explicit_doc(args, None)?;
                let brick_id = args
                    .get("id")
                    .and_then(|v| v.string().ok())
                    .ok_or("brick_crystallize requires id")?;
                let from_tx = args.get("fromTx") == Some(&Value::Bool(true));
                let ents: Vec<String> = args
                    .get("entities")
                    .and_then(|v| v.array().ok())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|x| x.string().ok().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let params: Vec<String> = args
                    .get("params")
                    .and_then(|v| v.array().ok())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|x| x.string().ok().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                self.crystallize_doc_internal(&doc_id, None, brick_id, &ents, from_tx, &params)
            }
            "scene_gbs" => {
                let doc_id = self.resolve_active_or_explicit_doc(args, None)?;
                let script = args
                    .get("script")
                    .and_then(|v| v.string().ok())
                    .ok_or("scene_gbs requires script")?;
                let dry_run = args.get("dryRun") == Some(&Value::Bool(true));
                self.apply_gbs_internal(MCP_CLIENT, &doc_id, script, dry_run, None, outbox)
            }
            "scene_verify" => {
                let doc_id = self.resolve_active_or_explicit_doc(args, None)?;
                let cam = args
                    .get("camera")
                    .and_then(|v| v.string().ok())
                    .map(String::from);
                let sheet_path = args.get("contactSheetPath").and_then(|v| v.string().ok());
                self.verify_doc_internal(
                    &doc_id,
                    None,
                    cam,
                    320,
                    180,
                    sheet_path.is_some(),
                    sheet_path,
                )
            }
            "doc_open" => {
                let (id, hint_path) = self.doc_target(args)?;
                if self.is_custom_target(&id, hint_path.as_deref()) {
                    let _ = self.ensure_custom_open(&id, hint_path.as_deref())?;
                    Ok(self.custom_doc_message(&id))
                } else {
                    let _ = self.ensure_open(&id)?;
                    Ok(self.doc_message(&id))
                }
            }
            "doc_edit" => {
                let doc_id = self.resolve_active_or_explicit_doc(args, None)?;
                let ops_val = args.get("ops").cloned().ok_or("doc_edit requires ops")?;
                let label = args
                    .get("label")
                    .and_then(|v| v.string().ok())
                    .unwrap_or("MCP Edit");
                let cur_rev = if self.custom_open.contains_key(&doc_id) {
                    self.custom_open[&doc_id].doc.revision()
                } else {
                    let _ = self.ensure_open(&doc_id)?;
                    self.open[&doc_id].doc.revision()
                };
                let edit_msg = obj(vec![
                    ("type", s("edit")),
                    ("doc", s(&doc_id)),
                    ("rev", n(cur_rev)),
                    ("label", s(label)),
                    ("ops", ops_val),
                ]);
                let sent = self.edit(MCP_CLIENT, &edit_msg)?;
                outbox.extend(sent);
                let new_rev = if let Some(o) = self.custom_open.get(&doc_id) {
                    o.doc.revision()
                } else {
                    self.open[&doc_id].doc.revision()
                };
                Ok(obj(vec![
                    ("ok", Value::Bool(true)),
                    ("doc", s(&doc_id)),
                    ("rev", n(new_rev)),
                ]))
            }
            "doc_undo" | "doc_redo" => {
                let doc_id = self.resolve_active_or_explicit_doc(args, None)?;
                let ty = if name == "doc_undo" { "undo" } else { "redo" };
                let sent =
                    self.dispatch(MCP_CLIENT, &obj(vec![("type", s(ty)), ("doc", s(&doc_id))]))?;
                let changed = !sent.is_empty();
                outbox.extend(sent);
                let rev = if let Some(o) = self.custom_open.get(&doc_id) {
                    o.doc.revision()
                } else if let Some(o) = self.open.get(&doc_id) {
                    o.doc.revision()
                } else {
                    0
                };
                Ok(obj(vec![
                    ("ok", Value::Bool(true)),
                    ("doc", s(&doc_id)),
                    ("changed", Value::Bool(changed)),
                    ("rev", n(rev)),
                ]))
            }
            "doc_save" => {
                let doc_id = self.resolve_active_or_explicit_doc(args, None)?;
                let force = args.get("force") == Some(&Value::Bool(true));
                let sent = self.dispatch(
                    MCP_CLIENT,
                    &obj(vec![
                        ("type", s("save")),
                        ("doc", s(&doc_id)),
                        ("force", Value::Bool(force)),
                    ]),
                )?;
                outbox.extend(sent);
                let (path, rev) = if let Some(o) = self.custom_open.get(&doc_id) {
                    (o.path.clone(), o.doc.revision())
                } else {
                    let o = &self.open[&doc_id];
                    (o.path.clone(), o.doc.revision())
                };
                Ok(obj(vec![
                    ("ok", Value::Bool(true)),
                    ("doc", s(&doc_id)),
                    ("path", s(&path)),
                    ("rev", n(rev)),
                ]))
            }
            "world_graph_fetch" => self.world_graph_fetch(),
            "world_graph_patch" => {
                let patch = args.get("patch").unwrap_or(args);
                self.world_graph_patch(patch, outbox)
            }
            "world_graph_prompt" => {
                let prompt = args
                    .get("prompt")
                    .and_then(|v| v.string().ok())
                    .ok_or("world_graph_prompt requires prompt")?;
                self.world_graph_prompt(prompt, outbox)
            }
            "world_graph_benchmark" => self.world_graph_benchmark(args),
            other => Err(format!("unknown MCP tool {other:?}")),
        }
    }

    /// Reads `<project_root>/world.semantic.json` (initializing clean `120Hz` defaults on first read).
    pub fn world_graph_read_or_init(
        &mut self,
    ) -> Result<(crate::ai::world_graph::SemanticWorldGraph, Vec<String>), String> {
        self.inside(crate::ai::world_graph::WORLD_GRAPH_REL_PATH)?;
        crate::ai::world_graph::load_or_init_world_graph(&self.project.root)
    }

    /// Returns the structured `world_graph_fetch` payload for `ganbate://context`.
    pub fn world_graph_fetch(&mut self) -> Result<Value, String> {
        let (graph, notices) = self.world_graph_read_or_init()?;
        Ok(obj(vec![
            ("ok", Value::Bool(true)),
            ("uri", s(crate::ai::world_graph::WORLD_GRAPH_URI)),
            ("path", s(crate::ai::world_graph::WORLD_GRAPH_REL_PATH)),
            ("estimated_tokens", n(graph.estimated_tokens() as u64)),
            (
                "guardrail_notices",
                Value::Array(notices.into_iter().map(|m| s(&m)).collect()),
            ),
            ("graph", graph.to_json_value()),
        ]))
    }

    /// Applies a JSON patch to `world.semantic.json` under `120Hz` and server-authority guardrails,
    /// persisting and broadcasting updates to all connected Living Canvas clients.
    pub fn world_graph_patch(
        &mut self,
        patch: &Value,
        outbox: &mut Outbox,
    ) -> Result<Value, String> {
        let (current, _) = self.world_graph_read_or_init()?;
        let outcome = crate::ai::world_graph::apply_world_graph_patch(&current, patch)?;
        if !outcome.blocked_by_authority && outcome.applied {
            self.inside(crate::ai::world_graph::WORLD_GRAPH_REL_PATH)?;
            let _ = crate::ai::world_graph::save_world_graph(&self.project.root, &outcome.graph)?;
            outbox.extend(outcome.build_outbox(self.clients.keys().copied()));
        }
        Ok(outcome.to_json_value())
    }

    /// Compiles a bilingual (EN/KO) natural-language prompt against `world.semantic.json` under
    /// `120Hz` and server-authority guardrails, persisting and broadcasting updates via `outbox`.
    pub fn world_graph_prompt(
        &mut self,
        prompt: &str,
        outbox: &mut Outbox,
    ) -> Result<Value, String> {
        let (current, _) = self.world_graph_read_or_init()?;
        let recipients: Vec<ClientId> = self.clients.keys().copied().collect();
        let (outcome, prompt_outbox) =
            crate::ai::world_graph::compile_world_graph_prompt(&current, prompt, &recipients);
        if !outcome.blocked_by_authority && outcome.applied {
            self.inside(crate::ai::world_graph::WORLD_GRAPH_REL_PATH)?;
            let _ = crate::ai::world_graph::save_world_graph(&self.project.root, &outcome.graph)?;
            outbox.extend(prompt_outbox);
        }
        Ok(outcome.to_json_value())
    }

    /// Runs the reproducible read-only academic benchmark suite (`world_graph_benchmark`) against
    /// `self.project.root`, optionally attaching rendered `latex` or `markdown` text when `format` is requested.
    pub fn world_graph_benchmark(&self, args: &Value) -> Result<Value, String> {
        let iterations = match args.get("iterations") {
            Some(v) => {
                let raw = v
                    .unsigned()
                    .map_err(|_| "iterations must be a positive integer".to_string())?;
                if raw == 0 {
                    return Err("iterations must be >= 1".into());
                }
                (raw as usize).min(crate::ai::bench::MAX_ITERATIONS)
            }
            None => crate::ai::bench::DEFAULT_ITERATIONS,
        };
        let requested_format = match args.get("format").and_then(|v| v.string().ok()) {
            Some(raw) if !raw.trim().is_empty() => Some(crate::ai::bench::BenchFormat::parse(raw)?),
            _ => None,
        };
        let report = crate::ai::bench::run_benchmark(&self.project.root, iterations)?;
        let mut val = report.to_json_value();
        if let (Some(fmt), Value::Object(map)) = (requested_format, &mut val) {
            map.insert("format".into(), s(fmt.as_str()));
            map.insert("rendered".into(), Value::String(report.render(fmt, false)?));
        }
        Ok(val)
    }
}

/// Refuses `rel`, a path in the project folder `root` (existing or about to be made), when the
/// part of it that exists resolves outside `real_root` (`root` with every symlink resolved).
fn inside(root: &Path, real_root: &Path, rel: &str) -> Result<(), String> {
    let path = root.join(rel);
    let mut p = path.as_path();
    loop {
        match std::fs::canonicalize(p) {
            Ok(real) if real.starts_with(real_root) => return Ok(()),
            Ok(_) => break,
            // Not made yet: what it will be made in decides.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => match p.parent() {
                Some(up) => p = up,
                None => break,
            },
            Err(_) => break,
        }
    }
    Err(format!("{rel} leads outside the project"))
}

/// Whether a file name is a cooked preview's: `<document id>.<16 hex digits>.gbew`.
fn is_preview_name(name: &str) -> bool {
    let mut parts = name.split('.');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(id), Some(hash), Some("gbew"), None) => {
            is_asset_id(id)
                && hash.len() == 16
                && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        }
        _ => false,
    }
}

/// Deletes the cooked previews in `dir` (and nothing else there).
fn sweep_previews(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        if e.file_name().to_str().is_some_and(is_preview_name) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Temp(std::path::PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn studio(tag: &str) -> (Studio, Temp) {
        let dir =
            std::env::temp_dir().join(format!("ganbate-studio-{tag}-{}", Ids::from_os().entity()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut ids = Ids::seeded(5);
        Project::create(&dir, "Studio test", &mut ids).unwrap();
        let (st, _) = Studio::open(&dir, ids).unwrap();
        (st, Temp(dir))
    }

    fn msg(text: &str) -> Value {
        json::parse(text.as_bytes()).unwrap()
    }
    fn of_type<'a>(out: &'a Outbox, ty: &str) -> Vec<&'a (ClientId, Value)> {
        out.iter()
            .filter(|(_, m)| m.get("type").and_then(|t| t.string().ok()) == Some(ty))
            .collect()
    }

    #[test]
    fn a_look_is_a_shared_undoable_document_with_its_existing_sidecar() {
        let (_, t) = studio("look");
        let path = t.0.join("content/orchard.look.json");
        let original =
            msg(r##"{"shading":{"rim_strength":0.2},"notes":{"vendor":"leave intact"}}"##);
        ganbate_pipeline::project::write_json(&path, &original).unwrap();
        let (mut st, _) = Studio::open(&t.0, Ids::seeded(9)).unwrap();
        let id = st
            .project
            .find_path("content/orchard.look.json")
            .unwrap()
            .id
            .clone();
        let sidecar = ganbate_pipeline::project::sidecar_path(&path);
        let meta = std::fs::read(&sidecar).unwrap();
        let a = st.connect("artist");
        let b = st.connect("viewer");
        let send = |st: &mut Studio, who, message: &str| {
            st.handle(who, &msg(&message.replace("$id", &id)))
        };
        for client in [a, b] {
            let opened = send(&mut st, client, r#"{"type":"open","doc":"$id"}"#);
            assert_eq!(opened[0].1.get("kind"), Some(&s("look")));
            assert!(
                opened[0]
                    .1
                    .get("issues")
                    .unwrap()
                    .array()
                    .unwrap()
                    .is_empty()
            );
        }
        let edit = r#"{"type":"edit","doc":"$id","rev":0,"label":"Rim","ops":[{"set":["shading","rim_strength"],"value":0.75}]}"#;
        assert_eq!(of_type(&send(&mut st, a, edit), "ops").len(), 2);
        assert_eq!(of_type(&send(&mut st, b, edit), "doc").len(), 1);
        let bad = r#"{"type":"edit","doc":"$id","rev":1,"ops":[{"set":["post"],"value":3}]}"#;
        assert_eq!(of_type(&send(&mut st, a, bad), "error").len(), 1);
        assert_eq!(st.open[&id].doc.revision(), 1);
        send(&mut st, b, r#"{"type":"undo","doc":"$id"}"#);
        assert_eq!(st.open[&id].doc.root(), &original);
        send(&mut st, a, r#"{"type":"redo","doc":"$id"}"#);
        let saved = send(&mut st, a, r#"{"type":"save","doc":"$id"}"#);
        assert_eq!(of_type(&saved, "saved").len(), 2);
        assert_eq!(std::fs::read(&sidecar).unwrap(), meta);
        let (mut reopened, _) = Studio::open(&t.0, Ids::seeded(11)).unwrap();
        let c = reopened.connect("new");
        send(&mut reopened, c, r#"{"type":"open","doc":"$id"}"#);
        let root = reopened.open[&id].doc.root();
        assert_eq!(root.get("notes"), original.get("notes"));
        assert_eq!(
            root.get("shading").unwrap().get("rim_strength"),
            Some(&Value::Number(0.75))
        );
        assert!(root.get("id").is_none());
        // Changes on disk need explicit overwrite. Even that cannot overwrite another asset.
        ganbate_pipeline::project::write_json(&path, &original).unwrap();
        let err = send(&mut reopened, c, r#"{"type":"save","doc":"$id"}"#);
        assert_eq!(err[0].1.get("code"), Some(&s(CHANGED_ON_DISK)));
        let other =
            ganbate_pipeline::project::new_sidecar("0123456789abcdef0123456789abcdef", Kind::Look);
        ganbate_pipeline::project::write_json(&sidecar, &other).unwrap();
        let err = send(
            &mut reopened,
            c,
            r#"{"type":"save","force":true,"doc":"$id"}"#,
        );
        assert_eq!(of_type(&err, "error").len(), 1);
        assert_eq!(
            ganbate_pipeline::project::read_json(&path).unwrap(),
            original
        );
    }

    #[test]
    fn new_looks_are_registered_once_and_can_finish_an_interrupted_creation() {
        let (mut st, t) = studio("new-look");
        let a = st.connect("artist");
        let rel = "content/looks/morning.look.json";
        let interrupted_id = "abcdef0123456789abcdef0123456789";
        let sidecar = ganbate_pipeline::project::sidecar_path(&t.0.join(rel));
        let meta = ganbate_pipeline::project::new_sidecar(interrupted_id, Kind::Look);
        ganbate_pipeline::project::write_json(&sidecar, &meta).unwrap();
        let out = create(&mut st, a, "look", rel);
        assert_eq!(created_doc(&out), interrupted_id);
        let file = t.0.join(rel);
        assert_eq!(
            ganbate_pipeline::project::read_json(&file).unwrap(),
            msg("{}")
        );
        assert!(error_of(&create(&mut st, a, "look", rel)).contains("already exists"));
        assert_eq!(
            ganbate_pipeline::project::read_json(&sidecar).unwrap(),
            meta
        );
        let new = create(&mut st, a, "look", "content/new.look.json");
        let id = created_doc(&new);
        assert!(is_asset_id(&id));
        let (reopened, _) = Studio::open(&t.0, Ids::seeded(32)).unwrap();
        assert_eq!(
            reopened.project.entry(&id).unwrap().path,
            "content/new.look.json"
        );
    }

    #[test]
    fn rescanning_renamed_assets_keeps_edits_and_notifies_both_viewers() {
        let (mut st, t) = studio("rescan-moved");
        let a = st.connect("artist");
        let b = st.connect("reviewer");
        let old = "content/day.look.json";
        let moved = "content/new-day.look.json";
        let id = created_doc(&create(&mut st, a, "look", old));
        let send = |st: &mut Studio, who, message: &str| {
            st.handle(who, &msg(&message.replace("$id", &id)))
        };
        for who in [a, b] {
            send(&mut st, who, r#"{"type":"open","doc":"$id"}"#);
        }
        send(
            &mut st,
            a,
            r#"{"type":"edit","doc":"$id","rev":0,"label":"Exposure","ops":[{"set":["post","exposure"],"value":0.75}]}"#,
        );
        std::fs::rename(t.0.join(old), t.0.join(moved)).unwrap();
        std::fs::rename(
            t.0.join(format!("{old}.meta.json")),
            t.0.join(format!("{moved}.meta.json")),
        )
        .unwrap();
        let outside = msg(r#"{"about":"changed by artist"}"#);
        ganbate_pipeline::project::write_json(&t.0.join(moved), &outside).unwrap();
        std::fs::write(t.0.join("content/new.glb"), b"model").unwrap();
        let out = send(&mut st, b, r#"{"type":"rescan"}"#);
        assert_eq!(of_type(&out, "assets").len(), 2);
        let report = &of_type(&out, "rescanned")[0].1;
        assert_eq!(report.get("ok"), Some(&Value::Bool(true)));
        assert_eq!(
            report.get("moved").unwrap().array().unwrap()[0].get("id"),
            Some(&s(&id))
        );
        assert!(st.project.find_path("content/new.glb").is_some());
        let docs = of_type(&out, "doc");
        assert_eq!(docs.len(), 2);
        assert!(
            docs.iter()
                .all(|(_, doc)| doc.get("dirty") == Some(&Value::Bool(true))
                    && doc.get("path") == Some(&s(moved)))
        );
        assert_eq!(docs[0].1.get("rev"), Some(&n(1)));
        assert_eq!(docs[0].1.get("undo"), Some(&s("Exposure")));
        assert_eq!(docs[0].1.get("changedOnDisk"), Some(&Value::Bool(true)));
        let refused = send(&mut st, a, r#"{"type":"save","doc":"$id"}"#);
        assert_eq!(
            of_type(&refused, "error")[0].1.get("code"),
            Some(&s(CHANGED_ON_DISK))
        );
        assert_eq!(
            ganbate_pipeline::project::read_json(&t.0.join(moved)).unwrap(),
            outside
        );
        send(&mut st, a, r#"{"type":"save","force":true,"doc":"$id"}"#);
        assert!(!t.0.join(old).exists());
        assert_eq!(
            st.project
                .read_document(&id)
                .unwrap()
                .get("post")
                .unwrap()
                .get("exposure"),
            Some(&Value::Number(0.75))
        );
        send(&mut st, b, r#"{"type":"undo","doc":"$id"}"#);
        assert_eq!(st.open[&id].doc.root(), &msg("{}"));
        send(&mut st, b, r#"{"type":"redo","doc":"$id"}"#);
        ganbate_pipeline::project::write_json(&t.0.join(moved), &outside).unwrap();
        let refreshed = send(&mut st, a, r#"{"type":"rescan"}"#);
        assert!(
            of_type(&refreshed, "doc")
                .iter()
                .all(|(_, doc)| doc.get("root") == Some(&outside))
        );
    }

    #[test]
    fn rescan_keeps_an_orphan_draft_and_never_replaces_another_identity() {
        let (mut st, t) = studio("rescan-orphan");
        let a = st.connect("artist");
        let path = "content/main.scene.json";
        let id = created_doc(&create(&mut st, a, "scene", path));
        let send =
            |st: &mut Studio, message: &str| st.handle(a, &msg(&message.replace("$id", &id)));
        send(&mut st, r#"{"type":"open","doc":"$id"}"#);
        send(
            &mut st,
            r#"{"type":"edit","doc":"$id","rev":0,"ops":[{"set":["name"],"value":"Draft"}]}"#,
        );
        let original = st.project.read_document(&id).unwrap();
        std::fs::remove_file(t.0.join(path)).unwrap();
        assert!(
            !of_type(
                &send(&mut st, r#"{"type":"save","force":true,"doc":"$id"}"#),
                "error"
            )
            .is_empty()
        );
        assert!(!t.0.join(path).exists());
        let out = send(&mut st, r#"{"type":"rescan"}"#);
        let doc = &of_type(&out, "doc")[0].1;
        assert!(
            doc.get("unavailable")
                .unwrap()
                .string()
                .unwrap()
                .contains("missing")
        );
        assert_eq!(doc.get("root").unwrap().get("name"), Some(&s("Draft")));
        assert_eq!(
            of_type(&out, "rescanned")[0]
                .1
                .get("drafts")
                .unwrap()
                .array()
                .unwrap()
                .len(),
            1
        );
        let replacement = scene::new_document(
            scene::Kind::Scene,
            "abcdef0123456789abcdef0123456789",
            "Replacement",
        );
        ganbate_pipeline::project::write_json(&t.0.join(path), &replacement).unwrap();
        send(&mut st, r#"{"type":"rescan"}"#);
        let denied = send(&mut st, r#"{"type":"save","force":true,"doc":"$id"}"#);
        assert!(!of_type(&denied, "error").is_empty());
        assert_eq!(
            ganbate_pipeline::project::read_json(&t.0.join(path)).unwrap(),
            replacement
        );
        ganbate_pipeline::project::write_json(&t.0.join(path), &original).unwrap();
        send(&mut st, r#"{"type":"rescan"}"#);
        let restored = send(&mut st, r#"{"type":"save","doc":"$id"}"#);
        assert_eq!(of_type(&restored, "saved").len(), 1);
        assert_eq!(
            st.project.read_document(&id).unwrap().get("name"),
            Some(&s("Draft"))
        );
    }

    #[test]
    fn duplicate_rescan_keeps_the_previous_index_and_creates_no_sidecars() {
        let (mut st, t) = studio("rescan-duplicate");
        let a = st.connect("artist");
        let b = st.connect("viewer");
        let id = created_doc(&create(&mut st, a, "look", "content/style.look.json"));
        let old = st.assets();
        std::fs::write(t.0.join("content/00new.glb"), b"not registered").unwrap();
        for suffix in ["", ".meta.json"] {
            std::fs::copy(
                t.0.join(format!("content/style.look.json{suffix}")),
                t.0.join(format!("content/copy.look.json{suffix}")),
            )
            .unwrap();
        }
        let out = st.handle(b, &msg(r#"{"type":"rescan"}"#));
        let reports = of_type(&out, "rescanned");
        assert_eq!(reports.len(), 2);
        assert_eq!(reports[0].1.get("ok"), Some(&Value::Bool(false)));
        assert!(
            reports[0].1.get("problems").unwrap().array().unwrap()[0]
                .string()
                .unwrap()
                .contains(&id)
        );
        assert!(of_type(&out, "assets").is_empty());
        assert_eq!(st.assets(), old);
        assert!(!t.0.join("content/00new.glb.meta.json").exists());
    }

    #[test]
    fn create_edit_undo_save_and_reopen() {
        let (mut st, t) = studio("edit");
        let a = st.connect("editor");
        let b = st.connect("phone");
        let out = st.handle(a, &msg(r#"{"type":"hello","client":"editor"}"#));
        assert_eq!(of_type(&out, "welcome").len(), 1);
        let out = st.handle(
            a,
            &msg(r#"{"type":"create","kind":"scene","path":"content/main.scene.json","name":"Main"}"#),
        );
        let created = of_type(&out, "created")[0].1.clone();
        let doc = created.get("doc").unwrap().string().unwrap().to_string();
        for c in [a, b] {
            st.handle(c, &msg(&format!(r#"{{"type":"open","doc":"{doc}"}}"#)));
        }
        let ids = st.handle(a, &msg(r#"{"type":"ids","count":2}"#));
        let e = ids[0].1.get("ids").unwrap().array().unwrap()[0]
            .string()
            .unwrap()
            .to_string();
        let edit = format!(
            r#"{{"type":"edit","doc":"{doc}","rev":0,"label":"Add","req":7,"ops":[{{"set":["entities","{e}"],"value":{{"name":"Lantern","components":{{"transform":{{"translation":[1,2,3]}}}}}}}}]}}"#
        );
        let out = st.handle(a, &msg(&edit));
        let ops = of_type(&out, "ops");
        assert_eq!(ops.len(), 2, "both watchers hear the edit");
        assert_eq!(ops[0].1.get("rev"), Some(&Value::Number(1.0)));
        assert_eq!(ops[0].1.get("req"), Some(&Value::Number(7.0)));
        // A stale revision is refused with the document attached.
        let out = st.handle(b, &msg(&edit));
        assert_eq!(of_type(&out, "error").len(), 1);
        assert_eq!(of_type(&out, "doc").len(), 1);
        // An edit that breaks the schema is refused and changes nothing.
        let bad = format!(
            r#"{{"type":"edit","doc":"{doc}","rev":1,"ops":[{{"set":["entities","{e}","components","transform","scale"],"value":[1,2]}}]}}"#
        );
        let out = st.handle(a, &msg(&bad));
        let err = of_type(&out, "error")[0]
            .1
            .get("message")
            .unwrap()
            .string()
            .unwrap()
            .to_string();
        assert!(err.contains("expected 3 numbers"), "{err}");
        // The refused edit left the revision alone: rev 1 still edits.
        let ok = format!(
            r#"{{"type":"edit","doc":"{doc}","rev":1,"ops":[{{"set":["entities","{e}","name"],"value":"Lamp"}}]}}"#
        );
        assert_eq!(of_type(&st.handle(a, &msg(&ok)), "ops").len(), 2);
        let ok_back = format!(
            r#"{{"type":"edit","doc":"{doc}","rev":2,"ops":[{{"set":["entities","{e}","name"],"value":"Lantern"}}]}}"#
        );
        assert_eq!(of_type(&st.handle(a, &msg(&ok_back)), "ops").len(), 2);
        // Header keys cannot be edited.
        let header = format!(
            r#"{{"type":"edit","doc":"{doc}","rev":3,"ops":[{{"set":["id"],"value":"x"}}]}}"#
        );
        assert_eq!(of_type(&st.handle(a, &msg(&header)), "error").len(), 1);
        // Save, then the file reopens with the entity.
        let out = st.handle(a, &msg(&format!(r#"{{"type":"save","doc":"{doc}"}}"#)));
        assert_eq!(of_type(&out, "saved").len(), 2);
        let saved = st.project.read_document(&doc).unwrap();
        assert!(scene::entities(&saved).contains_key(&e));
        // Undo removes it, redo restores it; the redo branch survived the refused edit.
        let out = st.handle(a, &msg(&format!(r#"{{"type":"undo","doc":"{doc}"}}"#)));
        assert_eq!(
            of_type(&out, "ops")[0].1.get("dirty"),
            Some(&Value::Bool(true))
        );
        let out = st.handle(a, &msg(&format!(r#"{{"type":"redo","doc":"{doc}"}}"#)));
        assert_eq!(
            of_type(&out, "ops")[0].1.get("dirty"),
            Some(&Value::Bool(false))
        );
        drop(t);
    }

    fn message_of(out: &Outbox) -> (String, Option<String>) {
        let e = &of_type(out, "error")[0].1;
        (
            e.get("message").unwrap().string().unwrap().to_string(),
            e.get("code")
                .and_then(|c| c.string().ok())
                .map(String::from),
        )
    }

    #[test]
    fn a_file_changed_on_disk_is_not_overwritten_and_is_read_again_when_clean() {
        let (mut st, t) = studio("disk");
        let a = st.connect("editor");
        let b = st.connect("viewer");
        let out = create(&mut st, a, "scene", "content/main.scene.json");
        let doc = created_doc(&out);
        for c in [a, b] {
            st.handle(c, &msg(&format!(r#"{{"type":"open","doc":"{doc}"}}"#)));
        }
        // Another tool (the ganbate command, git) rewrites the file: a new name.
        let path = t.0.join("content/main.scene.json");
        let mut disk = st.project.read_document(&doc).unwrap();
        if let Value::Object(m) = &mut disk {
            m.insert("name".into(), Value::String("Renamed outside".into()));
        }
        ganbate_pipeline::project::write_json(&path, &disk).unwrap();
        // Reopening a document with no unsaved edits reads it again, for every watcher.
        let out = st.handle(a, &msg(&format!(r#"{{"type":"open","doc":"{doc}"}}"#)));
        let docs = of_type(&out, "doc");
        assert_eq!(docs.len(), 2, "both watchers get the new text");
        assert_eq!(
            docs[0].1.get("root").unwrap().get("name"),
            Some(&Value::String("Renamed outside".into()))
        );
        // An unsaved edit, then the file changes again: the save is refused, not lost.
        let stale = format!(
            r#"{{"type":"edit","doc":"{doc}","rev":0,"ops":[{{"set":["name"],"value":"Old file"}}]}}"#
        );
        assert_eq!(of_type(&st.handle(b, &msg(&stale)), "error").len(), 1);
        let ok = format!(
            r#"{{"type":"edit","doc":"{doc}","rev":1,"ops":[{{"set":["name"],"value":"Mine"}}]}}"#
        );
        assert_eq!(of_type(&st.handle(a, &msg(&ok)), "ops").len(), 2);
        if let Value::Object(m) = &mut disk {
            m.insert("name".into(), Value::String("Theirs".into()));
        }
        ganbate_pipeline::project::write_json(&path, &disk).unwrap();
        // Reopening keeps the unsaved edit rather than dropping it.
        let out = st.handle(b, &msg(&format!(r#"{{"type":"open","doc":"{doc}"}}"#)));
        assert_eq!(
            of_type(&out, "doc")[0].1.get("root").unwrap().get("name"),
            Some(&Value::String("Mine".into()))
        );
        let out = st.handle(a, &msg(&format!(r#"{{"type":"save","doc":"{doc}"}}"#)));
        let (message, code) = message_of(&out);
        assert_eq!(code.as_deref(), Some(CHANGED_ON_DISK), "{message}");
        assert_eq!(
            st.project.read_document(&doc).unwrap().get("name"),
            Some(&Value::String("Theirs".into()))
        );
        // Saving with force overwrites; afterwards plain saves work again.
        let out = st.handle(
            a,
            &msg(&format!(r#"{{"type":"save","doc":"{doc}","force":true}}"#)),
        );
        assert_eq!(of_type(&out, "saved").len(), 2);
        assert_eq!(
            st.project.read_document(&doc).unwrap().get("name"),
            Some(&Value::String("Mine".into()))
        );
        let out = st.handle(a, &msg(&format!(r#"{{"type":"save","doc":"{doc}"}}"#)));
        assert_eq!(of_type(&out, "saved").len(), 2);
        drop(t);
    }

    #[test]
    fn edits_and_cooks_see_problems_that_only_prefab_expansion_shows() {
        let (mut st, t) = studio("prefabs");
        let a = st.connect("editor");
        // A prefab with a root and a child, and a scene with one instance of it.
        let out = create(&mut st, a, "prefab", "content/lamp.prefab.json");
        let prefab = created_doc(&out);
        // A new prefab already has its one root entity.
        let out = st.handle(a, &msg(&format!(r#"{{"type":"open","doc":"{prefab}"}}"#)));
        let opened = &of_type(&out, "doc")[0].1;
        let root_e = scene::entities(opened.get("root").unwrap())
            .keys()
            .next()
            .expect("the prefab's root")
            .clone();
        let child_e = "00000000000000a2";
        let edit = format!(
            r#"{{"type":"edit","doc":"{prefab}","rev":0,"ops":[{{"set":["entities","{child_e}"],"value":{{"name":"Bulb","parent":"{root_e}"}}}}]}}"#
        );
        let out = st.handle(a, &msg(&edit));
        assert_eq!(of_type(&out, "ops").len(), 1, "{out:?}");
        st.handle(a, &msg(&format!(r#"{{"type":"save","doc":"{prefab}"}}"#)));
        let out = create(&mut st, a, "scene", "content/street.scene.json");
        let doc = created_doc(&out);
        st.handle(a, &msg(&format!(r#"{{"type":"open","doc":"{doc}"}}"#)));
        let inst = "00000000000000b1";
        let edit = format!(
            r#"{{"type":"edit","doc":"{doc}","rev":0,"ops":[{{"set":["entities","{inst}"],"value":{{"name":"Lamp 1","prefab":{{"asset":"{prefab}"}}}}}}]}}"#
        );
        assert_eq!(of_type(&st.handle(a, &msg(&edit)), "ops").len(), 1);
        // The ids the instance shows, from a cook of the scene.
        let out = st.handle(a, &msg(&format!(r#"{{"type":"cook","doc":"{doc}"}}"#)));
        let cooked = &of_type(&out, "cooked")[0].1;
        let shown: Vec<String> = cooked
            .get("entities")
            .unwrap()
            .array()
            .unwrap()
            .iter()
            .filter(|e| e.get("name") == Some(&Value::String("Bulb".into())))
            .map(|e| e.get("id").unwrap().string().unwrap().to_string())
            .collect();
        assert_eq!(shown.len(), 1, "{cooked:?}");
        // Parenting the instance under what it shows is a cycle only expansion reveals.
        let cycle = format!(
            r#"{{"type":"edit","doc":"{doc}","rev":1,"ops":[{{"set":["entities","{inst}","parent"],"value":"{}"}}]}}"#,
            shown[0]
        );
        let out = st.handle(a, &msg(&cycle));
        let (message, _) = message_of(&out);
        assert!(message.contains("cycle"), "{message}");
        // A prefab that gained a cycle on disk is refused by name when cooking.
        let mut bad = st.project.read_document(&prefab).unwrap();
        if let Value::Object(top) = &mut bad
            && let Some(Value::Object(e)) = top.get_mut("entities")
            && let Some(Value::Object(r)) = e.get_mut(&root_e)
        {
            r.insert("parent".into(), Value::String(child_e.into()));
        }
        // The prefab is no longer open, so the cook reads it from disk.
        st.open.remove(&prefab);
        ganbate_pipeline::project::write_json(&t.0.join("content/lamp.prefab.json"), &bad).unwrap();
        let out = st.handle(a, &msg(&format!(r#"{{"type":"cook","doc":"{doc}"}}"#)));
        let (message, _) = message_of(&out);
        assert!(message.contains(&prefab), "{message}");
        drop(t);
    }

    #[test]
    fn deleting_an_instance_takes_what_was_added_under_what_it_shows() {
        let (mut st, t) = studio("delete");
        let a = st.connect("editor");
        let out = create(&mut st, a, "prefab", "content/post.prefab.json");
        let prefab = created_doc(&out);
        let out = st.handle(a, &msg(&format!(r#"{{"type":"open","doc":"{prefab}"}}"#)));
        let root_e = scene::entities(of_type(&out, "doc")[0].1.get("root").unwrap())
            .keys()
            .next()
            .unwrap()
            .clone();
        let child_e = "00000000000000c2";
        let add = format!(
            r#"{{"type":"edit","doc":"{prefab}","rev":0,"ops":[{{"set":["entities","{child_e}"],"value":{{"name":"Top","parent":"{root_e}"}}}}]}}"#
        );
        assert_eq!(of_type(&st.handle(a, &msg(&add)), "ops").len(), 1);
        st.handle(a, &msg(&format!(r#"{{"type":"save","doc":"{prefab}"}}"#)));
        let out = create(&mut st, a, "scene", "content/yard.scene.json");
        let doc = created_doc(&out);
        st.handle(a, &msg(&format!(r#"{{"type":"open","doc":"{doc}"}}"#)));
        let inst = "00000000000000d1";
        let add = format!(
            r#"{{"type":"edit","doc":"{doc}","rev":0,"ops":[{{"set":["entities","{inst}"],"value":{{"name":"Post 1","prefab":{{"asset":"{prefab}"}}}}}}]}}"#
        );
        assert_eq!(of_type(&st.handle(a, &msg(&add)), "ops").len(), 1);
        // A flag added under the "Top" the instance shows.
        let out = st.handle(a, &msg(&format!(r#"{{"type":"cook","doc":"{doc}"}}"#)));
        let top = of_type(&out, "cooked")[0]
            .1
            .get("entities")
            .unwrap()
            .array()
            .unwrap()
            .iter()
            .find(|e| e.get("name") == Some(&Value::String("Top".into())))
            .unwrap()
            .get("id")
            .unwrap()
            .string()
            .unwrap()
            .to_string();
        let flag = "00000000000000d2";
        let add = format!(
            r#"{{"type":"edit","doc":"{doc}","rev":1,"ops":[{{"set":["entities","{flag}"],"value":{{"name":"Flag","parent":"{top}"}}}}]}}"#
        );
        assert_eq!(of_type(&st.handle(a, &msg(&add)), "ops").len(), 1);
        let out = st.handle(
            a,
            &msg(&format!(
                r#"{{"type":"delete","doc":"{doc}","rev":2,"ids":["{inst}"],"req":5}}"#
            )),
        );
        let ops = of_type(&out, "ops");
        assert_eq!(ops.len(), 1, "{out:?}");
        assert_eq!(ops[0].1.get("req"), Some(&Value::Number(5.0)));
        let root = st.open[&doc].doc.root();
        assert!(scene::entities(root).is_empty(), "{root:?}");
        // Nothing left to delete is an error, not an empty edit.
        let out = st.handle(
            a,
            &msg(&format!(
                r#"{{"type":"delete","doc":"{doc}","rev":3,"ids":["{inst}"]}}"#
            )),
        );
        assert_eq!(of_type(&out, "error").len(), 1);
        drop(t);
    }

    fn created_doc(out: &Outbox) -> String {
        of_type(out, "created")[0]
            .1
            .get("doc")
            .unwrap()
            .string()
            .unwrap()
            .to_string()
    }

    fn create(st: &mut Studio, from: ClientId, kind: &str, path: &str) -> Outbox {
        st.handle(
            from,
            &msg(&format!(
                r#"{{"type":"create","kind":"{kind}","path":"{path}","name":"N"}}"#
            )),
        )
    }

    fn error_of(out: &Outbox) -> String {
        of_type(out, "error")
            .first()
            .map(|(_, m)| m.get("message").unwrap().string().unwrap().to_string())
            .unwrap_or_default()
    }

    #[test]
    fn a_batch_that_fails_partway_leaves_the_revision_alone() {
        let (mut st, _t) = studio("batch");
        let a = st.connect("editor");
        let b = st.connect("phone");
        let doc = created_doc(&create(&mut st, a, "scene", "content/main.scene.json"));
        for c in [a, b] {
            st.handle(c, &msg(&format!(r#"{{"type":"open","doc":"{doc}"}}"#)));
        }
        // The first op applies, the second cannot (it goes through a string): the batch is
        // taken back, and only the sender hears of it.
        let bad = format!(
            r#"{{"type":"edit","doc":"{doc}","rev":0,"req":1,"ops":[{{"set":["name"],"value":"X"}},{{"set":["name","inner"],"value":1}}]}}"#
        );
        let out = st.handle(a, &msg(&bad));
        assert!(out.iter().all(|(c, _)| *c == a), "{out:?}");
        assert_eq!(of_type(&out, "error").len(), 1);
        // So b, which saw revision 0, is still current: its edit goes through as revision 1.
        let good = format!(
            r#"{{"type":"edit","doc":"{doc}","rev":0,"req":2,"ops":[{{"set":["name"],"value":"Renamed"}}]}}"#
        );
        let out = st.handle(b, &msg(&good));
        assert_eq!(of_type(&out, "error"), Vec::<&(ClientId, Value)>::new());
        let ops = of_type(&out, "ops");
        assert_eq!(ops.len(), 2, "both watchers hear the edit");
        assert_eq!(ops[0].1.get("rev"), Some(&Value::Number(1.0)));
        let reopened = st.handle(a, &msg(&format!(r#"{{"type":"open","doc":"{doc}"}}"#)));
        let root = of_type(&reopened, "doc")[0].1.get("root").unwrap();
        assert_eq!(root.get("name").unwrap().string().unwrap(), "Renamed");
    }

    #[test]
    fn new_documents_go_where_the_project_scan_finds_them() {
        let (mut st, t) = studio("paths");
        let a = st.connect("editor");
        // The deepest folder the scan reads is fine, and the project opens again after it.
        let deepest = format!("content/{}x.scene.json", "d/".repeat(MAX_FOLDER_DEPTH));
        assert_eq!(
            of_type(&create(&mut st, a, "scene", &deepest), "created").len(),
            1
        );
        // One folder deeper would leave a project that no longer opens: refused, nothing made.
        let deeper = format!("content/{}x.scene.json", "e/".repeat(MAX_FOLDER_DEPTH + 1));
        let err = error_of(&create(&mut st, a, "scene", &deeper));
        assert!(err.contains("folders deep"), "{err}");
        assert!(!t.0.join("content/e").exists());
        // Hidden names (the scan skips them), empty and relative parts are refused too.
        for bad in [
            "content/.hidden/x.scene.json",
            "content/.x.scene.json",
            "content//x.scene.json",
            "content/./x.scene.json",
            "content/../x.scene.json",
            // `content/a\..\x.scene.json`, escaped for JSON.
            "content/a\\\\..\\\\x.scene.json",
        ] {
            let out = create(&mut st, a, "scene", bad);
            assert!(of_type(&out, "created").is_empty(), "{bad} was created");
        }
        assert!(Studio::open(&t.0, Ids::seeded(6)).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn the_host_never_reads_or_writes_through_a_symlink_out_of_the_project() {
        let (mut st, t) = studio("links");
        let outside = Temp(t.0.with_extension("outside"));
        std::fs::create_dir_all(&outside.0).unwrap();
        std::os::unix::fs::symlink(&outside.0, t.0.join("content/shared")).unwrap();
        let a = st.connect("editor");
        // Create through a symlinked folder: refused, nothing written outside.
        let err = error_of(&create(&mut st, a, "scene", "content/shared/x.scene.json"));
        assert!(err.contains("outside the project"), "{err}");
        assert_eq!(std::fs::read_dir(&outside.0).unwrap().count(), 0);
        // A symlink that stays inside the project is fine.
        std::fs::create_dir_all(t.0.join("content/real")).unwrap();
        std::os::unix::fs::symlink(t.0.join("content/real"), t.0.join("content/alias")).unwrap();
        assert_eq!(
            of_type(
                &create(&mut st, a, "scene", "content/alias/y.scene.json"),
                "created"
            )
            .len(),
            1
        );
        // A document whose folder is swapped for a symlink out is not saved through it.
        let doc = created_doc(&create(&mut st, a, "scene", "content/moved/z.scene.json"));
        st.handle(a, &msg(&format!(r#"{{"type":"open","doc":"{doc}"}}"#)));
        std::fs::remove_dir_all(t.0.join("content/moved")).unwrap();
        std::os::unix::fs::symlink(&outside.0, t.0.join("content/moved")).unwrap();
        let err = error_of(&st.handle(a, &msg(&format!(r#"{{"type":"save","doc":"{doc}"}}"#))));
        assert!(err.contains("outside the project"), "{err}");
        // Nor is a document read through one: a scene that the scan found in a content folder
        // which now leads out is not opened.
        let other = created_doc(&create(&mut st, a, "scene", "content/later/w.scene.json"));
        std::fs::rename(t.0.join("content/later"), outside.0.join("later")).unwrap();
        std::os::unix::fs::symlink(outside.0.join("later"), t.0.join("content/later")).unwrap();
        let err = error_of(&st.handle(a, &msg(&format!(r#"{{"type":"open","doc":"{other}"}}"#))));
        assert!(err.contains("outside the project"), "{err}");
        std::fs::remove_dir_all(outside.0.join("later")).unwrap();
        // Nor are previews cooked into a cache folder that leads out.
        std::fs::create_dir_all(t.0.join(".ganbate")).unwrap();
        std::os::unix::fs::symlink(&outside.0, t.0.join(".ganbate/cooked")).unwrap();
        let err = error_of(&st.handle(a, &msg(&format!(r#"{{"type":"cook","doc":"{doc}"}}"#))));
        assert!(err.contains("outside the project"), "{err}");
        assert_eq!(std::fs::read_dir(&outside.0).unwrap().count(), 0);
    }

    #[test]
    fn only_the_latest_previews_are_kept() {
        let (mut st, t) = studio("previews");
        let a = st.connect("editor");
        let doc = created_doc(&create(&mut st, a, "scene", "content/main.scene.json"));
        st.handle(a, &msg(&format!(r#"{{"type":"open","doc":"{doc}"}}"#)));
        let cooked = t.0.join(".ganbate/cooked");
        let mut files = vec![];
        for i in 0..20u64 {
            let edit = format!(
                r#"{{"type":"edit","doc":"{doc}","rev":{i},"ops":[{{"set":["name"],"value":"N{i}"}}]}}"#
            );
            assert_eq!(of_type(&st.handle(a, &msg(&edit)), "ops").len(), 1);
            let out = st.handle(a, &msg(&format!(r#"{{"type":"cook","doc":"{doc}"}}"#)));
            let file = of_type(&out, "cooked")[0]
                .1
                .get("file")
                .unwrap()
                .string()
                .unwrap();
            files.push(t.0.join(file));
            // Each distinct cook is a new file; only the latest few stay.
            let left = std::fs::read_dir(&cooked).unwrap().count();
            assert_eq!(left, (i as usize + 1).min(PREVIEWS_KEPT));
        }
        for (i, f) in files.iter().enumerate() {
            assert_eq!(
                f.exists(),
                i >= files.len() - PREVIEWS_KEPT,
                "{}",
                f.display()
            );
        }
        // Cooking an earlier state again brings back its file as the latest.
        let undo = st.handle(a, &msg(&format!(r#"{{"type":"undo","doc":"{doc}"}}"#)));
        assert_eq!(of_type(&undo, "ops").len(), 1);
        st.handle(a, &msg(&format!(r#"{{"type":"cook","doc":"{doc}"}}"#)));
        assert!(files[files.len() - 2].exists());
        assert_eq!(std::fs::read_dir(&cooked).unwrap().count(), PREVIEWS_KEPT);
        // Another session starts clean; files that are not previews are left alone.
        std::fs::write(cooked.join("notes.txt"), b"mine").unwrap();
        drop(st);
        let (_st, _) = Studio::open(&t.0, Ids::seeded(8)).unwrap();
        let left: Vec<_> = std::fs::read_dir(&cooked)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(left, vec![std::ffi::OsString::from("notes.txt")]);
    }

    #[test]
    fn previews_and_selections_go_to_the_other_watchers() {
        let (mut st, _t) = studio("relay");
        let a = st.connect("editor");
        let b = st.connect("game");
        let out = st.handle(
            a,
            &msg(r#"{"type":"create","kind":"prefab","path":"content/p.prefab.json","name":"P"}"#),
        );
        let doc = of_type(&out, "created")[0]
            .1
            .get("doc")
            .unwrap()
            .string()
            .unwrap()
            .to_string();
        for c in [a, b] {
            st.handle(c, &msg(&format!(r#"{{"type":"open","doc":"{doc}"}}"#)));
        }
        let out = st.handle(
            a,
            &msg(&format!(
                r#"{{"type":"select","doc":"{doc}","entities":[]}}"#
            )),
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, b);
        assert!(
            st.handle(a, &msg(r#"{"type":"nonsense"}"#))[0]
                .1
                .get("message")
                .is_some()
        );
    }

    #[test]
    fn asset_import_move_and_usage_preserve_identities_and_live_references() {
        let (mut st, t) = studio("asset-ops");
        let a = st.connect("artist");
        let b = st.connect("viewer");

        // 1. Import a PNG texture and a WAV sound over the protocol.
        let png_bytes = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR";
        let png_b64 = ganbate_core::digest::base64(png_bytes);
        let imported = st.handle(
            a,
            &msg(&format!(
                r#"{{"type":"asset-import","path":"content/textures/albedo.png","dataBase64":"{png_b64}","req":"imp-1"}}"#
            )),
        );
        let imp_msg = &of_type(&imported, "asset-imported")[0].1;
        let tex_id = imp_msg.get("id").unwrap().string().unwrap().to_string();
        assert_eq!(imp_msg.get("kind"), Some(&s("texture")));
        assert_eq!(of_type(&imported, "assets").len(), 2);
        assert!(t.0.join("content/textures/albedo.png").is_file());
        assert!(t.0.join("content/textures/albedo.png.meta.json").is_file());

        // Refuses overwriting existing path or invalid header or outside content folder.
        let dup = st.handle(
            a,
            &msg(&format!(
                r#"{{"type":"asset-import","path":"content/textures/albedo.png","dataBase64":"{png_b64}"}}"#
            )),
        );
        assert!(error_of(&dup).contains("already exists"));
        let bad_hdr = st.handle(
            a,
            &msg(&format!(
                r#"{{"type":"asset-import","path":"content/textures/notpng.png","dataBase64":"{}"}}"#,
                ganbate_core::digest::base64(b"hello")
            )),
        );
        assert!(error_of(&bad_hdr).contains("header"));
        let bad_dir = st.handle(
            a,
            &msg(&format!(
                r#"{{"type":"asset-import","path":"outside/albedo.png","dataBase64":"{png_b64}"}}"#
            )),
        );
        assert!(error_of(&bad_dir).contains("content folder"));

        // 2. Import a material referencing the texture, create a scene + look, and check usage.
        let mat_json = format!(r#"{{"albedo":"{tex_id}","roughness":0.4}}"#);
        let mat_imp = st.handle(
            a,
            &msg(&format!(
                r#"{{"type":"asset-import","path":"content/materials/stone.material.json","dataBase64":"{}"}}"#,
                ganbate_core::digest::base64(mat_json.as_bytes())
            )),
        );
        let mat_id = of_type(&mat_imp, "asset-imported")[0]
            .1
            .get("id")
            .unwrap()
            .string()
            .unwrap()
            .to_string();
        let a_imp_types: Vec<&str> = mat_imp
            .iter()
            .filter(|(c, _)| *c == a)
            .map(|(_, v)| v.get("type").unwrap().string().unwrap())
            .collect();
        assert_eq!(a_imp_types, vec!["assets", "asset-imported"]);

        let look_id = created_doc(&create(&mut st, a, "look", "content/looks/day.look.json"));
        let scene_id = created_doc(&create(
            &mut st,
            a,
            "scene",
            "content/scenes/main.scene.json",
        ));
        st.handle(a, &msg(&format!(r#"{{"type":"open","doc":"{scene_id}"}}"#)));
        st.handle(b, &msg(&format!(r#"{{"type":"open","doc":"{look_id}"}}"#)));
        st.handle(
            a,
            &msg(&format!(
                r#"{{"type":"edit","doc":"{scene_id}","rev":0,"ops":[{{"set":["entities","00000000000000e1"],"value":{{"name":"Sky","components":{{"environment":{{"look":"{look_id}"}}}}}}}}]}}"#
            )),
        );

        // Unsaved scene edit is immediately reflected in both project-wide counts and per-asset uses.
        let summary = st.handle(a, &msg(r#"{"type":"asset-usage","req":"u0"}"#));
        let counts = of_type(&summary, "asset-usage")[0].1.get("counts").unwrap();
        assert_eq!(counts.get(&look_id), Some(&n(1)));
        assert_eq!(counts.get(&tex_id), Some(&n(1)));
        assert_eq!(counts.get(&mat_id), None);

        let look_uses = st.handle(
            a,
            &msg(&format!(
                r#"{{"type":"asset-usage","id":"{look_id}","req":"u1"}}"#
            )),
        );
        let uses_arr = of_type(&look_uses, "asset-usage")[0]
            .1
            .get("uses")
            .unwrap()
            .array()
            .unwrap();
        assert_eq!(uses_arr.len(), 1);
        assert_eq!(uses_arr[0].get("doc"), Some(&s(&scene_id)));
        assert_eq!(uses_arr[0].get("entity"), Some(&s("00000000000000e1")));
        assert_eq!(uses_arr[0].get("entityName"), Some(&s("Sky")));
        assert_eq!(uses_arr[0].get("field"), Some(&s("environment.look")));

        // 3. Move/rename both the sidecar-backed Look (watched by b!) and the texture; IDs stay intact.
        let moved_look = st.handle(
            a,
            &msg(&format!(
                r#"{{"type":"asset-move","id":"{look_id}","to":"content/looks/sunset.look.json","req":"mv-1"}}"#
            )),
        );
        assert_eq!(of_type(&moved_look, "asset-moved").len(), 1);
        assert_eq!(of_type(&moved_look, "assets").len(), 2);
        let a_move_types: Vec<&str> = moved_look
            .iter()
            .filter(|(c, _)| *c == a)
            .map(|(_, v)| v.get("type").unwrap().string().unwrap())
            .collect();
        assert_eq!(a_move_types, vec!["assets", "asset-moved"]);
        let watcher_docs = of_type(&moved_look, "doc");
        assert_eq!(watcher_docs.len(), 1);
        assert_eq!(
            watcher_docs[0].1.get("path"),
            Some(&s("content/looks/sunset.look.json"))
        );
        assert!(!t.0.join("content/looks/day.look.json").exists());
        assert!(!t.0.join("content/looks/day.look.json.meta.json").exists());
        assert!(t.0.join("content/looks/sunset.look.json").is_file());
        assert!(
            t.0.join("content/looks/sunset.look.json.meta.json")
                .is_file()
        );

        // Changing kind via rename is refused.
        let wrong_ext = st.handle(
            a,
            &msg(&format!(
                r#"{{"type":"asset-move","id":"{tex_id}","to":"content/textures/albedo.glb"}}"#
            )),
        );
        assert!(!of_type(&wrong_ext, "error").is_empty());

        // Re-scanning from scratch finds the moved files under the exact same asset IDs.
        let (reopened, notes) = Studio::open(&t.0, Ids::seeded(99)).unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(
            reopened.project.entry(&look_id).unwrap().path,
            "content/looks/sunset.look.json"
        );
        assert_eq!(
            reopened.project.entry(&tex_id).unwrap().path,
            "content/textures/albedo.png"
        );
    }

    #[test]
    fn data_documents_validate_against_schemas_undo_save_track_usage_and_expose_game_workspaces() {
        let (_, t) = studio("data-doc");
        // Configure schemas and editor folders in manifest + studio-game.json + schema file.
        let manifest_path = t.0.join(ganbate_pipeline::project::MANIFEST);
        let mut manifest = ganbate_pipeline::project::read_json(&manifest_path).unwrap();
        if let Value::Object(map) = &mut manifest {
            map.insert("schemas".into(), Value::Array(vec![s("schemas")]));
            map.insert("editor".into(), Value::Array(vec![s("editor")]));
        }
        ganbate_pipeline::project::write_json(&manifest_path, &manifest).unwrap();
        std::fs::create_dir_all(t.0.join("schemas")).unwrap();
        std::fs::create_dir_all(t.0.join("editor")).unwrap();
        std::fs::write(
            t.0.join("editor/studio-game.json"),
            br#"{"version":1,"origin":"http://127.0.0.1:5173","entry":"/?studioPie=1"}"#,
        )
        .unwrap();
        let schema_json = msg(r#"{
              "ganbate": "schema",
              "version": 1,
              "components": {
                "hud.anchors": {
                  "label": "Observatory HUD",
                  "editor": "layout-rig",
                  "fields": {
                    "opacity": { "type": "float", "min": 0, "max": 1, "default": 1 },
                    "banner": { "type": "vec2", "unit": "norm", "default": [0.5, 0.1] },
                    "cue_sound": { "type": "asset", "kind": "sound" },
                    "cues": {
                      "type": "array",
                      "max": 4,
                      "items": {
                        "type": "object",
                        "fields": {
                          "delay": { "type": "float", "min": 0, "max": 10, "default": 0 },
                          "clip": { "type": "asset", "kind": "sound" }
                        }
                      }
                    }
                  }
                }
              }
            }"#);
        ganbate_pipeline::project::write_json(&t.0.join("schemas/hud.schema.json"), &schema_json)
            .unwrap();

        let (mut st, notes) = Studio::open(&t.0, Ids::seeded(41)).unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        let a = st.connect("designer");
        let b = st.connect("pie");

        // Welcome advertises gameWorkspaces alongside schemas.
        let hello = st.handle(a, &msg(r#"{"type":"hello","client":"designer"}"#));
        let welcome = &of_type(&hello, "welcome")[0].1;
        assert_eq!(
            welcome.get("gameWorkspaces"),
            Some(&Value::Array(vec![s("editor/studio-game.json")]))
        );

        // Creating with unknown or engine schema is refused.
        assert!(
            !of_type(
                &st.handle(
                    a,
                    &msg(
                        r#"{"type":"create","kind":"data","schema":"unknown.kind","path":"content/hud.data.json"}"#
                    ),
                ),
                "error"
            )
            .is_empty()
        );
        assert!(
            !of_type(
                &st.handle(
                    a,
                    &msg(
                        r#"{"type":"create","kind":"data","schema":"transform","path":"content/hud.data.json"}"#
                    ),
                ),
                "error"
            )
            .is_empty()
        );

        // Create valid data document in nested subfolder.
        let created = st.handle(
            a,
            &msg(
                r#"{"type":"create","kind":"data","schema":"hud.anchors","path":"content/data/hud.data.json","name":"Observatory HUD"}"#,
            ),
        );
        let created_msg = &of_type(&created, "created")[0].1;
        assert_eq!(created_msg.get("kind"), Some(&s("data")));
        let data_id = created_msg
            .get("doc")
            .unwrap()
            .string()
            .unwrap()
            .to_string();
        assert!(is_asset_id(&data_id));
        assert!(!t.0.join("content/data/hud.data.json.meta.json").exists());

        // Import a WAV sound asset to test asset-usage from Kind::Data.
        let wav: Vec<u8> = [
            b"RIFF".as_slice(),
            &36u32.to_le_bytes(),
            b"WAVEfmt ",
            &16u32.to_le_bytes(),
            &1u16.to_le_bytes(),
            &1u16.to_le_bytes(),
            &44100u32.to_le_bytes(),
            &88200u32.to_le_bytes(),
            &2u16.to_le_bytes(),
            &16u16.to_le_bytes(),
            b"data",
            &0u32.to_le_bytes(),
        ]
        .concat();
        let sound_import = st.handle(
            a,
            &msg(&format!(
                r#"{{"type":"asset-import","path":"content/audio/chime.wav","dataBase64":"{}"}}"#,
                ganbate_core::digest::base64(&wav)
            )),
        );
        let sound_id = of_type(&sound_import, "asset-imported")[0]
            .1
            .get("id")
            .unwrap()
            .string()
            .unwrap()
            .to_string();

        // Open data doc from both watchers.
        for who in [a, b] {
            let opened = st.handle(
                who,
                &msg(&format!(r#"{{"type":"open","doc":"{data_id}"}}"#)),
            );
            let doc_msg = &of_type(&opened, "doc")[0].1;
            assert_eq!(doc_msg.get("kind"), Some(&s("data")));
            assert_eq!(doc_msg.get("issues"), Some(&Value::Array(vec![])));
        }

        // Valid edit updates values, broadcasts ops + issues, and tracks Undo/Redo.
        let valid_edit = st.handle(
            a,
            &msg(&format!(
                r#"{{"type":"edit","doc":"{data_id}","rev":0,"label":"Tune HUD","ops":[
                    {{"set":["values","opacity"],"value":0.8}},
                    {{"set":["values","banner"],"value":[0.25,0.15]}},
                    {{"set":["values","cue_sound"],"value":"{sound_id}"}},
                    {{"set":["values","cues"],"value":[{{"delay":0.5,"clip":"{sound_id}"}}]}}
                ]}}"#
            )),
        );
        let ops_msgs = of_type(&valid_edit, "ops");
        assert_eq!(ops_msgs.len(), 2);
        assert_eq!(ops_msgs[0].1.get("issues"), Some(&Value::Array(vec![])));

        assert_eq!(st.open[&data_id].doc.revision(), 4);

        // Out-of-range, wrong-type, unknown-field, header edits, delete, and cook are refused with rollback.
        for bad in [
            format!(
                r#"{{"type":"edit","doc":"{data_id}","rev":4,"ops":[{{"set":["values","opacity"],"value":2.5}}]}}"#
            ),
            format!(
                r#"{{"type":"edit","doc":"{data_id}","rev":4,"ops":[{{"set":["values","banner"],"value":"bad"}}]}}"#
            ),
            format!(
                r#"{{"type":"edit","doc":"{data_id}","rev":4,"ops":[{{"set":["values","unknown_key"],"value":1}}]}}"#
            ),
            format!(
                r#"{{"type":"edit","doc":"{data_id}","rev":4,"ops":[{{"set":["schema"],"value":"other.kind"}}]}}"#
            ),
            format!(
                r#"{{"type":"edit","doc":"{data_id}","rev":4,"ops":[{{"set":["id"],"value":"0123456789abcdef0123456789abcdef"}}]}}"#
            ),
            format!(r#"{{"type":"delete","doc":"{data_id}","ids":["0000000000000001"]}}"#),
            format!(r#"{{"type":"cook","doc":"{data_id}"}}"#),
        ] {
            let reply = st.handle(a, &msg(&bad));
            assert!(
                !of_type(&reply, "error").is_empty(),
                "expected error for {bad}"
            );
            assert!(
                of_type(&reply, "doc").is_empty(),
                "must not be stale-rev rejection: {bad}"
            );
        }
        assert_eq!(st.open[&data_id].doc.revision(), 4);

        // Asset usage counts both top-level and nested array asset references in Kind::Data.
        let usage = st.handle(
            a,
            &msg(&format!(
                r#"{{"type":"asset-usage","id":"{sound_id}","req":"u-sound"}}"#
            )),
        );
        let uses = of_type(&usage, "asset-usage")[0]
            .1
            .get("uses")
            .unwrap()
            .array()
            .unwrap();
        assert_eq!(uses.len(), 2);
        assert_eq!(uses[0].get("doc"), Some(&s(&data_id)));
        assert_eq!(uses[0].get("docKind"), Some(&s("data")));
        assert_eq!(uses[0].get("entityName"), Some(&s("Observatory HUD")));

        // Undo & Redo work cleanly.
        st.handle(a, &msg(&format!(r#"{{"type":"undo","doc":"{data_id}"}}"#)));
        assert_eq!(
            st.open[&data_id].doc.root().get("values"),
            Some(&Value::Object(BTreeMap::new()))
        );
        st.handle(a, &msg(&format!(r#"{{"type":"redo","doc":"{data_id}"}}"#)));
        assert_eq!(
            st.open[&data_id]
                .doc
                .root()
                .get("values")
                .unwrap()
                .get("opacity"),
            Some(&Value::Number(0.8))
        );

        // CHANGED_ON_DISK protection works on save, and force overwrites cleanly.
        let mut external = st.project.read_document(&data_id).unwrap();
        if let Value::Object(map) = &mut external {
            map.insert("name".into(), s("External Edit"));
        }
        ganbate_pipeline::project::write_json(&t.0.join("content/data/hud.data.json"), &external)
            .unwrap();
        let refused = st.handle(a, &msg(&format!(r#"{{"type":"save","doc":"{data_id}"}}"#)));
        assert_eq!(
            of_type(&refused, "error")[0].1.get("code"),
            Some(&s(CHANGED_ON_DISK))
        );
        let saved = st.handle(
            a,
            &msg(&format!(
                r#"{{"type":"save","force":true,"doc":"{data_id}"}}"#
            )),
        );
        assert_eq!(of_type(&saved, "saved").len(), 2);

        // asset-import and asset-move work for .data.json.
        let imported_json =
            json::to_canonical(&st.project.read_document(&data_id).unwrap()).unwrap();
        let imported = st.handle(
            a,
            &msg(&format!(
                r#"{{"type":"asset-import","path":"content/data/hud_copy.data.json","dataBase64":"{}"}}"#,
                ganbate_core::digest::base64(imported_json.as_bytes())
            )),
        );
        let copy_id = of_type(&imported, "asset-imported")[0]
            .1
            .get("id")
            .unwrap()
            .string()
            .unwrap()
            .to_string();
        assert_ne!(copy_id, data_id);
        let moved = st.handle(
            a,
            &msg(&format!(
                r#"{{"type":"asset-move","id":"{data_id}","to":"content/data/observatory_hud.data.json"}}"#
            )),
        );
        assert_eq!(of_type(&moved, "asset-moved").len(), 1);
        assert!(t.0.join("content/data/observatory_hud.data.json").is_file());
    }

    #[test]
    fn custom_workbench_documents_support_create_open_edit_undo_redo_save_and_rescan() {
        let (mut st, t) = studio("custom-workbenches");
        let a = st.connect("workbench-author");

        // 1. Material (`Kind::Material`)
        let created_mat = st.handle(
            a,
            &msg(
                r#"{"type":"create","kind":"material","path":"content/materials/velvet.material.json","name":"Velvet","initial":{"roughness":0.82,"nodes":[]}}"#,
            ),
        );
        let mat_id = of_type(&created_mat, "created")[0]
            .1
            .get("doc")
            .unwrap()
            .string()
            .unwrap()
            .to_string();
        assert_eq!(
            of_type(&created_mat, "created")[0].1.get("kind"),
            Some(&s("material"))
        );
        let opened_mat = st.handle(a, &msg(&format!(r#"{{"type":"open","doc":"{mat_id}"}}"#)));
        assert_eq!(opened_mat[0].1.get("kind"), Some(&s("material")));
        let edited_mat = st.handle(
            a,
            &msg(&format!(
                r#"{{"type":"edit","doc":"{mat_id}","rev":0,"label":"Tune Roughness","ops":[{{"set":["roughness"],"value":0.35}}]}}"#
            )),
        );
        assert_eq!(of_type(&edited_mat, "ops").len(), 1);
        let saved_mat = st.handle(a, &msg(&format!(r#"{{"type":"save","doc":"{mat_id}"}}"#)));
        assert_eq!(of_type(&saved_mat, "saved").len(), 1);
        assert!(t.0.join("content/materials/velvet.material.json").is_file());

        // 2. Custom JSON workbenches (`soundbank`, `character`, `seq`, `vfx`)
        for (kind, rel) in [
            ("soundbank", "content/audio/ambient.soundbank.json"),
            ("character", "content/chars/hero.character.json"),
            ("seq", "content/cinematics/intro.seq.json"),
            ("vfx", "content/fx/sparks.vfx.json"),
        ] {
            let created = st.handle(
                a,
                &msg(&format!(
                    r#"{{"type":"create","kind":"{kind}","path":"{rel}","name":"Sample {kind}","initial":{{"tracks":[]}}}}"#
                )),
            );
            let cid = of_type(&created, "created")[0]
                .1
                .get("doc")
                .unwrap()
                .string()
                .unwrap()
                .to_string();
            assert_eq!(cid, custom_doc_id(rel));
            let opened = st.handle(a, &msg(&format!(r#"{{"type":"open","path":"{rel}"}}"#)));
            assert_eq!(opened[0].1.get("kind"), Some(&s(kind)));
            assert_eq!(opened[0].1.get("doc"), Some(&s(&cid)));

            let edited = st.handle(
                a,
                &msg(&format!(
                    r#"{{"type":"edit","doc":"{cid}","rev":0,"label":"Add Track","ops":[{{"set":["tracks"],"value":["main"]}}]}}"#
                )),
            );
            assert_eq!(of_type(&edited, "ops").len(), 1);

            // Rescan does not mark custom_open unavailable!
            let _ = st.handle(a, &msg(r#"{"type":"rescan"}"#));
            let undone = st.handle(a, &msg(&format!(r#"{{"type":"undo","doc":"{cid}"}}"#)));
            assert_eq!(of_type(&undone, "ops").len(), 1);
            let redone = st.handle(a, &msg(&format!(r#"{{"type":"redo","doc":"{cid}"}}"#)));
            assert_eq!(of_type(&redone, "ops").len(), 1);
            let saved = st.handle(a, &msg(&format!(r#"{{"type":"save","doc":"{cid}"}}"#)));
            assert_eq!(of_type(&saved, "saved").len(), 1);
            assert!(t.0.join(rel).is_file());
        }
    }

    #[test]
    fn ai_ide_gbs_verify_crystallize_and_mcp_roundtrip_under_100ms() {
        let (mut st, t) = studio("ai-ide");
        let a = st.connect("studio-viewport");

        // Create and open a scene.
        let created = st.handle(
            a,
            &msg(
                r#"{"type":"create","kind":"scene","path":"content/harbor.scene.json","name":"Harbor"}"#,
            ),
        );
        let scene_id = of_type(&created, "created")[0]
            .1
            .get("doc")
            .unwrap()
            .string()
            .unwrap()
            .to_string();
        st.handle(a, &msg(&format!(r#"{{"type":"open","doc":"{scene_id}"}}"#)));

        // Sync live @selection from Studio client.
        let synced = st.handle(
            a,
            &msg(&format!(
                r#"{{"type":"selection-sync","doc":"{scene_id}","entities":[],"cursorWorld":[0,0,0],"workbench":"world","req":"sync1"}}"#
            )),
        );
        assert_eq!(of_type(&synced, "selection-synced").len(), 1);

        // Apply a .gbs relational spatial script (spawn table + snap lantern onto table + camera frame).
        let script = [
            r#"tx "Build Shrine Table""#,
            r#"spawn pedestal empty name="Pedestal""#,
            r#"attach @this collider shape="box" half_extents=[1.0, 0.5, 1.0]"#,
            r#"place @this snap onto ground at [0, 0]"#,
            r#"spawn lantern empty name="ShrineLantern""#,
            r#"attach @this collider shape="box" half_extents=[0.2, 0.3, 0.2]"#,
            r#"attach @this light intensity=4.0 range=8.0"#,
            r#"place @this socket top_center on $pedestal"#,
            r#"spawn hero_cam empty name="HeroCam""#,
            r#"attach @this camera fov=50"#,
            r#"cam frame $pedestal dist=4.5 elev=25 yaw=35 camera=$hero_cam"#,
        ]
        .join("\n");

        let gbs_req = obj(vec![
            ("type", s("gbs-apply")),
            ("doc", s(&scene_id)),
            ("script", s(&script)),
            ("req", s("g1")),
        ]);
        let gbs_out = st.handle(a, &gbs_req);
        assert_eq!(of_type(&gbs_out, "ops").len(), 1);
        assert_eq!(of_type(&gbs_out, "gbs-applied").len(), 1);

        // Run <100ms verify oracle with contactSheet PNG.
        let ver_out = st.handle(
            a,
            &msg(&format!(
                r#"{{"type":"scene-verify","doc":"{scene_id}","contactSheet":true,"req":"v1"}}"#
            )),
        );
        let ver_msg = &of_type(&ver_out, "scene-verified")[0].1;
        let report = ver_msg.get("report").unwrap();
        assert_eq!(report.get("passed"), Some(&Value::Bool(true)));
        let elapsed = report.get("elapsedMs").unwrap().float().unwrap();
        assert!(elapsed < 100.0, "verify took {elapsed}ms");
        assert!(
            report
                .get("contactSheetBase64")
                .and_then(|v| v.string().ok())
                .is_some_and(|b| b.len() > 64)
        );
        // Neither pedestal nor lantern should trigger floating-prop because pedestal snapped onto ground and lantern snapped to pedestal's top_center!
        let lints = report.get("spatialLints").unwrap().array().unwrap();
        assert!(
            !lints
                .iter()
                .any(|l| l.get("code") == Some(&s("floating-prop"))),
            "unexpected spatial lints: {lints:?}"
        );

        // Crystallize the transaction into a parameterized Brick (`shrine_set`) and spawn 3 in a ring via MCP!
        let cryst_out = st.handle(
            a,
            &msg(&format!(
                r#"{{"type":"brick-crystallize","doc":"{scene_id}","id":"shrine_set","fromTx":true,"params":["glow=light.intensity"],"req":"c1"}}"#
            )),
        );
        assert_eq!(of_type(&cryst_out, "brick-crystallized").len(), 1);
        assert!(t.0.join(".ganbate/bricks/shrine_set.brick.json").is_file());
        assert!(t.0.join(".ganbate/catalog.idx").is_file());

        // Test MCP JSON-RPC 2.0 `tools/call` (`scene_gbs` + `doc_undo`) broadcasting to Studio watcher `a`!
        let mcp_call = br#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"scene_gbs","arguments":{"script":"ring shrines brick:shrine_set count=3 radius=5.0 glow=6.5"}}}"#;
        let (rpc_resp, mcp_outbox) = crate::ai::mcp::handle_rpc_bytes(&mut st, mcp_call);
        let resp_val = rpc_resp.expect("json-rpc response");
        assert_eq!(
            resp_val.get("result").and_then(|r| r.get("isError")),
            Some(&Value::Bool(false))
        );
        assert_eq!(of_type(&mcp_outbox, "ops").len(), 1);
        assert_eq!(mcp_outbox[0].0, a);

        // Undo via MCP `doc_undo` restores exact previous revision!
        let rev_after_ring = st.open[&scene_id].doc.revision();
        let (_, undo_outbox) = crate::ai::mcp::handle_rpc_bytes(
            &mut st,
            br#"{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"doc_undo","arguments":{}}}"#,
        );
        assert_eq!(of_type(&undo_outbox, "ops").len(), 1);
        assert!(st.open[&scene_id].doc.revision() > rev_after_ring);
    }
}
