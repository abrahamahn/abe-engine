//! `ganbate scene inspect`: entities in tree order (depth first, siblings in order) with their id,
//! name, path, parent, depth, number of children and components (with `--defaults`, every
//! field a component leaves out filled with its schema default).
use ganbate_core::json::Value;

use crate::Reply;
use crate::args::Args;
use crate::error::{self, Error};
use crate::json::{J, Obj};
use crate::refs::{self, Tree};
use crate::workspace::{Located, SceneFile, Workspace, about};

fn components(e: &Value) -> Option<&std::collections::BTreeMap<String, Value>> {
    match e.get("components") {
        Some(Value::Object(c)) => Some(c),
        _ => None,
    }
}

pub fn run(ws: &Workspace, at: &Located, file: &SceneFile, args: &Args) -> Result<Reply, Error> {
    let root = &file.root;
    let tree = Tree::new(root);
    let depth = match args.value("--depth")? {
        None => None,
        Some(d) => Some(
            d.parse::<usize>()
                .map_err(|_| error::usage(format!("--depth takes a whole number, not {d:?}")))?,
        ),
    };
    let kinds: Vec<&str> = args
        .values("--component")
        .iter()
        .map(String::as_str)
        .collect();
    for kind in &kinds {
        let used = tree.ids().any(|id| {
            tree.entity(id)
                .and_then(components)
                .is_some_and(|c| c.contains_key(*kind))
        });
        if ws.registry.get(kind).is_none() && !used {
            return Err(error::not_found(format!(
                "no component kind {kind:?} in the project's schemas or the scene"
            )));
        }
    }
    let defaults = args.switch("--defaults");
    let (starts, everything) = match args.value("--entity")? {
        Some(r) => {
            let id = refs::resolve(&tree, r)?;
            (tree.key(&id).into_iter().collect::<Vec<_>>(), false)
        }
        None => (tree.children(None).to_vec(), true),
    };
    let mut list = vec![];
    for (id, d) in tree.walk(&starts, depth, everything) {
        let Some(e) = tree.entity(id) else {
            continue;
        };
        let comps = components(e);
        if !kinds.is_empty()
            && !kinds
                .iter()
                .any(|k| comps.is_some_and(|c| c.contains_key(*k)))
        {
            continue;
        }
        let mut o = Obj::new()
            .with("id", id)
            .with("name", e.get("name").map(J::from))
            .with("path", tree.path(id))
            .with("depth", d)
            .with("parent", e.get("parent").map(J::from))
            .with("children", tree.children(Some(id)).len());
        match (comps, e.get("components")) {
            (Some(c), _) => {
                let mut shown = Obj::new();
                for (kind, v) in c {
                    if kinds.is_empty() || kinds.contains(&kind.as_str()) {
                        let v = if defaults {
                            ws.with_defaults(kind, v)
                        } else {
                            v.clone()
                        };
                        shown.put(kind, v);
                    }
                }
                o.put("components", shown);
            }
            (None, Some(other)) => o.put("components", other),
            (None, None) => o.put("components", Obj::new()),
        }
        if let Some(p) = e.get("prefab") {
            o.put("prefab", p);
        }
        list.push(J::from(o));
    }
    let body = Obj::new()
        .with("scene", about(at, root))
        .with("total", tree.len())
        .with("count", list.len())
        .with("entities", list);
    Ok(Reply::ok(body))
}
