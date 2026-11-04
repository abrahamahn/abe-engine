//! The entity tree of a document and entity references.
//!
//! A reference (REF) is an entity's 16-hex id, its name when no other entity has it, or a path
//! of names from a root separated by `/` (a leading `/` is allowed). In a name, `\/` stands for
//! a `/` and `\\` for a `\`. Siblings sort as in `ganbate_scene::scene`: by order key (entities
//! without one last), then name, then id. Every walk here stops at parent cycles, which the
//! scene checker reports and an agent may have to repair.
use std::collections::{BTreeMap, BTreeSet};

use ganbate_core::id::is_entity_id;
use ganbate_core::json::Value;
use ganbate_scene::scene;

use crate::error::{self, Error};
use crate::json::{J, Obj};

/// At most this many candidates are listed for an ambiguous or missing reference.
const MAX_CANDIDATES: usize = 20;

fn text<'a>(e: &'a Value, key: &str) -> Option<&'a str> {
    match e.get(key) {
        Some(Value::String(s)) => Some(s),
        _ => None,
    }
}

pub struct Tree<'a> {
    ents: &'a BTreeMap<String, Value>,
    roots: Vec<&'a str>,
    kids: BTreeMap<&'a str, Vec<&'a str>>,
}

impl<'a> Tree<'a> {
    pub fn new(root: &'a Value) -> Self {
        let ents = scene::entities(root);
        let mut roots = vec![];
        let mut kids: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (id, e) in ents {
            match text(e, "parent") {
                Some(p) => kids.entry(p).or_default().push(id),
                None => roots.push(id.as_str()),
            }
        }
        let key = |id: &&'a str| {
            let e = &ents[*id];
            let order = text(e, "order");
            (
                order.is_none(),
                order.unwrap_or(""),
                text(e, "name").unwrap_or(""),
                *id,
            )
        };
        roots.sort_by_key(key);
        for list in kids.values_mut() {
            list.sort_by_key(key);
        }
        Self { ents, roots, kids }
    }

    pub fn contains(&self, id: &str) -> bool {
        self.ents.contains_key(id)
    }
    /// `id` as the document spells it, borrowed from the document.
    pub fn key(&self, id: &str) -> Option<&'a str> {
        self.ents.get_key_value(id).map(|(k, _)| k.as_str())
    }
    pub fn len(&self) -> usize {
        self.ents.len()
    }
    pub fn ids(&self) -> impl Iterator<Item = &'a str> + '_ {
        self.ents.keys().map(String::as_str)
    }
    pub fn entity(&self, id: &str) -> Option<&'a Value> {
        self.ents.get(id)
    }
    pub fn name(&self, id: &str) -> &'a str {
        self.ents
            .get(id)
            .and_then(|e| text(e, "name"))
            .unwrap_or("")
    }
    pub fn parent(&self, id: &str) -> Option<&'a str> {
        self.ents.get(id).and_then(|e| text(e, "parent"))
    }
    /// The entities directly under `parent` (`None`: the roots), in sibling order.
    pub fn children(&self, parent: Option<&str>) -> &[&'a str] {
        match parent {
            None => &self.roots,
            Some(p) => self.kids.get(p).map_or(&[], Vec::as_slice),
        }
    }

    /// Names from the top down to `id`, each escaped, joined by `/`.
    pub fn path(&self, id: &str) -> String {
        let mut names = vec![];
        let mut seen = BTreeSet::new();
        let mut at = id;
        while seen.insert(at) {
            let Some(e) = self.ents.get(at) else {
                break;
            };
            names.push(escape(text(e, "name").unwrap_or("")));
            match text(e, "parent") {
                Some(p) if self.ents.contains_key(p) => at = p,
                _ => break,
            }
        }
        names.reverse();
        names.join("/")
    }

    /// Whether `id` lies on a parent cycle (so everything beneath it is beneath itself).
    pub fn in_cycle(&self, id: &str) -> bool {
        let mut seen = BTreeSet::new();
        let mut at = id;
        while let Some(p) = self.parent(at).filter(|p| self.ents.contains_key(*p)) {
            if p == id {
                return true;
            }
            if !seen.insert(p) {
                return false;
            }
            at = p;
        }
        false
    }

    /// Whether the document has any parent cycle.
    pub fn has_cycle(&self) -> bool {
        self.ents.keys().any(|id| self.in_cycle(id))
    }

    /// `starts` and what lies beneath them depth first in sibling order, with their depths
    /// (a start is depth 0), down to `max_depth`. With `everything`, what the starts do not
    /// reach at any depth (beneath a missing parent, or on a parent cycle) follows, from tops
    /// taken in id order.
    pub fn walk(
        &self,
        starts: &[&'a str],
        max_depth: Option<usize>,
        everything: bool,
    ) -> Vec<(&'a str, usize)> {
        let descend = |start: &'a str,
                       limit: Option<usize>,
                       seen: &mut BTreeSet<&'a str>,
                       out: &mut Vec<(&'a str, usize)>| {
            let mut stack = vec![(start, 0usize)];
            while let Some((id, depth)) = stack.pop() {
                if !seen.insert(id) {
                    continue;
                }
                out.push((id, depth));
                if limit.is_none_or(|m| depth < m) {
                    for k in self.children(Some(id)).iter().rev() {
                        stack.push((k, depth + 1));
                    }
                }
            }
        };
        let mut tops = starts.to_vec();
        if everything {
            let mut reached = BTreeSet::new();
            let mut scratch = vec![];
            for s in starts {
                descend(s, None, &mut reached, &mut scratch);
            }
            // Beneath a missing parent first, then what a cycle keeps out of reach.
            let dangling: Vec<&str> = self
                .ids()
                .filter(|id| self.parent(id).is_some_and(|p| !self.ents.contains_key(p)))
                .collect();
            for s in dangling.into_iter().chain(self.ids()) {
                if !reached.contains(s) {
                    descend(s, None, &mut reached, &mut scratch);
                    tops.push(s);
                }
            }
        }
        let (mut seen, mut out) = (BTreeSet::new(), vec![]);
        for s in tops {
            descend(s, max_depth, &mut seen, &mut out);
        }
        out
    }

    /// `"Name" (id)`, for messages.
    pub fn label(&self, id: &str) -> String {
        format!("{:?} ({id})", self.name(id))
    }

    fn candidates(&self, ids: &[&str]) -> J {
        let mut list: Vec<(String, &str)> = ids.iter().map(|id| (self.path(id), *id)).collect();
        list.sort();
        J::Arr(
            list.into_iter()
                .take(MAX_CANDIDATES)
                .map(|(path, id)| Obj::new().with("id", id).with("path", path).into())
                .collect(),
        )
    }
}

/// A name as one segment of a path.
pub fn escape(name: &str) -> String {
    name.replace('\\', "\\\\").replace('/', "\\/")
}

/// Splits a reference at its unescaped slashes; says whether there were any.
fn split(r: &str) -> (Vec<String>, bool) {
    let mut segs = vec![String::new()];
    let mut slashed = false;
    let mut chars = r.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some(n) => segs.last_mut().expect("one").push(n),
                None => segs.last_mut().expect("one").push('\\'),
            },
            '/' => {
                slashed = true;
                segs.push(String::new());
            }
            c => segs.last_mut().expect("one").push(c),
        }
    }
    (segs, slashed)
}

/// The id of the one entity `r` names (see the module notes).
pub fn resolve(tree: &Tree, r: &str) -> Result<String, Error> {
    if r.is_empty() {
        return Err(error::not_found("an empty entity reference"));
    }
    if is_entity_id(r) && tree.contains(r) {
        return Ok(r.to_string());
    }
    let (mut segs, slashed) = split(r);
    let found: Vec<&str> = if slashed {
        if segs.first().is_some_and(String::is_empty) {
            segs.remove(0);
        }
        if segs.is_empty() || segs.iter().any(String::is_empty) {
            return Err(error::not_found(format!(
                "{r:?} is not a path of entity names"
            )));
        }
        let mut at: Vec<&str> = tree
            .children(None)
            .iter()
            .copied()
            .filter(|id| tree.name(id) == segs[0])
            .collect();
        for seg in &segs[1..] {
            at = at
                .iter()
                .flat_map(|id| tree.children(Some(id)).iter().copied())
                .filter(|id| tree.name(id) == seg)
                .collect();
        }
        at
    } else {
        tree.ids().filter(|id| tree.name(id) == segs[0]).collect()
    };
    match found.as_slice() {
        [one] => Ok(one.to_string()),
        [] => {
            let wanted = segs.last().map_or(String::new(), |s| s.to_lowercase());
            let similar: Vec<&str> = tree
                .ids()
                .filter(|id| !wanted.is_empty() && tree.name(id).to_lowercase().contains(&wanted))
                .collect();
            let what = if slashed {
                "path"
            } else if is_entity_id(r) {
                "id or name"
            } else {
                "name"
            };
            let mut e = error::not_found(format!("no entity has the {what} {r:?}"));
            if !similar.is_empty() {
                e = e.with("candidates", tree.candidates(&similar));
            }
            Err(e)
        }
        many => Err(error::ambiguous(format!(
            "{r:?} names {} entities; use an id or a path",
            many.len()
        ))
        .with("candidates", tree.candidates(many))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ganbate_core::json::parse;

    fn doc() -> Value {
        parse(
            br#"{"ganbate":"scene","entities":{
              "00000000000000a1":{"name":"Street"},
              "00000000000000a2":{"name":"Lamp","parent":"00000000000000a1","order":"V"},
              "00000000000000a3":{"name":"Lamp","parent":"00000000000000a1","order":"U"},
              "00000000000000a4":{"name":"a/b","parent":"00000000000000a3"},
              "00000000000000b1":{"name":"Loop","parent":"00000000000000b2"},
              "00000000000000b2":{"name":"Back","parent":"00000000000000b1"},
              "00000000000000c1":{"name":"Orphan","parent":"00000000000000ff"}}}"#,
        )
        .unwrap()
    }

    #[test]
    fn references_resolve_by_id_name_and_path() {
        let d = doc();
        let t = Tree::new(&d);
        assert_eq!(resolve(&t, "00000000000000a2").unwrap(), "00000000000000a2");
        assert_eq!(resolve(&t, "Street").unwrap(), "00000000000000a1");
        assert_eq!(resolve(&t, "/Street").unwrap(), "00000000000000a1");
        let e = resolve(&t, "Lamp").unwrap_err();
        assert_eq!(e.code, error::AMBIGUOUS);
        assert_eq!(
            resolve(&t, "Street/Lamp").unwrap_err().code,
            error::AMBIGUOUS
        );
        assert_eq!(resolve(&t, r"a\/b").unwrap(), "00000000000000a4");
        assert_eq!(t.path("00000000000000a4"), r"Street/Lamp/a\/b");
        assert_eq!(resolve(&t, "Nope").unwrap_err().code, error::NOT_FOUND);
        assert_eq!(
            resolve(&t, "Street//Lamp").unwrap_err().code,
            error::NOT_FOUND
        );
        // Siblings in order-key order.
        assert_eq!(
            t.children(Some("00000000000000a1")),
            ["00000000000000a3", "00000000000000a2"]
        );
    }

    #[test]
    fn walks_stop_at_cycles_and_reach_everything() {
        let d = doc();
        let t = Tree::new(&d);
        assert!(t.in_cycle("00000000000000b1"));
        assert!(!t.in_cycle("00000000000000a4"));
        assert!(t.has_cycle());
        let all = t.walk(t.children(None), None, true);
        assert_eq!(all.len(), 7);
        let shallow = t.walk(t.children(None), Some(1), false);
        assert_eq!(shallow.len(), 3);
        // At depth 0: the root, the orphan's top and the first entity of the cycle.
        let tops: Vec<&str> = t
            .walk(t.children(None), Some(0), true)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(
            tops,
            ["00000000000000a1", "00000000000000c1", "00000000000000b1"]
        );
        assert_eq!(t.path("00000000000000b1"), "Back/Loop");
    }
}
