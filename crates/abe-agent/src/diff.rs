//! `abe scene diff`: two scene or prefab documents compared by entity id. Entities only in B
//! are `added`, only in A `removed`; for entities in both, a new name is `renamed`, a new
//! parent `reparented`, and every other value that differs is a `changed` field (path, before,
//! after; objects are compared key by key, arrays as whole values). Header fields (the
//! document's name, id, version) go to `document`. Everything comes in id order, then path
//! order, so the same documents always give the same answer.
use std::collections::BTreeSet;

use abe_core::json::Value;
use abe_scene::scene;

use crate::json::{J, Obj};

fn text(e: &Value, key: &str) -> Option<String> {
    e.get(key).and_then(|v| v.string().ok()).map(String::from)
}

/// Values that differ between `a` and `b`, below `path`.
fn fields(
    path: &mut Vec<String>,
    a: Option<&Value>,
    b: Option<&Value>,
    out: &mut Vec<(String, Option<Value>, Option<Value>)>,
) {
    match (a, b) {
        (Some(Value::Object(ma)), Some(Value::Object(mb))) => {
            let keys: BTreeSet<&String> = ma.keys().chain(mb.keys()).collect();
            for k in keys {
                path.push(k.clone());
                fields(path, ma.get(k), mb.get(k), out);
                path.pop();
            }
        }
        (x, y) if x == y => {}
        (x, y) => out.push((path.join("."), x.cloned(), y.cloned())),
    }
}

fn change(path: String, before: Option<Value>, after: Option<Value>) -> Obj {
    Obj::new()
        .with("path", path)
        .with("before", before)
        .with("after", after)
}

pub fn diff(a: &Value, b: &Value) -> Obj {
    let mut document = vec![];
    let header = |v: &Value| match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .filter(|(k, _)| k.as_str() != "entities")
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        ),
        other => other.clone(),
    };
    let mut found = vec![];
    fields(&mut vec![], Some(&header(a)), Some(&header(b)), &mut found);
    for (p, x, y) in found {
        document.push(J::from(change(p, x, y)));
    }
    let (ea, eb) = (scene::entities(a), scene::entities(b));
    let entity = |id: &str, e: &Value| -> J {
        Obj::new()
            .with("id", id)
            .with("name", text(e, "name"))
            .with("parent", text(e, "parent"))
            .with("value", e)
            .into()
    };
    let added: Vec<J> = eb
        .iter()
        .filter(|(id, _)| !ea.contains_key(*id))
        .map(|(id, e)| entity(id, e))
        .collect();
    let removed: Vec<J> = ea
        .iter()
        .filter(|(id, _)| !eb.contains_key(*id))
        .map(|(id, e)| entity(id, e))
        .collect();
    let (mut renamed, mut reparented, mut changed) = (vec![], vec![], vec![]);
    for (id, x) in ea {
        let Some(y) = eb.get(id) else {
            continue;
        };
        let name = text(y, "name").or_else(|| text(x, "name"));
        if x.get("name") != y.get("name") {
            renamed.push(J::from(
                Obj::new()
                    .with("id", id.as_str())
                    .with("before", x.get("name"))
                    .with("after", y.get("name")),
            ));
        }
        if x.get("parent") != y.get("parent") {
            reparented.push(J::from(
                Obj::new()
                    .with("id", id.as_str())
                    .with("name", name.clone())
                    .with("before", x.get("parent"))
                    .with("after", y.get("parent")),
            ));
        }
        let strip = |v: &Value| match v {
            Value::Object(m) => Value::Object(
                m.iter()
                    .filter(|(k, _)| !matches!(k.as_str(), "name" | "parent"))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            ),
            other => other.clone(),
        };
        let mut found = vec![];
        fields(&mut vec![], Some(&strip(x)), Some(&strip(y)), &mut found);
        for (p, before, after) in found {
            changed.push(J::from(
                Obj::new()
                    .with("entity", id.as_str())
                    .with("name", name.clone())
                    .with("path", p)
                    .with("before", before)
                    .with("after", after),
            ));
        }
    }
    let equal = document.is_empty()
        && added.is_empty()
        && removed.is_empty()
        && renamed.is_empty()
        && reparented.is_empty()
        && changed.is_empty();
    Obj::new()
        .with("equal", equal)
        .with("document", document)
        .with("added", added)
        .with("removed", removed)
        .with("renamed", renamed)
        .with("reparented", reparented)
        .with("changed", changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use abe_core::json::parse;

    #[test]
    fn every_kind_of_change_is_reported_in_order() {
        let a = parse(
            br#"{"abe":"scene","name":"A","entities":{
              "00000000000000a1":{"name":"Keep","components":{"light":{"intensity":1,"color":[1,1,1]}}},
              "00000000000000a2":{"name":"Gone"},
              "00000000000000a3":{"name":"Old","parent":"00000000000000a1"}}}"#,
        )
        .unwrap();
        let b = parse(
            br#"{"abe":"scene","name":"B","entities":{
              "00000000000000a1":{"name":"Keep","components":{"light":{"intensity":2,"color":[1,1,1]},"mesh":{}}},
              "00000000000000a3":{"name":"New","order":"V"},
              "00000000000000a4":{"name":"Fresh"}}}"#,
        )
        .unwrap();
        let d = J::from(diff(&a, &b));
        let text = crate::json::write(&d, false);
        assert_eq!(
            text,
            concat!(
                r#"{"equal":false,"document":[{"path":"name","before":"A","after":"B"}],"#,
                r#""added":[{"id":"00000000000000a4","name":"Fresh","parent":null,"value":{"name":"Fresh"}}],"#,
                r#""removed":[{"id":"00000000000000a2","name":"Gone","parent":null,"value":{"name":"Gone"}}],"#,
                r#""renamed":[{"id":"00000000000000a3","before":"Old","after":"New"}],"#,
                r#""reparented":[{"id":"00000000000000a3","name":"New","before":"00000000000000a1","after":null}],"#,
                r#""changed":[{"entity":"00000000000000a1","name":"Keep","path":"components.light.intensity","before":1,"after":2},"#,
                r#"{"entity":"00000000000000a1","name":"Keep","path":"components.mesh","before":null,"after":{}},"#,
                r#"{"entity":"00000000000000a3","name":"New","path":"order","before":null,"after":"V"}]}"#
            )
        );
        assert_eq!(
            crate::json::write(&diff(&a, &a).into(), false),
            r#"{"equal":true,"document":[],"added":[],"removed":[],"renamed":[],"reparented":[],"changed":[]}"#
        );
    }
}
