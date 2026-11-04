//! Problems in a document. Errors come from the scene checker (`ganbate_scene::scene::check`): the
//! rules Ganbate Studio enforces on every edit and the cook before it builds. Warnings are
//! references that point nowhere: assets missing from the project or of the wrong kind,
//! entities missing from the scene (or from what its prefab instances show).
use std::collections::{BTreeMap, BTreeSet};

use ganbate_core::id::{is_asset_id, is_entity_id};
use ganbate_core::json::Value;
use ganbate_scene::scene;
use ganbate_scene::schema::{Field, FieldType};

use crate::Reply;
use crate::error;
use crate::json::{J, Obj};
use crate::refs::Tree;
use crate::workspace::{Located, SceneFile, Workspace, about};

/// An issue as answers report it: where, what, and the entity it is about.
pub fn issue(tree: &Tree, severity: Option<&str>, path: &str, message: &str) -> Obj {
    let mut o = Obj::new();
    if let Some(s) = severity {
        o.put("severity", s);
    }
    o.put("path", path);
    o.put("message", message);
    let id = path
        .strip_prefix("entities.")
        .and_then(|rest| rest.split('.').next());
    if let Some(id) = id.filter(|id| tree.contains(id)) {
        o.put("entity", id);
        o.put("name", tree.name(id));
    }
    o
}

pub fn run(ws: &Workspace, at: &Located, file: &SceneFile) -> Reply {
    let tree = Tree::new(&file.root);
    let errors = scene::check(&file.root, &ws.registry);
    let warnings = warnings(ws, &file.root);
    let mut issues: Vec<J> = errors
        .iter()
        .map(|i| issue(&tree, Some("error"), &i.path, &i.message).into())
        .collect();
    issues.extend(
        warnings
            .iter()
            .map(|(p, m)| J::from(issue(&tree, Some("warning"), p, m))),
    );
    let body = Obj::new()
        .with("scene", about(at, &file.root))
        .with("errors", errors.len())
        .with("warnings", warnings.len())
        .with("issues", issues);
    let failure = (!errors.is_empty()).then(|| {
        error::invalid(format!(
            "{} has {} error{}",
            at.rel,
            errors.len(),
            if errors.len() == 1 { "" } else { "s" }
        ))
    });
    Reply {
        body,
        error: failure,
    }
}

/// References that point nowhere, as (path, message).
pub fn warnings(ws: &Workspace, root: &Value) -> Vec<(String, String)> {
    let tree = Tree::new(root);
    // What a reference may name: the document's entities and those its instances show.
    let mut known: BTreeSet<String> = tree.ids().map(String::from).collect();
    if !tree.has_cycle() {
        if let Ok(all) = scene::flatten(root, &|id| ws.prefab(id)) {
            known.extend(all.into_iter().map(|r| r.id));
        }
    }
    let mut w = Walker {
        ws,
        known: &known,
        out: vec![],
    };
    for (id, e) in scene::entities(root) {
        let at = format!("entities.{id}");
        if let Some(Value::String(p)) = e.get("parent") {
            if is_entity_id(p) && p != id && !known.contains(p) {
                w.out.push((
                    format!("{at}.parent"),
                    format!("parent {p} is not in the scene; the entity is placed at the top"),
                ));
            }
        }
        if let Some(Value::Object(comps)) = e.get("components") {
            for (kind, c) in comps {
                w.component(kind, c, &format!("{at}.components.{kind}"));
            }
        }
        let Some(link) = e.get("prefab") else {
            continue;
        };
        if let Some(Value::String(a)) = link.get("asset") {
            if is_asset_id(a) {
                w.asset(a, Some("prefab"), &format!("{at}.prefab.asset"));
            }
        }
        if let Some(Value::Object(over)) = link.get("overrides") {
            for (inner, o) in over {
                if let Some(Value::Object(comps)) = o.get("components") {
                    for (kind, c) in comps {
                        let p = format!("{at}.prefab.overrides.{inner}.components.{kind}");
                        w.component(kind, c, &p);
                    }
                }
            }
        }
    }
    w.out
}

struct Walker<'a> {
    ws: &'a Workspace,
    known: &'a BTreeSet<String>,
    out: Vec<(String, String)>,
}

impl Walker<'_> {
    fn component(&mut self, kind: &str, c: &Value, at: &str) {
        if let (Some(schema), Value::Object(map)) = (self.ws.registry.get(kind), c) {
            self.fields(&schema.fields, map, at);
        }
    }
    fn fields(
        &mut self,
        fields: &BTreeMap<String, Field>,
        map: &BTreeMap<String, Value>,
        at: &str,
    ) {
        for (k, v) in map {
            if let Some(f) = fields.get(k) {
                self.value(f, v, &format!("{at}.{k}"));
            }
        }
    }
    fn value(&mut self, f: &Field, v: &Value, at: &str) {
        match (&f.ty, v) {
            (FieldType::Asset { kind }, Value::String(s)) if is_asset_id(s) => {
                self.asset(s, kind.as_deref(), at);
            }
            (FieldType::Entity, Value::String(s)) if is_entity_id(s) && !self.known.contains(s) => {
                self.out
                    .push((at.to_string(), format!("no entity {s} in the scene")));
            }
            (FieldType::Array { items, .. }, Value::Array(a)) => {
                for (i, x) in a.iter().enumerate() {
                    self.value(items, x, &format!("{at}[{i}]"));
                }
            }
            (FieldType::Object(fields), Value::Object(m)) => self.fields(fields, m, at),
            _ => {}
        }
    }
    fn asset(&mut self, id: &str, kind: Option<&str>, at: &str) {
        match self.ws.project.entry(id) {
            None => self
                .out
                .push((at.to_string(), format!("no asset {id} in the project"))),
            Some(e) => {
                if let Some(k) = kind.filter(|k| *k != e.kind.name()) {
                    self.out.push((
                        at.to_string(),
                        format!("asset {id} is a {} ({}), not a {k}", e.kind.name(), e.path),
                    ));
                }
            }
        }
    }
}
