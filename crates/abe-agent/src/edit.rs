//! `abe scene apply`: edits translated into the ops ABE Studio sends (`abe_scene::scene`'s
//! `create`, `delete`, `reparent`, `duplicate` and `set_field`, and plain `Set`/`Remove` on
//! document paths), applied in memory as one transaction and judged by the studio's rule: an
//! edit may not add a problem the document did not already have.
//!
//! ```json
//! {"label": "Brighter lamps", "edits": [
//!   {"op": "set", "entity": "Street/Lamp", "path": "components.light.intensity", "value": 2},
//!   {"op": "remove", "entity": "Lamp", "path": "components.light"},
//!   {"op": "create", "name": "Bench", "parent": "Street", "components": {"transform": {}}},
//!   {"op": "delete", "entity": "0f1e2d3c4b5a6978"},
//!   {"op": "reparent", "entity": "Bench", "parent": null},
//!   {"op": "duplicate", "entity": "Bench", "name": "Bench 2"}]}
//! ```
//!
//! A path is a dotted string below the entity (`name`, `order`, `components...`, `prefab...`)
//! or an array of keys. A game's component kinds carry a dot (`arena.spawn`), so after
//! `components` the longest known kind wins: `components.arena.spawn.team` is field `team` of
//! `arena.spawn`. Arrays are values: set a whole array, not an element. A create may give the
//! new entity's `id`; otherwise new ids come from a generator seeded with the document and the
//! edits, so the same edits on the same document make the same ids (and a dry run reports the
//! ids a real run writes).
use std::collections::{BTreeMap, BTreeSet};

use abe_core::digest::Sha256;
use abe_core::id::{Ids, is_entity_id};
use abe_core::json::{self, Value};
use abe_scene::doc::{self, Document, Op};
use abe_scene::scene;
use abe_scene::schema::Registry;

use crate::error::{self, Error};
use crate::json::{J, Obj};
use crate::refs::{self, Tree};
use crate::validate;

/// What an edit's path may start with (the parent changes through `reparent`).
const ENTITY_KEYS: [&str; 4] = ["name", "order", "components", "prefab"];
/// Keys below the entity (an op's path holds at most 64 with `entities` and the id).
const MAX_KEYS: usize = 62;

/// One value that changed: in which edit, on which entity, where, from what, to what
/// (`None`: absent). The path is `""` for the entity itself (created or deleted).
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub edit: Option<usize>,
    pub entity: String,
    pub name: String,
    pub path: Vec<String>,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

impl Change {
    pub fn json(&self) -> J {
        let mut o = Obj::new();
        if let Some(i) = self.edit {
            o.put("edit", i);
        }
        o.put("entity", self.entity.as_str());
        o.put("name", self.name.as_str());
        o.put("path", self.path.join("."));
        o.put("before", self.before.as_ref().map(J::from));
        o.put("after", self.after.as_ref().map(J::from));
        o.into()
    }
}

/// A transaction applied in memory.
pub struct Applied {
    pub root: Value,
    /// The ops in the order they were applied, and the ops that take them all back.
    pub ops: Vec<Op>,
    pub undo: Vec<Op>,
    pub changes: Vec<Change>,
    /// (edit, id) of each entity a create or duplicate made (a duplicate's top copy).
    pub created: Vec<(usize, String)>,
}

/// Reads EDITS: `{"edits": [...], "label"?: "..."}` or a bare array of edits.
pub fn parse(bytes: &[u8]) -> Result<(Option<String>, Vec<Value>), Error> {
    let shape = "EDITS is {\"edits\": [{\"op\": ...}, ...]}";
    match json::parse(bytes).map_err(|e| error::bad_edit(format!("EDITS is not JSON: {e}")))? {
        Value::Array(list) => Ok((None, list)),
        Value::Object(m) => {
            if let Some(k) = m.keys().find(|k| !matches!(k.as_str(), "edits" | "label")) {
                return Err(error::bad_edit(format!(
                    "{shape} with an optional \"label\"; not {k:?}"
                )));
            }
            let label = match m.get("label") {
                None => None,
                Some(Value::String(s)) => Some(s.clone()),
                Some(_) => return Err(error::bad_edit("\"label\" is a string")),
            };
            match m.get("edits") {
                Some(Value::Array(list)) => Ok((label, list.clone())),
                _ => Err(error::bad_edit(shape)),
            }
        }
        _ => Err(error::bad_edit(shape)),
    }
}

/// The id generator's seed: the document and the edits, so the same inputs make the same ids.
pub fn seed(root: &Value, edits: &[Value]) -> u64 {
    let mut h = Sha256::new();
    for v in [root, &Value::Array(edits.to_vec())] {
        // Parsed values are finite, so the canonical text exists.
        h.update(json::to_canonical(v).unwrap_or_default().as_bytes());
        h.update(b"\n");
    }
    let mut first = [0u8; 8];
    first.copy_from_slice(&h.finish()[..8]);
    u64::from_be_bytes(first)
}

/// A history label for edits that came without one: their ops.
pub fn label(edits: &[Value]) -> String {
    let ops: Vec<&str> = edits
        .iter()
        .map(|e| e.get("op").and_then(|o| o.string().ok()).unwrap_or("?"))
        .collect();
    format!("abe: {}", ops.join(", "))
}

fn name_at(root: &Value, path: &[String]) -> Option<String> {
    match path {
        [first, id, ..] if first == "entities" => scene::entities(root)
            .get(id)?
            .get("name")?
            .string()
            .ok()
            .map(String::from),
        _ => None,
    }
}

/// Applies `ops` one by one, noting each value that changed; returns their inverses in the
/// order the ops were applied. On failure `doc` is left part-way: callers drop it.
pub fn apply_ops(
    doc: &mut Document,
    ops: &[Op],
    edit: Option<usize>,
    changes: &mut Vec<Change>,
) -> Result<Vec<Op>, String> {
    let mut inverses = Vec::with_capacity(ops.len());
    for op in ops {
        let path = op.path();
        let before = doc.get(path).cloned();
        let old_name = name_at(doc.root(), path);
        inverses.push(doc.apply(op)?);
        let after = doc.get(path).cloned();
        if before == after {
            continue;
        }
        let (entity, rest) = match path.as_slice() {
            [first, id, rest @ ..] if first == "entities" => (id.clone(), rest.to_vec()),
            _ => (String::new(), path.clone()),
        };
        changes.push(Change {
            edit,
            entity,
            name: name_at(doc.root(), path).or(old_name).unwrap_or_default(),
            path: rest,
            before,
            after,
        });
    }
    Ok(inverses)
}

fn issue_set(root: &Value, registry: &Registry) -> BTreeSet<(String, String)> {
    scene::check(root, registry)
        .into_iter()
        .map(|i| (i.path, i.message))
        .collect()
}

/// Fresh entity ids: from the seeded generator, never one the document or this transaction
/// already uses.
struct IdSource {
    ids: Ids,
    taken: BTreeSet<String>,
}

impl IdSource {
    fn next(&mut self, root: &Value) -> String {
        loop {
            let id = self.ids.entity();
            if !scene::entities(root).contains_key(&id) && self.taken.insert(id.clone()) {
                return id;
            }
        }
    }
}

/// Applies `edits` to `root` as one transaction (see the module notes).
pub fn run(
    root: &Value,
    edits: &[Value],
    registry: &Registry,
    load: &dyn Fn(&str) -> Option<Value>,
    seed: u64,
) -> Result<Applied, Error> {
    let mut doc = Document::new(root.clone()).map_err(error::invalid)?;
    let mut ids = IdSource {
        ids: Ids::seeded(seed),
        taken: BTreeSet::new(),
    };
    let mut batches = Vec::with_capacity(edits.len());
    let mut undo = vec![];
    let mut changes = vec![];
    let mut created = vec![];
    for (i, e) in edits.iter().enumerate() {
        let (ops, made) = translate(doc.root(), e, &mut ids, registry, load)
            .map_err(|err| err.with("edit", i))?;
        let inverses = apply_ops(&mut doc, &ops, Some(i), &mut changes).map_err(|m| {
            error::bad_edit(format!("edit {i}: {m}; set the whole value instead")).with("edit", i)
        })?;
        undo.extend(inverses);
        if let Some(id) = made {
            created.push((i, id));
        }
        batches.push(ops);
    }
    undo.reverse();
    let before = issue_set(root, registry);
    let after = issue_set(doc.root(), registry);
    let fresh: Vec<&(String, String)> = after.difference(&before).collect();
    if !fresh.is_empty() {
        return Err(blame(root, doc.root(), &batches, &fresh, registry));
    }
    Ok(Applied {
        root: doc.root().clone(),
        ops: batches.concat(),
        undo,
        changes,
        created,
    })
}

/// The refusal for a transaction that leaves new problems: each problem with the edit after
/// which it appeared and stayed.
fn blame(
    root: &Value,
    end: &Value,
    batches: &[Vec<Op>],
    fresh: &[&(String, String)],
    registry: &Registry,
) -> Error {
    let mut sets = Vec::with_capacity(batches.len());
    if let Ok(mut doc) = Document::new(root.clone()) {
        for ops in batches {
            // These ops applied a moment ago, to the same document.
            let _ = doc.apply_all(ops);
            sets.push(issue_set(doc.root(), registry));
        }
    }
    let tree = Tree::new(end);
    let last = batches.len().saturating_sub(1);
    let mut first = last;
    let mut issues = vec![];
    for problem in fresh {
        let mut at = last;
        while at > 0 && sets.get(at - 1).is_some_and(|s| s.contains(*problem)) {
            at -= 1;
        }
        first = first.min(at);
        issues.push(J::from(
            validate::issue(&tree, None, &problem.0, &problem.1).with("edit", at),
        ));
    }
    let (path, message) = fresh[0];
    error::invalid(format!(
        "the edits would make the scene invalid ({} new problem{}; {path}: {message})",
        fresh.len(),
        if fresh.len() == 1 { "" } else { "s" }
    ))
    .with("edit", first)
    .with("issues", issues)
}

/// The ops for one edit, against the document as the edits before it left it; and the id of
/// the entity it creates, if any.
fn translate(
    root: &Value,
    e: &Value,
    ids: &mut IdSource,
    registry: &Registry,
    load: &dyn Fn(&str) -> Option<Value>,
) -> Result<(Vec<Op>, Option<String>), Error> {
    let Value::Object(m) = e else {
        return Err(error::bad_edit("an edit is an object with an \"op\""));
    };
    let op = match m.get("op") {
        Some(Value::String(s)) => s.as_str(),
        _ => {
            return Err(error::bad_edit(
                "an edit needs \"op\": set, remove, create, delete, reparent or duplicate",
            ));
        }
    };
    let keys: &[&str] = match op {
        "set" => &["entity", "path", "value"],
        "remove" => &["entity", "path"],
        "create" => &["name", "parent", "components", "id"],
        "delete" => &["entity"],
        "reparent" => &["entity", "parent"],
        "duplicate" => &["entity", "name"],
        other => {
            return Err(error::bad_edit(format!(
                "unknown op {other:?}: use set, remove, create, delete, reparent or duplicate"
            )));
        }
    };
    if let Some(k) = m
        .keys()
        .find(|k| k.as_str() != "op" && !keys.contains(&k.as_str()))
    {
        return Err(error::bad_edit(format!(
            "a {op} edit takes {}; not {k:?}",
            keys.join(", ")
        )));
    }
    let tree = Tree::new(root);
    let target = || -> Result<String, Error> {
        match m.get("entity") {
            Some(Value::String(r)) => refs::resolve(&tree, r),
            Some(_) => Err(error::bad_edit(
                "\"entity\" is a reference: an entity id, a name or a path of names",
            )),
            None => Err(error::bad_edit(format!("a {op} edit needs \"entity\""))),
        }
    };
    let parent = |required: bool| -> Result<Option<String>, Error> {
        match m.get("parent") {
            None if required => Err(error::bad_edit(
                "a reparent edit needs \"parent\": an entity, or null for the top level",
            )),
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(r)) => refs::resolve(&tree, r).map(Some),
            Some(_) => Err(error::bad_edit("\"parent\" is an entity reference or null")),
        }
    };
    // Whatever walks beneath an entity on a parent cycle would never end.
    let acyclic = |id: &str| -> Result<(), Error> {
        if tree.in_cycle(id) {
            return Err(error::invalid(format!(
                "{} is on a parent cycle; reparent it to null (the top level) first",
                tree.label(id)
            )));
        }
        Ok(())
    };
    match op {
        "set" | "remove" => {
            let id = target()?;
            let keys = field_path(root, &id, m.get("path"), registry)?;
            let mut full = scene::path(&["entities", &id]);
            full.extend(keys.iter().cloned());
            if op == "remove" {
                if doc::get(root, &full).is_none() {
                    return Err(error::not_found(format!(
                        "{} has no {}",
                        tree.label(&id),
                        keys.join(".")
                    )));
                }
                return Ok((vec![Op::Remove { path: full }], None));
            }
            let value = m
                .get("value")
                .cloned()
                .ok_or_else(|| error::bad_edit("a set edit needs \"value\""))?;
            let op = match keys.as_slice() {
                [first, kind, field @ ..] if first == "components" => {
                    let field: Vec<&str> = field.iter().map(String::as_str).collect();
                    scene::set_field(&id, kind, &field, value)
                }
                _ => Op::Set { path: full, value },
            };
            Ok((vec![op], None))
        }
        "create" => {
            let name = match m.get("name") {
                Some(Value::String(s)) => s.clone(),
                _ => return Err(error::bad_edit("a create edit needs \"name\": a string")),
            };
            let parent = parent(false)?;
            let components: BTreeMap<String, Value> = match m.get("components") {
                None => BTreeMap::new(),
                Some(Value::Object(c)) => c.clone(),
                Some(_) => {
                    return Err(error::bad_edit(
                        "\"components\" is an object of components by kind",
                    ));
                }
            };
            let id = match m.get("id") {
                None => ids.next(root),
                Some(Value::String(s)) if is_entity_id(s) => {
                    if tree.contains(s) || !ids.taken.insert(s.clone()) {
                        return Err(error::bad_edit(format!("entity {s} already exists")));
                    }
                    s.clone()
                }
                Some(_) => {
                    return Err(error::bad_edit(
                        "\"id\" is a new entity id: 16 lowercase hex digits",
                    ));
                }
            };
            let ops = scene::create(&id, &name, parent.as_deref(), None, components);
            Ok((ops, Some(id)))
        }
        "delete" => {
            let id = target()?;
            acyclic(&id)?;
            Ok((scene::delete(root, &[&id], load), None))
        }
        "reparent" => {
            let id = target()?;
            let parent = parent(true)?;
            if parent.is_some() {
                acyclic(&id)?;
            }
            let ops = scene::reparent(root, &id, parent.as_deref(), None, load)
                .map_err(error::bad_edit)?;
            Ok((ops, None))
        }
        _ => {
            // duplicate
            let id = target()?;
            acyclic(&id)?;
            let name = match m.get("name") {
                None => None,
                Some(Value::String(s)) => Some(s.clone()),
                Some(_) => return Err(error::bad_edit("\"name\" is a string")),
            };
            let (mut ops, tops) = scene::duplicate(root, &[&id], || ids.next(root));
            let top = tops.into_iter().next();
            if let (Some(name), Some(top)) = (&name, &top) {
                for op in &mut ops {
                    if let Op::Set {
                        path,
                        value: Value::Object(e),
                    } = op
                    {
                        if path.len() == 2 && path[1] == *top {
                            e.insert("name".into(), Value::String(name.clone()));
                        }
                    }
                }
            }
            Ok((ops, top))
        }
    }
}

/// The keys below entity `id` that an edit's `path` names.
fn field_path(
    root: &Value,
    id: &str,
    path: Option<&Value>,
    registry: &Registry,
) -> Result<Vec<String>, Error> {
    let keys: Vec<String> = match path {
        Some(Value::Array(a)) => a
            .iter()
            .map(|k| match k {
                Value::String(s) if !s.is_empty() => Ok(s.clone()),
                _ => Err(error::bad_edit(
                    "a path array holds keys (non-empty strings)",
                )),
            })
            .collect::<Result<_, _>>()?,
        Some(Value::String(s)) => split_dotted(s, &kinds(root, id, registry))?,
        _ => {
            return Err(error::bad_edit(
                "\"path\" is a dotted string such as \"components.light.intensity\" or an array of keys",
            ));
        }
    };
    match keys.first().map(String::as_str) {
        None => Err(error::bad_edit(
            "\"path\" is empty; a delete edit removes a whole entity",
        )),
        Some("parent") => Err(error::bad_edit(
            "an entity's parent changes with a reparent edit",
        )),
        Some(k) if !ENTITY_KEYS.contains(&k) => Err(error::bad_edit(format!(
            "an entity has {}; not {k:?}",
            ENTITY_KEYS.join(", ")
        ))),
        _ if keys.len() > MAX_KEYS => Err(error::bad_edit("the path is too deep")),
        _ => Ok(keys),
    }
}

/// Component kinds a dotted path may name: the project's, and any the entity already holds.
fn kinds(root: &Value, id: &str, registry: &Registry) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = registry.kinds().map(String::from).collect();
    let mut stored = |c: Option<&Value>| {
        if let Some(Value::Object(c)) = c {
            out.extend(c.keys().cloned());
        }
    };
    let e = scene::entities(root).get(id);
    stored(e.and_then(|e| e.get("components")));
    if let Some(Value::Object(over)) = e
        .and_then(|e| e.get("prefab"))
        .and_then(|p| p.get("overrides"))
    {
        for o in over.values() {
            stored(o.get("components"));
        }
    }
    out
}

/// Splits a dotted path, keeping a component kind with dots in it as one key.
fn split_dotted(s: &str, kinds: &BTreeSet<String>) -> Result<Vec<String>, Error> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.iter().any(|p| p.is_empty()) {
        return Err(error::bad_edit(format!("the path {s:?} has an empty key")));
    }
    let mut out = vec![];
    let mut i = 0;
    while i < parts.len() {
        out.push(parts[i].to_string());
        if parts[i] == "components" && i + 1 < parts.len() {
            let mut take = 1;
            for n in 2..parts.len() - i {
                if kinds.contains(&parts[i + 1..i + 1 + n].join(".")) {
                    take = n;
                }
            }
            out.push(parts[i + 1..i + 1 + take].join("."));
            i += 1 + take;
        } else {
            i += 1;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dotted_paths_keep_namespaced_kinds_whole() {
        let kinds: BTreeSet<String> = ["light", "arena.spawn", "arena"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let split = |s: &str| split_dotted(s, &kinds).unwrap();
        assert_eq!(
            split("components.light.intensity"),
            ["components", "light", "intensity"]
        );
        assert_eq!(
            split("components.arena.spawn.team"),
            ["components", "arena.spawn", "team"]
        );
        assert_eq!(split("components.arena.x"), ["components", "arena", "x"]);
        assert_eq!(
            split("components.nope.x.y"),
            ["components", "nope", "x", "y"]
        );
        assert_eq!(
            split("prefab.overrides.00000000000000a1.components.arena.spawn"),
            [
                "prefab",
                "overrides",
                "00000000000000a1",
                "components",
                "arena.spawn"
            ]
        );
        assert_eq!(split("name"), ["name"]);
        assert!(split_dotted("components..x", &kinds).is_err());
    }
}
