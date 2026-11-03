//! The studio's state and the edit protocol. Every message is a JSON object with a `type`;
//! `Studio::handle` takes one from a client and returns the messages to send (to that client
//! and to others watching the same document), so the protocol is tested without sockets.
//!
//! | Client sends | Host answers |
//! | --- | --- |
//! | `hello {client}` | `welcome {project, schemas, assets}` |
//! | `open {doc}` | `doc {doc, rev, path, kind, root, dirty, undo, redo, issues}` (a document open with no unsaved edits is read again if its file changed on disk, and everyone watching it gets the new `doc`) |
//! | `close {doc}` | nothing |
//! | `edit {doc, rev, label, merge?, ops, req?}` | everyone watching: `ops {doc, rev, ops, label, by, req}`; on failure the sender gets `error {req, message}` (and `doc` when its revision was stale) |
//! | `delete {doc, rev, ids, label?, req?}` | as `edit`, with the ops that remove `ids` and everything beneath them, including what was added under the entities a deleted prefab instance shows |
//! | `undo {doc}` / `redo {doc}` | everyone watching: `ops ...` |
//! | `save {doc, force?}` | everyone watching: `saved {doc, rev}`; if the file changed on disk since it was opened or saved, the sender gets `error {code: "changed-on-disk"}` instead, unless `force` |
//! | `ids {count}` | `ids {ids}`: fresh entity ids |
//! | `create {kind, path, name}` | `created {doc, path}` and everyone: `assets` |
//! | `import {model, path, name}` | `created {doc, path}` and everyone: `assets` |
//! | `cook {doc}` | `cooked {doc, rev, file, entities, warnings}` (the open, unsaved document) |
//! | `preview {doc, ...}` / `select {doc, ...}` | relayed to the others watching `doc` |
//!
//! An op is `{"set": [path...], "value": v}` or `{"remove": [path...]}`. In scenes and prefabs
//! only `entities` and `name` may change; an edit that adds a validation problem (including
//! the problems only expanding its prefabs shows, such as a parent cycle through what an
//! instance shows) is refused.
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use abe_core::id::{Ids, is_asset_id};
use abe_core::json::{self, Value};
use abe_pipeline::cook;
use abe_pipeline::project::{Kind, Project};
use abe_scene::doc::{Document, History, Op};
use abe_scene::scene;
use abe_scene::schema::{Registry, engine_registry};

pub type ClientId = u64;
/// Messages to send: to whom, and what.
pub type Outbox = Vec<(ClientId, Value)>;

const HISTORY_LIMIT: usize = 512;
const MAX_IDS: usize = 4096;
/// Deepest folder nesting below a content folder for a new document: the project scan
/// (`abe_pipeline::project`) refuses a project with deeper folders, so one more would leave a
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
    /// The document as last read from or written to its file, to notice another tool (`abe`,
    /// git, a second host) changing the file meanwhile.
    on_disk: Value,
}

pub struct Studio {
    pub project: Project,
    pub registry: Registry,
    ids: Ids,
    open: BTreeMap<String, Open>,
    clients: BTreeMap<ClientId, String>,
    next_client: ClientId,
    /// Where cooked previews are written (inside the project's `.abe` folder).
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

/// Every problem of `root`, including what only expanding its prefabs shows (a parent cycle
/// through what an instance shows, a missing or broken prefab).
fn issue_set(
    root: &Value,
    registry: &Registry,
    load: &dyn Fn(&str) -> Option<Value>,
) -> BTreeSet<(String, String)> {
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
            clients: BTreeMap::new(),
            next_client: 1,
            cache_rel: ".abe/cooked".into(),
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
        for o in self.open.values_mut() {
            o.watchers.remove(&client);
        }
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.project.root.join(&self.cache_rel)
    }

    fn assets(&self) -> Value {
        Value::Array(
            self.project
                .entries()
                .map(|e| {
                    obj(vec![
                        ("id", s(&e.id)),
                        ("kind", s(e.kind.name())),
                        ("path", s(&e.path)),
                    ])
                })
                .collect(),
        )
    }

    fn doc_message(&self, id: &str) -> Value {
        let o = &self.open[id];
        let label = |l: Option<&str>| l.map_or(Value::Null, s);
        let open = self.open_prefabs();
        let load = |a: &str| load_prefab(&open, &self.project, &self.real_root, a);
        let issues = scene::check_with_prefabs(o.doc.root(), &self.registry, &load);
        obj(vec![
            ("type", s("doc")),
            ("doc", s(id)),
            ("rev", n(o.doc.revision())),
            ("path", s(&o.path)),
            ("kind", s(o.kind.name())),
            ("root", o.doc.root().clone()),
            ("dirty", Value::Bool(o.history.dirty())),
            ("undo", label(o.history.can_undo())),
            ("redo", label(o.history.can_redo())),
            (
                "issues",
                Value::Array(
                    issues
                        .iter()
                        .map(|i| obj(vec![("path", s(&i.path)), ("message", s(&i.message))]))
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
            .filter(|(_, o)| o.kind == Kind::Prefab)
            .map(|(k, o)| (k.clone(), o.doc.root().clone()))
            .collect()
    }

    /// Opens `id` if it is not open yet. A document already open with no unsaved edits is read
    /// again when its file changed on disk (another tool edited it); returns whether it was.
    fn ensure_open(&mut self, id: &str) -> Result<bool, String> {
        if let Some(o) = self.open.get(id) {
            if o.history.dirty() {
                return Ok(false);
            }
            self.inside(&o.path)?;
            let disk = self.project.read_document(id)?;
            if disk == o.on_disk {
                return Ok(false);
            }
            let open = self.open_prefabs();
            let load = |a: &str| load_prefab(&open, &self.project, &self.real_root, a);
            let known_issues = issue_set(&disk, &self.registry, &load);
            let o = self.open.get_mut(id).expect("open");
            o.doc = Document::new(disk.clone())?;
            o.history = History::new(HISTORY_LIMIT);
            o.known_issues = known_issues;
            o.on_disk = disk;
            return Ok(true);
        }
        let entry = self
            .project
            .entry(id)
            .ok_or_else(|| format!("no asset {id}"))?
            .clone();
        if !matches!(entry.kind, Kind::Scene | Kind::Prefab) {
            return Err(format!("{} is not an editable document", entry.path));
        }
        self.inside(&entry.path)?;
        let root = self.project.read_document(id)?;
        let open = self.open_prefabs();
        let load = |a: &str| load_prefab(&open, &self.project, &self.real_root, a);
        let known_issues = issue_set(&root, &self.registry, &load);
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

    fn doc_id(msg: &Value) -> Result<&str, String> {
        msg.get("doc")
            .and_then(|d| d.string().ok())
            .filter(|d| is_asset_id(d))
            .ok_or_else(|| "missing or invalid doc id".into())
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
                        ("schemas", Value::Array(self.registry.sources().to_vec())),
                        ("assets", self.assets()),
                    ]),
                )])
            }
            "open" => {
                let id = Self::doc_id(msg)?.to_string();
                let reread = self.ensure_open(&id)?;
                self.open.get_mut(&id).expect("open").watchers.insert(from);
                let doc = self.doc_message(&id);
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
                let id = Self::doc_id(msg)?;
                if let Some(o) = self.open.get_mut(id) {
                    o.watchers.remove(&from);
                }
                Ok(vec![])
            }
            "edit" => self.edit(from, msg),
            "delete" => {
                // Entities are deleted through the expanded hierarchy (scene::delete), so what
                // was added under the entities a prefab instance shows goes with the instance.
                let id = Self::doc_id(msg)?.to_string();
                let o = self.open.get(&id).ok_or("open the document first")?;
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
                let id = Self::doc_id(msg)?.to_string();
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
                        let known = issue_set(&root, &self.registry, &load);
                        self.open.get_mut(&id).expect("open").known_issues = known;
                        Ok(self.broadcast_ops(&id, &ops, &label, from, msg.get("req")))
                    }
                    None => Ok(vec![]),
                }
            }
            "save" => {
                let id = Self::doc_id(msg)?.to_string();
                let o = self.open.get(&id).ok_or("open the document first")?;
                let (path, kind, root) = (o.path.clone(), o.kind, o.doc.root().clone());
                self.inside(&path)?;
                let force = msg.get("force") == Some(&Value::Bool(true));
                // A file another tool changed since this host read or wrote it is not
                // overwritten without being asked to (`force`).
                let changed = match self.project.read_document(&id) {
                    Ok(disk) => disk != o.on_disk,
                    Err(_) => true,
                };
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
                self.project.write_document(&path, kind, &root)?;
                let o = self.open.get_mut(&id).expect("open");
                o.on_disk = root;
                o.history.mark_saved();
                let msg = obj(vec![
                    ("type", s("saved")),
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
                let kind = match msg.get("kind").and_then(|k| k.string().ok()) {
                    Some("scene") => scene::Kind::Scene,
                    Some("prefab") => scene::Kind::Prefab,
                    _ => return Err("create a scene or a prefab".into()),
                };
                let path = self.new_path(msg, kind.name())?;
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
                Ok(self.created(from, &id, &path))
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
                let bytes = abe_assets::read_asset(&self.project.root.join(&entry.path))?;
                let nodes = abe_assets::glb::import_nodes(&bytes, true)?;
                let name = msg
                    .get("name")
                    .and_then(|v| v.string().ok())
                    .unwrap_or(&entry.path)
                    .to_string();
                let id = self.ids.asset();
                let root = cook::scene_from_model(&nodes, model, &id, &name, &mut self.ids);
                self.project.write_document(&path, Kind::Scene, &root)?;
                Ok(self.created(from, &id, &path))
            }
            "cook" => self.cook(from, msg),
            "preview" | "select" => {
                let id = Self::doc_id(msg)?;
                let o = self.open.get(id).ok_or("open the document first")?;
                let mut relayed = msg.clone();
                if let Value::Object(m) = &mut relayed {
                    m.insert("by".into(), n(from));
                }
                Ok(o.watchers
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

    fn created(&self, from: ClientId, id: &str, path: &str) -> Outbox {
        let mut out = vec![(
            from,
            obj(vec![
                ("type", s("created")),
                ("doc", s(id)),
                ("path", s(path)),
            ]),
        )];
        let assets = obj(vec![("type", s("assets")), ("assets", self.assets())]);
        out.extend(self.clients.keys().map(|c| (*c, assets.clone())));
        out
    }

    fn edit(&mut self, from: ClientId, msg: &Value) -> Result<Outbox, String> {
        let id = Self::doc_id(msg)?.to_string();
        let req = msg.get("req");
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
            if !matches!(op.path()[0].as_str(), "entities" | "name") {
                return Err(format!(
                    "{:?} cannot be edited (only entities and name)",
                    op.path()[0]
                ));
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
        let after = issue_set(o.doc.root(), registry, &load);
        let fresh: Vec<&(String, String)> = after.difference(&o.known_issues).collect();
        if let Some((path, message)) = fresh.first() {
            let e = format!("{path}: {message}");
            // Taken back before it enters the history, so undo and redo are untouched.
            o.doc.rollback(&undo, before)?;
            return Err(e);
        }
        let applied = ops.clone();
        o.history.record(label, merge, ops, undo);
        o.known_issues = after;
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
        let id = Self::doc_id(msg)?.to_string();
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
        let bytes = abe_assets::cooked::encode_scene(&cooked.world, &cooked.table, false)?;
        let name = cook::content_name(&id, "abew", &bytes);
        let dir = self.cache_dir();
        self.inside(&self.cache_rel)?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let file = dir.join(&name);
        if !file.exists() {
            abe_assets::write_asset(&file, &bytes).map_err(|e| e.to_string())?;
        }
        self.keep_preview(&id, &name);
        // Each entity's own geometry's world bounds, for selection boxes.
        let bounds = |e: &abe_core::world::EntityRecord| -> Value {
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

/// Whether a file name is a cooked preview's: `<document id>.<16 hex digits>.abew`.
fn is_preview_name(name: &str) -> bool {
    let mut parts = name.split('.');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(id), Some(hash), Some("abew"), None) => {
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
            std::env::temp_dir().join(format!("abe-studio-{tag}-{}", Ids::from_os().entity()));
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
        // Another tool (the abe command, git) rewrites the file: a new name.
        let path = t.0.join("content/main.scene.json");
        let mut disk = st.project.read_document(&doc).unwrap();
        if let Value::Object(m) = &mut disk {
            m.insert("name".into(), Value::String("Renamed outside".into()));
        }
        abe_pipeline::project::write_json(&path, &disk).unwrap();
        // Reopening a document with no unsaved edits reads it again, for every watcher.
        let out = st.handle(a, &msg(&format!(r#"{{"type":"open","doc":"{doc}"}}"#)));
        let docs = of_type(&out, "doc");
        assert_eq!(docs.len(), 2, "both watchers get the new text");
        assert_eq!(
            docs[0].1.get("root").unwrap().get("name"),
            Some(&Value::String("Renamed outside".into()))
        );
        // An unsaved edit, then the file changes again: the save is refused, not lost.
        let ok = format!(
            r#"{{"type":"edit","doc":"{doc}","rev":0,"ops":[{{"set":["name"],"value":"Mine"}}]}}"#
        );
        assert_eq!(of_type(&st.handle(a, &msg(&ok)), "ops").len(), 2);
        if let Value::Object(m) = &mut disk {
            m.insert("name".into(), Value::String("Theirs".into()));
        }
        abe_pipeline::project::write_json(&path, &disk).unwrap();
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
        abe_pipeline::project::write_json(&t.0.join("content/lamp.prefab.json"), &bad).unwrap();
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
        std::fs::create_dir_all(t.0.join(".abe")).unwrap();
        std::os::unix::fs::symlink(&outside.0, t.0.join(".abe/cooked")).unwrap();
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
        let cooked = t.0.join(".abe/cooked");
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
}
