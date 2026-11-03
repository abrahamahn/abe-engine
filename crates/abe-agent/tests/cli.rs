//! The `abe` command line through its library entry point (`abe_agent::run`), on a synthetic
//! project built in a temporary folder: a street with two lamps (one with a bulb), a park with
//! a bench carrying a game component, and a camera.
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;

use abe_agent::render::{Backend, Rendered, Shot, Still};
use abe_core::id::Ids;
use abe_core::json::{self, Value};
use abe_pipeline::project::{Kind, Project, write_json};
use abe_scene::doc::Document;
use abe_scene::scene;

const STREET: &str = "00000000000000a1";
const LAMP1: &str = "00000000000000a2";
const LAMP2: &str = "00000000000000a3";
const BULB: &str = "00000000000000a4";
const PARK: &str = "00000000000000b1";
const BENCH: &str = "00000000000000b2";
const CAMERA: &str = "00000000000000c1";
const SCENE: &str = "content/street.scene.json";

const SCHEMA: &str = r#"{"abe":"schema","version":1,"components":{"test.marker":{"label":"Marker","fields":{
  "size":{"type":"float","min":0,"max":10,"default":1},
  "target":{"type":"entity"},
  "art":{"type":"asset","kind":"model"}}}}}"#;

struct Fixture {
    dir: PathBuf,
    scene_id: String,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Fixture {
    fn p(&self) -> String {
        self.dir.display().to_string()
    }
    fn scene_path(&self) -> PathBuf {
        self.dir.join(SCENE)
    }
    fn bytes(&self) -> Vec<u8> {
        std::fs::read(self.scene_path()).unwrap()
    }
    fn doc(&self) -> Value {
        json::parse(&self.bytes()).unwrap()
    }
    fn history(&self) -> PathBuf {
        self.dir
            .join(".abe/history")
            .join(format!("{}.jsonl", self.scene_id))
    }
    fn history_lines(&self) -> usize {
        std::fs::read_to_string(self.history()).map_or(0, |t| t.lines().count())
    }
    fn apply(&self, edits: &str, extra: &[&str]) -> (i32, Value) {
        let mut args = vec!["scene", "apply", self.dir.to_str().unwrap(), SCENE, "-"];
        args.extend_from_slice(extra);
        abe_in(&args, edits)
    }
    fn undo(&self) -> (i32, Value) {
        abe(&["scene", "undo", &self.p(), SCENE])
    }
}

fn comps(pairs: &[(&str, &str)]) -> BTreeMap<String, Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), json::parse(v.as_bytes()).unwrap()))
        .collect()
}

fn fixture(tag: &str) -> Fixture {
    let dir = std::env::temp_dir().join(format!("abe-agent-{tag}-{}", Ids::from_os().entity()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut ids = Ids::seeded(42);
    let mut p = Project::create(&dir, "Agent test", &mut ids).unwrap();
    std::fs::write(dir.join("schemas/test.schema.json"), SCHEMA).unwrap();
    let scene_id = ids.asset();
    let mut doc = Document::new(scene::new_document(
        scene::Kind::Scene,
        &scene_id,
        "Street scene",
    ))
    .unwrap();
    fn t(x: &str) -> (&str, &str) {
        ("transform", x)
    }
    for ops in [
        scene::create(
            STREET,
            "Street",
            None,
            None,
            comps(&[t(r#"{"translation":[0,0,-5]}"#)]),
        ),
        scene::create(
            LAMP1,
            "Lamp",
            Some(STREET),
            Some("U"),
            comps(&[
                t(r#"{"translation":[1,0,0]}"#),
                ("light", r#"{"intensity":1}"#),
            ]),
        ),
        scene::create(
            LAMP2,
            "Lamp",
            Some(STREET),
            Some("V"),
            comps(&[("light", r#"{"intensity":3,"kind":"point"}"#)]),
        ),
        scene::create(
            BULB,
            "Bulb",
            Some(LAMP1),
            None,
            comps(&[t(r#"{"translation":[0,2,0]}"#)]),
        ),
        scene::create(PARK, "Park", None, None, BTreeMap::new()),
        scene::create(
            BENCH,
            "Bench",
            Some(PARK),
            None,
            comps(&[("test.marker", r#"{"size":2}"#)]),
        ),
        scene::create(
            CAMERA,
            "Camera",
            None,
            None,
            comps(&[("camera", r#"{"fov":40}"#), t(r#"{"translation":[0,2,8]}"#)]),
        ),
    ] {
        doc.apply_all(&ops).unwrap();
    }
    p.write_document(SCENE, Kind::Scene, doc.root()).unwrap();
    Fixture { dir, scene_id }
}

fn abe(args: &[&str]) -> (i32, Value) {
    abe_in(args, "")
}

fn abe_in(args: &[&str], stdin: &str) -> (i32, Value) {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let out = abe_agent::run(&args, &mut stdin.as_bytes(), None);
    assert!(!out.stdout.contains('\n'), "not one line: {}", out.stdout);
    let v = json::parse(out.stdout.as_bytes()).unwrap_or_else(|e| panic!("{e}: {}", out.stdout));
    assert_eq!(
        v.get("ok"),
        Some(&Value::Bool(out.status == 0)),
        "{}",
        out.stdout
    );
    (out.status, v)
}

/// The value at a `/`-separated path (array elements by index).
fn at<'a>(v: &'a Value, path: &str) -> &'a Value {
    let mut cur = v;
    for k in path.split('/') {
        cur = match cur {
            Value::Array(a) => a
                .get(k.parse::<usize>().unwrap())
                .unwrap_or_else(|| panic!("no [{k}] in {path}")),
            _ => cur
                .get(k)
                .unwrap_or_else(|| panic!("no {k:?} in {path}: {cur:?}")),
        };
    }
    cur
}

fn s(v: &Value) -> &str {
    v.string().unwrap()
}

fn n(x: f64) -> Value {
    Value::Number(x)
}

fn code(v: &Value) -> &str {
    s(at(v, "error/code"))
}

fn ids(v: &Value) -> Vec<String> {
    at(v, "entities")
        .array()
        .unwrap()
        .iter()
        .map(|e| s(at(e, "id")).to_string())
        .collect()
}

#[test]
fn inspect_finds_entities_by_id_name_and_path() {
    let f = fixture("inspect");
    let (status, v) = abe(&["scene", "inspect", &f.p(), SCENE]);
    assert_eq!(status, 0);
    // Tree order: roots by name (none has an order key), children by order key.
    assert_eq!(ids(&v), [CAMERA, PARK, BENCH, STREET, LAMP1, BULB, LAMP2]);
    assert_eq!((at(&v, "total"), at(&v, "count")), (&n(7.0), &n(7.0)));
    assert_eq!(s(at(&v, "scene/id")), f.scene_id);

    let (_, v) = abe(&["scene", "inspect", &f.p(), SCENE, "--entity", LAMP1]);
    assert_eq!(ids(&v), [LAMP1, BULB]);
    let lamp = at(&v, "entities/0");
    assert_eq!(s(at(lamp, "path")), "Street/Lamp");
    assert_eq!(s(at(lamp, "parent")), STREET);
    assert_eq!(at(lamp, "children"), &n(1.0));
    assert_eq!(at(lamp, "components/light/intensity"), &n(1.0));
    assert_eq!(at(&v, "entities/1/depth"), &n(1.0));

    let (_, v) = abe(&["scene", "inspect", &f.p(), SCENE, "--entity", "Bulb"]);
    assert_eq!(ids(&v), [BULB]);
    assert_eq!(s(at(&v, "entities/0/path")), "Street/Lamp/Bulb");

    for path in ["Park/Bench", "/Park/Bench"] {
        let (_, v) = abe(&["scene", "inspect", &f.p(), SCENE, "--entity", path]);
        assert_eq!(ids(&v), [BENCH], "{path}");
    }
    // The scene by asset id, and by a file path inside the project.
    let file = f.scene_path().display().to_string();
    for scene in [f.scene_id.as_str(), file.as_str()] {
        let (_, v) = abe(&["scene", "inspect", &f.p(), scene, "--entity", "Camera"]);
        assert_eq!(ids(&v), [CAMERA]);
    }
}

#[test]
fn ambiguous_and_missing_references_are_refused() {
    let f = fixture("refs");
    for r in ["Lamp", "Street/Lamp"] {
        let (status, v) = abe(&["scene", "inspect", &f.p(), SCENE, "--entity", r]);
        assert_eq!((status, code(&v)), (1, "ambiguous-ref"), "{r}");
        let found: Vec<&str> = at(&v, "error/candidates")
            .array()
            .unwrap()
            .iter()
            .map(|c| s(at(c, "id")))
            .collect();
        assert_eq!(found, [LAMP1, LAMP2]);
    }
    for r in ["Nope", "Street/Nope", "ffffffffffffffff", "Street//Lamp"] {
        let (status, v) = abe(&["scene", "inspect", &f.p(), SCENE, "--entity", r]);
        assert_eq!((status, code(&v)), (1, "not-found"), "{r}");
    }
    // A near miss suggests what exists.
    let (_, v) = abe(&["scene", "inspect", &f.p(), SCENE, "--entity", "bench"]);
    assert_eq!(s(at(&v, "error/candidates/0/id")), BENCH);
    let (_, v) = abe(&["scene", "inspect", &f.p(), "content/none.scene.json"]);
    assert_eq!(code(&v), "not-found");
    let nowhere = f.dir.join("nowhere").display().to_string();
    let (_, v) = abe(&["scene", "inspect", &nowhere, SCENE]);
    assert_eq!(code(&v), "not-found");
    // Command-line mistakes exit 2.
    for args in [
        &["scene", "inspect", "only-one"][..],
        &["scene", "inspect", "a", "b", "--dry-run"],
        &["scene", "explode"],
        &["scene", "inspect", "a", "b", "--depth"],
    ] {
        let (status, v) = abe(args);
        assert_eq!((status, code(&v)), (2, "usage"), "{args:?}");
    }
}

#[test]
fn inspect_filters_by_component_and_depth_and_fills_defaults() {
    let f = fixture("filter");
    let (_, v) = abe(&["scene", "inspect", &f.p(), SCENE, "--depth", "0"]);
    assert_eq!(ids(&v), [CAMERA, PARK, STREET]);
    let (_, v) = abe(&["scene", "inspect", &f.p(), SCENE, "--component", "light"]);
    assert_eq!(ids(&v), [LAMP1, LAMP2]);
    let comps = at(&v, "entities/0/components");
    assert_eq!(comps.get("transform"), None, "only the asked-for kinds");
    assert_eq!(comps.get("light").and_then(|l| l.get("range")), None);
    let (_, v) = abe(&[
        "scene",
        "inspect",
        &f.p(),
        SCENE,
        "--component",
        "light",
        "--defaults",
    ]);
    assert_eq!(at(&v, "entities/0/components/light/range"), &n(6.0));
    assert_eq!(s(at(&v, "entities/0/components/light/kind")), "point");
    assert_eq!(at(&v, "entities/0/components/light/intensity"), &n(1.0));
    // Nested defaults, and stored values winning over them.
    let (_, v) = abe(&[
        "scene",
        "inspect",
        &f.p(),
        SCENE,
        "--entity",
        "Camera",
        "--defaults",
    ]);
    assert_eq!(at(&v, "entities/0/components/camera/focus/depth"), &n(4.0));
    assert_eq!(at(&v, "entities/0/components/camera/fov"), &n(40.0));
    // A game's kind (with a dot), and a kind nobody knows.
    let (_, v) = abe(&[
        "scene",
        "inspect",
        &f.p(),
        SCENE,
        "--component",
        "test.marker",
    ]);
    assert_eq!(ids(&v), [BENCH]);
    let (_, v) = abe(&["scene", "inspect", &f.p(), SCENE, "--component", "lights"]);
    assert_eq!(code(&v), "not-found");
}

#[test]
fn set_and_remove_change_fields() {
    let f = fixture("set");
    let edits = format!(
        r#"{{"label":"Tune","edits":[
          {{"op":"set","entity":"{LAMP1}","path":"components.light.intensity","value":2}},
          {{"op":"set","entity":"Park/Bench","path":"components.test.marker.size","value":5}},
          {{"op":"set","entity":"Park","path":["name"],"value":"Garden"}},
          {{"op":"remove","entity":"{LAMP2}","path":"components.light"}}]}}"#
    );
    let (status, v) = f.apply(&edits, &[]);
    assert_eq!(status, 0, "{v:?}");
    assert_eq!(at(&v, "written"), &Value::Bool(true));
    let d = f.doc();
    let e = |id: &str, p: &str| at(&d, &format!("entities/{id}/{p}")).clone();
    assert_eq!(e(LAMP1, "components/light/intensity"), n(2.0));
    assert_eq!(e(BENCH, "components/test.marker/size"), n(5.0));
    assert_eq!(e(PARK, "name"), Value::String("Garden".into()));
    assert_eq!(
        at(&d, &format!("entities/{LAMP2}")).get("components"),
        Some(&Value::Object(BTreeMap::new()))
    );
    let changes = at(&v, "changes").array().unwrap();
    assert_eq!(changes.len(), 4);
    let first = &changes[0];
    assert_eq!(
        (
            s(at(first, "entity")),
            s(at(first, "name")),
            s(at(first, "path"))
        ),
        (LAMP1, "Lamp", "components.light.intensity")
    );
    assert_eq!(
        (at(first, "before"), at(first, "after")),
        (&n(1.0), &n(2.0))
    );
    assert_eq!(s(at(&changes[1], "path")), "components.test.marker.size");
    assert_eq!(s(at(&changes[2], "name")), "Garden");
    let removed = &changes[3];
    assert_eq!(at(removed, "after"), &Value::Null);
    assert_eq!(at(removed, "before/intensity"), &n(3.0));
    assert_eq!(f.history_lines(), 1);
    // Removing what is not there is refused.
    let (_, v) = f.apply(
        &format!(r#"{{"edits":[{{"op":"remove","entity":"{LAMP2}","path":"components.light"}}]}}"#),
        &[],
    );
    assert_eq!((code(&v), at(&v, "error/edit")), ("not-found", &n(0.0)));
}

#[test]
fn create_resolves_parents_against_earlier_edits() {
    let f = fixture("create");
    let edits = r#"{"edits":[
      {"op":"create","name":"Sign","parent":"Park","components":{"transform":{"translation":[1,2,3]}}},
      {"op":"create","name":"Arrow","parent":"Park/Sign"},
      {"op":"create","name":"Pinned","id":"00000000000000d1"}]}"#;
    let (status, v) = f.apply(edits, &[]);
    assert_eq!(status, 0, "{v:?}");
    let created = at(&v, "created").array().unwrap();
    assert_eq!(created.len(), 3);
    let sign = s(at(&created[0], "entity")).to_string();
    let arrow = s(at(&created[1], "entity")).to_string();
    assert_eq!(s(at(&created[2], "entity")), "00000000000000d1");
    let d = f.doc();
    assert_eq!(s(at(&d, &format!("entities/{sign}/parent"))), PARK);
    assert_eq!(s(at(&d, &format!("entities/{arrow}/parent"))), sign);
    assert_eq!(
        at(
            &d,
            &format!("entities/{sign}/components/transform/translation")
        ),
        &json::parse(b"[1,2,3]").unwrap()
    );
    assert_eq!(at(&d, "entities/00000000000000d1").get("parent"), None);
    // The same edits on the same document make the same ids, bytes and answer.
    let g = fixture("create-again");
    let (_, w) = g.apply(edits, &[]);
    assert_eq!(at(&w, "created"), at(&v, "created"));
    assert_eq!(at(&w, "hash"), at(&v, "hash"));
    assert_eq!(g.bytes(), f.bytes());
    // An id already in use is refused.
    let (_, v) = f.apply(
        r#"{"edits":[{"op":"create","name":"Twin","id":"00000000000000d1"}]}"#,
        &[],
    );
    assert_eq!(code(&v), "bad-edit");
}

#[test]
fn delete_takes_the_subtree() {
    let f = fixture("delete");
    let (status, v) = f.apply(r#"{"edits":[{"op":"delete","entity":"Street"}]}"#, &[]);
    assert_eq!(status, 0, "{v:?}");
    let gone: Vec<&str> = at(&v, "changes")
        .array()
        .unwrap()
        .iter()
        .map(|c| {
            assert_eq!((s(at(c, "path")), at(c, "after")), ("", &Value::Null));
            s(at(c, "entity"))
        })
        .collect();
    assert_eq!(gone, [STREET, LAMP1, LAMP2, BULB]);
    let d = f.doc();
    assert_eq!(scene::entities(&d).len(), 3);
}

#[test]
fn reparent_moves_and_refuses_cycles() {
    let f = fixture("reparent");
    let (status, v) = f.apply(
        r#"{"edits":[{"op":"reparent","entity":"Bulb","parent":"Park"},{"op":"reparent","entity":"Lamp","parent":null}]}"#,
        &[],
    );
    // "Lamp" is ambiguous: the whole transaction is refused.
    assert_eq!(
        (status, code(&v), at(&v, "error/edit")),
        (1, "ambiguous-ref", &n(1.0))
    );
    let (status, v) = f.apply(
        &format!(r#"{{"edits":[{{"op":"reparent","entity":"Bulb","parent":"Park"}},{{"op":"reparent","entity":"{LAMP2}","parent":null}}]}}"#),
        &[],
    );
    assert_eq!(status, 0, "{v:?}");
    let d = f.doc();
    assert_eq!(s(at(&d, &format!("entities/{BULB}/parent"))), PARK);
    // Moved to the top, a lamp loses its parent and its order key among the old siblings.
    let lamp = at(&d, &format!("entities/{LAMP2}"));
    assert_eq!((lamp.get("parent"), lamp.get("order")), (None, None));
    let (_, v) = abe(&["scene", "inspect", &f.p(), SCENE, "--entity", "Park/Bulb"]);
    assert_eq!(ids(&v), [BULB]);
    // Beneath itself or its own descendant: refused.
    for parent in ["Street", "Street/Lamp"] {
        let (_, v) = f.apply(
            &format!(r#"{{"edits":[{{"op":"reparent","entity":"Street","parent":"{parent}"}}]}}"#),
            &[],
        );
        assert_eq!(code(&v), "bad-edit", "{parent}");
    }
    // The parent changes only through reparent.
    let (_, v) = f.apply(
        &format!(
            r#"{{"edits":[{{"op":"set","entity":"Bulb","path":"parent","value":"{STREET}"}}]}}"#
        ),
        &[],
    );
    assert_eq!(code(&v), "bad-edit");
}

#[test]
fn duplicate_copies_the_subtree_under_new_ids() {
    let f = fixture("duplicate");
    let (status, v) = f.apply(
        r#"{"edits":[{"op":"duplicate","entity":"Street","name":"Avenue"},{"op":"duplicate","entity":"Park"}]}"#,
        &[],
    );
    assert_eq!(status, 0, "{v:?}");
    let created = at(&v, "created").array().unwrap();
    let avenue = s(at(&created[0], "entity")).to_string();
    let park2 = s(at(&created[1], "entity")).to_string();
    assert_eq!(s(at(&created[0], "name")), "Avenue");
    assert_eq!(s(at(&created[1], "name")), "Park copy");
    let (_, v) = abe(&["scene", "inspect", &f.p(), SCENE, "--entity", &avenue]);
    let names: Vec<&str> = at(&v, "entities")
        .array()
        .unwrap()
        .iter()
        .map(|e| s(at(e, "name")))
        .collect();
    assert_eq!(names, ["Avenue", "Lamp", "Bulb", "Lamp"]);
    let copied = ids(&v);
    assert!(
        copied
            .iter()
            .all(|id| ![STREET, LAMP1, LAMP2, BULB].contains(&id.as_str()))
    );
    let (_, v) = abe(&["scene", "inspect", &f.p(), SCENE, "--entity", &park2]);
    assert_eq!(at(&v, "count"), &n(2.0));
    let (status, _) = abe(&["scene", "validate", &f.p(), SCENE]);
    assert_eq!(status, 0);
}

#[test]
fn a_transaction_with_one_bad_edit_writes_nothing() {
    let f = fixture("atomic");
    let before = f.bytes();
    let good = format!(
        r#"{{"op":"set","entity":"{LAMP1}","path":"components.light.intensity","value":2}},{{"op":"create","name":"Sign"}}"#
    );
    for (bad, want, edit) in [
        (
            r#"{"op":"set","entity":"Nope","path":"name","value":"x"}"#,
            "not-found",
            2.0,
        ),
        (r#"{"op":"explode","entity":"Park"}"#, "bad-edit", 2.0),
        (
            r#"{"op":"set","entity":"Park","path":"name","valeu":"x"}"#,
            "bad-edit",
            2.0,
        ),
        (
            r#"{"op":"set","entity":"Park","path":"colour","value":1}"#,
            "bad-edit",
            2.0,
        ),
        (
            r#"{"op":"set","entity":"Lamp","path":"name","value":"x"}"#,
            "ambiguous-ref",
            2.0,
        ),
        (
            r#"{"op":"set","entity":"Bulb","path":"components.transform.translation.x","value":1}"#,
            "bad-edit",
            2.0,
        ),
        // Valid shape, but out of the schema's range: the checker refuses it.
        (
            r#"{"op":"set","entity":"Camera","path":"components.camera.fov","value":500}"#,
            "invalid",
            2.0,
        ),
    ] {
        let (status, v) = f.apply(&format!(r#"{{"edits":[{good},{bad}]}}"#), &[]);
        assert_eq!((status, code(&v)), (1, want), "{bad}: {v:?}");
        assert_eq!(at(&v, "error/edit"), &n(edit), "{bad}");
        assert_eq!(f.bytes(), before, "{bad} wrote");
        assert!(!f.dir.join(".abe").exists(), "{bad} touched .abe");
    }
    let (_, v) = f.apply(
        &format!(r#"{{"edits":[{{"op":"set","entity":"{LAMP1}","path":"components.light.intensity","value":-1}}]}}"#),
        &[],
    );
    assert_eq!(code(&v), "invalid");
    let issue = at(&v, "error/issues/0");
    assert_eq!(
        s(at(issue, "path")),
        format!("entities.{LAMP1}.components.light.intensity")
    );
    assert_eq!(
        (s(at(issue, "entity")), at(issue, "edit")),
        (LAMP1, &n(0.0))
    );
    for bad in ["not json", r#"{"edits":[]}"#, r#"{"edit":[]}"#, "[1]"] {
        let (_, v) = f.apply(bad, &[]);
        assert_eq!(code(&v), "bad-edit", "{bad}");
    }
    assert_eq!(f.bytes(), before);
}

#[test]
fn dry_run_writes_nothing_and_reports_the_same_changes() {
    let f = fixture("dry");
    let before = f.bytes();
    let edits = format!(
        r#"{{"edits":[{{"op":"set","entity":"{LAMP1}","path":"components.light.intensity","value":4}},
                       {{"op":"create","name":"Sign","parent":"Park"}},
                       {{"op":"duplicate","entity":"Street"}}]}}"#
    );
    let (status, dry) = f.apply(&edits, &["--dry-run"]);
    assert_eq!(status, 0, "{dry:?}");
    assert_eq!(
        (at(&dry, "dry_run"), at(&dry, "written")),
        (&Value::Bool(true), &Value::Bool(false))
    );
    assert_eq!(f.bytes(), before);
    assert!(!f.dir.join(".abe").exists());
    let (_, real) = f.apply(&edits, &[]);
    assert_eq!(at(&real, "written"), &Value::Bool(true));
    for key in ["changes", "created", "hash", "issues", "edits"] {
        assert_eq!(at(&real, key), at(&dry, key), "{key}");
    }
    assert_eq!(
        s(at(&real, "hash")),
        abe_core::digest::hex(&abe_core::digest::sha256(&f.bytes()))
    );
}

#[test]
fn validate_reports_schema_errors_and_dangling_references() {
    let f = fixture("validate");
    let (status, v) = abe(&["scene", "validate", &f.p(), SCENE]);
    assert_eq!(
        (status, at(&v, "errors"), at(&v, "warnings")),
        (0, &n(0.0), &n(0.0))
    );
    // Break the document behind the tools' back.
    let mut d = f.doc();
    let broken = json::parse(
        format!(r#"{{"entities":{{
          "{LAMP1}":{{"components":{{"light":{{"intensity":"bright"}},"glow":{{}}}}}},
          "{BENCH}":{{"components":{{"test.marker":{{"target":"00000000000000ff","art":"0123456789abcdef0123456789abcdef"}}}}}}}}}}"#)
        .as_bytes(),
    )
    .unwrap();
    scene::merge(&mut d, &broken);
    write_json(&f.scene_path(), &d).unwrap();
    let (status, v) = abe(&["scene", "validate", &f.p(), SCENE]);
    assert_eq!((status, code(&v)), (1, "invalid"));
    assert_eq!((at(&v, "errors"), at(&v, "warnings")), (&n(2.0), &n(2.0)));
    let issues: Vec<(&str, &str)> = at(&v, "issues")
        .array()
        .unwrap()
        .iter()
        .map(|i| (s(at(i, "severity")), s(at(i, "path"))))
        .collect();
    let bench = format!("entities.{BENCH}.components.test.marker");
    let lamp = format!("entities.{LAMP1}.components");
    assert_eq!(
        issues,
        [
            ("error", format!("{lamp}.glow").as_str()),
            ("error", format!("{lamp}.light.intensity").as_str()),
            ("warning", format!("{bench}.art").as_str()),
            ("warning", format!("{bench}.target").as_str()),
        ]
    );
    assert_eq!(s(at(&v, "issues/0/name")), "Lamp");
    // An edit is not blamed for problems the document already had.
    let (status, v) = f.apply(
        r#"{"edits":[{"op":"set","entity":"Park","path":"name","value":"Garden"}]}"#,
        &[],
    );
    assert_eq!(status, 0, "{v:?}");
    assert_eq!(at(&v, "issues").array().unwrap().len(), 2);
}

#[test]
fn apply_then_undo_restores_the_original_bytes_exactly() {
    let f = fixture("undo");
    let original = f.bytes();
    let (_, v) = f.apply(
        &format!(r#"{{"edits":[{{"op":"set","entity":"{LAMP1}","path":"components.light.intensity","value":2}}]}}"#),
        &[],
    );
    assert_eq!(at(&v, "written"), &Value::Bool(true));
    let after_first = f.bytes();
    let (_, v) = f.apply(
        r#"{"label":"Rework","edits":[{"op":"delete","entity":"Street"},{"op":"create","name":"Kiosk","parent":"Park"},{"op":"duplicate","entity":"Park"}]}"#,
        &[],
    );
    assert_eq!(at(&v, "written"), &Value::Bool(true));
    assert_eq!(f.history_lines(), 2);
    let (_, dry) = abe(&["scene", "undo", &f.p(), SCENE, "--dry-run"]);
    assert_eq!(f.history_lines(), 2, "a dry undo pops nothing");
    let (status, v) = f.undo();
    assert_eq!(status, 0, "{v:?}");
    assert_eq!(f.bytes(), after_first);
    assert_eq!(s(at(&v, "undone/label")), "Rework");
    assert_eq!(
        (at(&v, "restored"), at(&v, "remaining")),
        (&Value::Bool(true), &n(1.0))
    );
    assert_eq!(at(&v, "changes"), at(&dry, "changes"));
    let (_, v) = f.undo();
    assert_eq!(at(&v, "remaining"), &n(0.0));
    assert_eq!(f.bytes(), original);
    assert!(!f.history().exists());
    let (status, v) = f.undo();
    assert_eq!((status, code(&v)), (1, "not-found"));
}

#[test]
fn undo_gives_back_a_hand_written_file_byte_for_byte() {
    let f = fixture("by-hand");
    // Valid but not canonical: on one line, as a person or another tool might write it.
    let hand = json::to_compact(&f.doc()).unwrap() + "\n";
    std::fs::write(f.scene_path(), &hand).unwrap();
    let (_, v) = f.apply(
        r#"{"edits":[{"op":"set","entity":"Park","path":"name","value":"Garden"}]}"#,
        &[],
    );
    assert_eq!(at(&v, "written"), &Value::Bool(true));
    assert_ne!(f.bytes(), hand.as_bytes());
    let (_, v) = f.undo();
    assert_eq!(at(&v, "restored"), &Value::Bool(true));
    assert_eq!(f.bytes(), hand.as_bytes());
}

#[test]
fn undo_refuses_after_an_outside_change() {
    let f = fixture("outside");
    f.apply(
        r#"{"edits":[{"op":"set","entity":"Park","path":"name","value":"Garden"}]}"#,
        &[],
    );
    let ours = f.bytes();
    // Someone else (the studio, a person) saves the document afterwards.
    let mut d = f.doc();
    scene::merge(
        &mut d,
        &json::parse(br#"{"name":"Renamed elsewhere"}"#).unwrap(),
    );
    write_json(&f.scene_path(), &d).unwrap();
    let theirs = f.bytes();
    let (status, v) = f.undo();
    assert_eq!((status, code(&v)), (1, "changed-since"));
    assert_eq!(f.bytes(), theirs, "a refused undo writes nothing");
    assert_eq!(f.history_lines(), 1, "and keeps the transaction");
    // Once the document is back as the transaction left it, undo goes ahead.
    std::fs::write(f.scene_path(), &ours).unwrap();
    let (status, _) = f.undo();
    assert_eq!(status, 0);
}

#[test]
fn diff_reports_each_kind_of_change_in_order() {
    let f = fixture("diff");
    let before = f.dir.join("before.scene.json");
    std::fs::write(&before, f.bytes()).unwrap();
    let (status, v) = f.apply(
        &format!(
            r#"{{"edits":[
              {{"op":"reparent","entity":"Bulb","parent":"Park"}},
              {{"op":"set","entity":"Park","path":"name","value":"Garden"}},
              {{"op":"delete","entity":"{LAMP2}"}},
              {{"op":"create","name":"Sign","id":"00000000000000d1"}},
              {{"op":"set","entity":"{LAMP1}","path":"components.light.intensity","value":5}}]}}"#
        ),
        &[],
    );
    assert_eq!(status, 0, "{v:?}");
    let a = before.display().to_string();
    let b = f.scene_path().display().to_string();
    let (status, v) = abe(&["scene", "diff", &a, &b]);
    assert_eq!(status, 0);
    assert_eq!(at(&v, "equal"), &Value::Bool(false));
    assert_eq!(s(at(&v, "added/0/id")), "00000000000000d1");
    assert_eq!(s(at(&v, "removed/0/id")), LAMP2);
    assert_eq!(
        (s(at(&v, "renamed/0/before")), s(at(&v, "renamed/0/after"))),
        ("Park", "Garden")
    );
    assert_eq!(
        (
            s(at(&v, "reparented/0/id")),
            s(at(&v, "reparented/0/before")),
            s(at(&v, "reparented/0/after"))
        ),
        (BULB, LAMP1, PARK)
    );
    let changed = at(&v, "changed").array().unwrap();
    assert_eq!(changed.len(), 1);
    assert_eq!(s(at(&changed[0], "path")), "components.light.intensity");
    assert_eq!(
        (at(&changed[0], "before"), at(&changed[0], "after")),
        (&n(1.0), &n(5.0))
    );
    for list in ["added", "removed", "renamed", "reparented"] {
        assert_eq!(at(&v, list).array().unwrap().len(), 1, "{list}");
    }
    // One side from standard input; the same documents always give the same bytes.
    let text = String::from_utf8(f.bytes()).unwrap();
    let args: Vec<String> = ["scene", "diff", &a, "-"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let x = abe_agent::run(&args, &mut text.as_bytes(), None);
    let y = abe_agent::run(&args, &mut text.as_bytes(), None);
    assert_eq!(x, y);
    assert_eq!(
        at(&json::parse(x.stdout.as_bytes()).unwrap(), "changed"),
        at(&v, "changed")
    );
    let (_, same) = abe(&["scene", "diff", &a, &a]);
    assert_eq!(at(&same, "equal"), &Value::Bool(true));
}

#[test]
fn answers_are_deterministic() {
    let f = fixture("same");
    let args: Vec<String> = ["scene", "inspect", &f.p(), SCENE, "--defaults"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let first = abe_agent::run(&args, &mut std::io::empty(), None);
    assert_eq!(first, abe_agent::run(&args, &mut std::io::empty(), None));
    // --pretty is the same answer, indented.
    let mut pretty = args.clone();
    pretty.push("--pretty".into());
    let p = abe_agent::run(&pretty, &mut std::io::empty(), None);
    assert!(p.stdout.contains("\n  \"ok\": true"));
    assert_eq!(
        json::parse(p.stdout.as_bytes()).unwrap(),
        json::parse(first.stdout.as_bytes()).unwrap()
    );
}

/// The same edit through ABE Studio's edit protocol and through `abe scene apply` writes the
/// same file.
#[test]
fn edits_write_the_same_bytes_as_abe_studio() {
    let cli = fixture("cli-side");
    let studio_side = fixture("studio-side");
    assert_eq!(cli.bytes(), studio_side.bytes());
    let (status, v) = cli.apply(
        &format!(
            r#"{{"edits":[
              {{"op":"set","entity":"{LAMP1}","path":"components.light.intensity","value":2.5}},
              {{"op":"create","name":"Sign","parent":"Park","id":"00000000000000d1","components":{{"transform":{{}}}}}},
              {{"op":"reparent","entity":"Bulb","parent":"Park"}},
              {{"op":"delete","entity":"Street"}},
              {{"op":"remove","entity":"Camera","path":"components.camera"}}]}}"#
        ),
        &[],
    );
    assert_eq!(status, 0, "{v:?}");

    let (mut st, _) = abe_studio::session::Studio::open(&studio_side.dir, Ids::seeded(1)).unwrap();
    let client = st.connect("test");
    let doc = &studio_side.scene_id;
    st.handle(
        client,
        &json::parse(format!(r#"{{"type":"open","doc":"{doc}"}}"#).as_bytes()).unwrap(),
    );
    let ops = format!(
        r#"[{{"set":["entities","{LAMP1}","components","light","intensity"],"value":2.5}},
            {{"set":["entities","00000000000000d1"],"value":{{"name":"Sign","parent":"{PARK}","components":{{"transform":{{}}}}}}}},
            {{"set":["entities","{BULB}","parent"],"value":"{PARK}"}},{{"remove":["entities","{BULB}","order"]}},
            {{"remove":["entities","{STREET}"]}},{{"remove":["entities","{LAMP1}"]}},{{"remove":["entities","{LAMP2}"]}},
            {{"remove":["entities","{CAMERA}","components","camera"]}}]"#
    );
    let edit = format!(r#"{{"type":"edit","doc":"{doc}","rev":0,"label":"Same","ops":{ops}}}"#);
    let out = st.handle(client, &json::parse(edit.as_bytes()).unwrap());
    assert!(
        out.iter()
            .all(|(_, m)| m.get("type") != Some(&Value::String("error".into()))),
        "{out:?}"
    );
    st.handle(
        client,
        &json::parse(format!(r#"{{"type":"save","doc":"{doc}"}}"#).as_bytes()).unwrap(),
    );
    assert_eq!(
        String::from_utf8(cli.bytes()).unwrap(),
        String::from_utf8(studio_side.bytes()).unwrap()
    );
    // The history records the ops in the protocol's own form.
    let line = std::fs::read_to_string(cli.history()).unwrap();
    let record = json::parse(line.trim().as_bytes()).unwrap();
    for op in at(&record, "ops")
        .array()
        .unwrap()
        .iter()
        .chain(at(&record, "undo").array().unwrap())
    {
        abe_studio::session::op_from_json(op).unwrap();
    }
}

/// What a backend was asked to draw: camera, hour, weather, (width, height, samples), and
/// whether the world held the scene's 8 entities.
type Call = (
    Option<Shot>,
    Option<f32>,
    Option<String>,
    (u32, u32, u32),
    bool,
);

struct Fake(RefCell<Vec<Call>>);

impl Backend for Fake {
    fn render(&self, s: &Still<'_>) -> Result<Rendered, String> {
        let (_, table) = abe_assets::cooked::decode_scene(s.world)?;
        self.0.borrow_mut().push((
            s.shot.cloned(),
            s.time,
            s.weather.map(String::from),
            (s.width, s.height, s.samples),
            table.entities.len() == 8,
        ));
        let view = s.shot.cloned().unwrap_or(Shot {
            name: "overview".into(),
            eye: [0.0; 3],
            target: [0.0, 0.0, -1.0],
            up: [0.0, 1.0, 0.0],
            fov: 45.0,
            near: 0.08,
            shift: [0.0; 2],
            focus: None,
        });
        Ok(Rendered {
            png: b"\x89PNG\r\n\x1a\nnot really".to_vec(),
            device: "fake".into(),
            source: if s.shot.is_some() {
                "camera"
            } else {
                "overview"
            }
            .into(),
            view,
            time: s.time.unwrap_or(21.0),
        })
    }
}

#[test]
fn render_hands_the_backend_the_cooked_scene_camera_and_environment() {
    let f = fixture("render");
    f.apply(
        r#"{"edits":[{"op":"create","name":"Sky","components":{"environment":{"time":18,"weather":"rain"}}}]}"#,
        &[],
    );
    let out = f.dir.join("out/still.png").display().to_string();
    let fake = Fake(RefCell::new(vec![]));
    let base: Vec<String> = ["render", &f.p(), SCENE, "--out", &out]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut args = base.clone();
    args.extend(["--size", "64x32", "--samples", "1"].map(String::from));
    let answer = abe_agent::run(&args, &mut std::io::empty(), Some(&fake));
    assert_eq!(answer.status, 0, "{}", answer.stdout);
    let v = json::parse(answer.stdout.as_bytes()).unwrap();
    let png = std::fs::read(&out).unwrap();
    assert_eq!(at(&v, "bytes"), &n(png.len() as f64));
    assert_eq!(
        s(at(&v, "sha256")),
        abe_core::digest::hex(&abe_core::digest::sha256(&png))
    );
    assert_eq!(s(at(&v, "view/entity")), CAMERA);
    assert_eq!((at(&v, "time"), s(at(&v, "weather"))), (&n(18.0), "rain"));
    assert_eq!(at(&v, "warnings").array().unwrap().len(), 0);
    let calls = fake.0.borrow();
    let (shot, time, weather, size, table) = &calls[0];
    let shot = shot.as_ref().unwrap();
    assert_eq!(
        (shot.name.as_str(), shot.eye, shot.fov),
        ("Camera", [0.0, 2.0, 8.0], 40.0)
    );
    assert_eq!(shot.up, [0.0, 1.0, 0.0]);
    assert!(shot.target[2] < shot.eye[2], "a camera looks along -Z");
    assert_eq!(
        (*time, weather.as_deref(), *size, *table),
        (Some(18.0), Some("rain"), (64, 32, 1), true)
    );
    drop(calls);
    // Flags override the environment; refused values are usage errors.
    let mut more = args.clone();
    more.extend(["--time", "6.5", "--weather", "mist"].map(String::from));
    assert_eq!(
        abe_agent::run(&more, &mut std::io::empty(), Some(&fake)).status,
        0
    );
    assert_eq!(fake.0.borrow()[1].1, Some(6.5));
    let json_out = ["render", &f.p(), SCENE, "--out", "still.json"].map(String::from);
    let a = abe_agent::run(&json_out, &mut std::io::empty(), Some(&fake));
    assert_eq!(a.status, 2, "{}", a.stdout);
    assert!(a.stdout.contains(".png file"), "{}", a.stdout);
    for (flag, value) in [
        ("--weather", "snow"),
        ("--size", "0x10"),
        ("--time", "25"),
        ("--samples", "0"),
    ] {
        let mut bad = base.clone();
        bad.extend([flag, value].map(String::from));
        let a = abe_agent::run(&bad, &mut std::io::empty(), Some(&fake));
        assert_eq!(a.status, 2, "{flag} {value}: {}", a.stdout);
    }
    // A Look slot that names an asset of another kind draws with the default Look, warned.
    f.apply(
        &format!(
            r#"{{"edits":[{{"op":"set","entity":"Sky","path":"components.environment.look","value":"{}"}}]}}"#,
            f.scene_id
        ),
        &[],
    );
    let a = abe_agent::run(&args, &mut std::io::empty(), Some(&fake));
    let v = json::parse(a.stdout.as_bytes()).unwrap();
    assert_eq!(a.status, 0, "{}", a.stdout);
    assert!(
        s(at(&v, "warnings/0")).contains("is a scene"),
        "{}",
        a.stdout
    );
    assert_eq!(at(&v, "look"), &Value::Null);
    let mut not_a_camera = args.clone();
    not_a_camera.extend(["--camera", "Bench"].map(String::from));
    let a = abe_agent::run(&not_a_camera, &mut std::io::empty(), Some(&fake));
    assert_eq!(code(&json::parse(a.stdout.as_bytes()).unwrap()), "invalid");
    // Without a renderer (the library alone) rendering is refused, not attempted.
    let a = abe_agent::run(&args, &mut std::io::empty(), None);
    assert_eq!(code(&json::parse(a.stdout.as_bytes()).unwrap()), "render");
}

/// Writers racing on one document: each transaction either lands, recorded in order, or is
/// refused as `changed-since`; undoing what landed gives back the original bytes.
#[test]
fn concurrent_writers_never_lose_a_transaction() {
    let f = fixture("race");
    let original = f.bytes();
    let p = f.p();
    let results: Vec<(i32, Value)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let p = p.clone();
                scope.spawn(move || {
                    let edits = format!(
                        r#"{{"edits":[{{"op":"set","entity":"{LAMP1}","path":"components.light.intensity","value":{i}}},{{"op":"create","name":"Mark {i}"}}]}}"#
                    );
                    abe_in(&["scene", "apply", &p, SCENE, "-"], &edits)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let landed = results.iter().filter(|(status, _)| *status == 0).count();
    assert!(landed >= 1);
    for (status, v) in &results {
        if *status != 0 {
            assert_eq!(code(v), "changed-since", "{v:?}");
        }
    }
    assert_eq!(f.history_lines(), landed);
    let marks = scene::entities(&f.doc())
        .values()
        .filter(|e| {
            e.get("name")
                .and_then(|n| n.string().ok())
                .is_some_and(|n| n.starts_with("Mark"))
        })
        .count();
    assert_eq!(marks, landed);
    for _ in 0..landed {
        let (status, v) = f.undo();
        assert_eq!(status, 0, "{v:?}");
    }
    assert_eq!(f.bytes(), original);
}

#[test]
fn scenes_are_found_by_path_or_id_in_the_content_folders() {
    let f = fixture("locate");
    for scene in ["./content/street.scene.json", "content\\street.scene.json"] {
        let (status, v) = abe(&["scene", "inspect", &f.p(), scene, "--depth", "0"]);
        assert_eq!(status, 0, "{scene}: {v:?}");
    }
    // Outside the content folders a file is not a project document.
    std::fs::copy(f.scene_path(), f.dir.join("schemas/stray.scene.json")).unwrap();
    let (_, v) = abe(&["scene", "inspect", &f.p(), "schemas/stray.scene.json"]);
    assert_eq!(code(&v), "not-found");
    // A Look file is an asset, not a scene.
    std::fs::write(f.dir.join("content/day.look.json"), "{}").unwrap();
    let (_, v) = abe(&["scene", "inspect", &f.p(), "content/day.look.json"]);
    assert_eq!(code(&v), "invalid");
    // A copied scene keeps its id: the index (which validate needs) refuses the project.
    std::fs::copy(f.scene_path(), f.dir.join("content/copy.scene.json")).unwrap();
    let (_, v) = abe(&["scene", "validate", &f.p(), SCENE]);
    assert_eq!(code(&v), "invalid");
    assert!(s(at(&v, "error/message")).contains("same id"), "{v:?}");
    let (_, v) = abe(&["scene", "inspect", &f.p(), &f.scene_id]);
    assert_eq!(code(&v), "invalid");
}
