//! The `ganbate` binary itself: one line of JSON on stdout, stable exit codes, edits on standard
//! input, and (with a Vulkan driver; run with `--ignored`) a real render of a synthetic scene.
use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use ganbate_engine::id::Ids;
use ganbate_engine::json::{self, Value};
use ganbate_pipeline::project::{Kind, Project, new_sidecar, write_json};
use ganbate_scene::doc::Document;
use ganbate_scene::scene;

const GANBATE: &str = env!("CARGO_BIN_EXE_ganbate");
const SCENE: &str = "content/cube.scene.json";

struct Temp(PathBuf);

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A GLB with a 20 m ground plate and a unit cube standing on it.
fn glb() -> Vec<u8> {
    let (mut pos, mut nor, mut idx) = (vec![], vec![], vec![]);
    let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([1., 0., 0.], [0., 1., 0.], [0., 0., 1.]),
        ([-1., 0., 0.], [0., 0., 1.], [0., 1., 0.]),
        ([0., 1., 0.], [0., 0., 1.], [1., 0., 0.]),
        ([0., -1., 0.], [1., 0., 0.], [0., 0., 1.]),
        ([0., 0., 1.], [1., 0., 0.], [0., 1., 0.]),
        ([0., 0., -1.], [0., 1., 0.], [1., 0., 0.]),
    ];
    for (n, u, v) in faces {
        let base = (pos.len() / 3) as u16;
        for (a, b) in [(-1., -1.), (1., -1.), (1., 1.), (-1., 1.)] {
            for k in 0..3 {
                pos.push(0.5 * (n[k] + a * u[k] + b * v[k]));
            }
            nor.extend_from_slice(&n);
        }
        idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    let ground_pos: [f32; 12] = [-10., 0., -10., 10., 0., -10., 10., 0., 10., -10., 0., 10.];
    let ground_nor: [f32; 12] = [0., 1., 0., 0., 1., 0., 0., 1., 0., 0., 1., 0.];
    let ground_idx: [u16; 6] = [0, 2, 1, 0, 3, 2];
    let mut bin: Vec<u8> = vec![];
    let mut views = vec![];
    let mut view = |bytes: Vec<u8>| {
        let start = bin.len();
        bin.extend_from_slice(&bytes);
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
        views.push(format!(
            r#"{{"buffer":0,"byteOffset":{start},"byteLength":{}}}"#,
            bytes.len()
        ));
    };
    let f32s = |v: &[f32]| v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
    let u16s = |v: &[u16]| v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
    view(f32s(&pos));
    view(f32s(&nor));
    view(u16s(&idx));
    view(f32s(&ground_pos));
    view(f32s(&ground_nor));
    view(u16s(&ground_idx));
    let doc = format!(
        r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{len}}}],"bufferViews":[{views}],
"accessors":[
 {{"bufferView":0,"componentType":5126,"count":24,"type":"VEC3","min":[-0.5,-0.5,-0.5],"max":[0.5,0.5,0.5]}},
 {{"bufferView":1,"componentType":5126,"count":24,"type":"VEC3"}},
 {{"bufferView":2,"componentType":5123,"count":36,"type":"SCALAR"}},
 {{"bufferView":3,"componentType":5126,"count":4,"type":"VEC3","min":[-10,0,-10],"max":[10,0,10]}},
 {{"bufferView":4,"componentType":5126,"count":4,"type":"VEC3"}},
 {{"bufferView":5,"componentType":5123,"count":6,"type":"SCALAR"}}],
"materials":[{{"name":"ground","pbrMetallicRoughness":{{"baseColorFactor":[0.3,0.45,0.25,1]}}}},
 {{"name":"cube","pbrMetallicRoughness":{{"baseColorFactor":[0.8,0.2,0.1,1]}}}}],
"meshes":[{{"primitives":[{{"attributes":{{"POSITION":3,"NORMAL":4}},"indices":5,"material":0}}]}},
 {{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1}},"indices":2,"material":1}}]}}],
"nodes":[{{"name":"Ground","mesh":0}},{{"name":"Cube","mesh":1,"translation":[0,0.5,0]}}],
"scenes":[{{"nodes":[0,1]}}],"scene":0}}"#,
        len = bin.len(),
        views = views.join(","),
    );
    let mut j = doc.into_bytes();
    while j.len() % 4 != 0 {
        j.push(b' ');
    }
    let mut out = b"glTF".to_vec();
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(28 + j.len() as u32 + bin.len() as u32).to_le_bytes());
    for (id, chunk) in [(0x4e4f534au32, j), (0x004e4942, bin)] {
        out.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&chunk);
    }
    out
}

/// A project with the model imported as a scene, a camera looking at the cube and an
/// environment.
fn project(tag: &str) -> Temp {
    let dir = std::env::temp_dir().join(format!("ganbate-cli-{tag}-{}", Ids::from_os().entity()));
    std::fs::create_dir_all(dir.join("content/models")).unwrap();
    let dir = Temp(dir);
    let mut ids = Ids::seeded(7);
    let mut p = Project::create(&dir.0, "CLI test", &mut ids).unwrap();
    let bytes = glb();
    std::fs::write(dir.0.join("content/models/cube.glb"), &bytes).unwrap();
    let model = ids.asset();
    write_json(
        &dir.0.join("content/models/cube.glb.meta.json"),
        &new_sidecar(&model, Kind::Model),
    )
    .unwrap();
    let nodes = ganbate_engine::assets::glb::import_nodes(&bytes, false).unwrap();
    let scene_id = ids.asset();
    let mut doc = Document::new(ganbate_pipeline::cook::scene_from_model(
        &nodes, &model, &scene_id, "Cube", &mut ids,
    ))
    .unwrap();
    let comp = |k: &str, v: &str| (k.to_string(), json::parse(v.as_bytes()).unwrap());
    doc.apply_all(&scene::create(
        &ids.entity(),
        "Camera",
        None,
        None,
        BTreeMap::from([
            comp("camera", r#"{"fov":45}"#),
            // 1.6 m up and 5 m back, pitched 10 degrees down.
            comp(
                "transform",
                r#"{"translation":[0,1.6,5],"rotation":[-0.0871557,0,0,0.9961947]}"#,
            ),
        ]),
    ))
    .unwrap();
    doc.apply_all(&scene::create(
        &ids.entity(),
        "Environment",
        None,
        None,
        BTreeMap::from([comp("environment", r#"{"time":15,"weather":"clear"}"#)]),
    ))
    .unwrap();
    p.write_document(SCENE, Kind::Scene, doc.root()).unwrap();
    dir
}

/// Runs the binary; returns its exit code, stdout and the parsed answer.
fn ganbate(args: &[&str], stdin: &str) -> (i32, String, Value) {
    let mut child = Command::new(GANBATE)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("VK_ICD_FILENAMES", lavapipe())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    if !args.contains(&"--pretty") {
        assert_eq!(text.matches('\n').count(), 1, "one line: {text}");
    }
    assert!(text.ends_with('\n'));
    let v = json::parse(text.trim_end().as_bytes()).unwrap_or_else(|e| panic!("{e}: {text}"));
    (out.status.code().unwrap_or(-1), text, v)
}

/// Mesa's software Vulkan driver when it is installed (the tests do not need a GPU), else
/// whatever the system loader finds.
fn lavapipe() -> String {
    let icd = "/usr/share/vulkan/icd.d/lvp_icd.json";
    match std::env::var("VK_ICD_FILENAMES") {
        Ok(v) if !v.is_empty() => v,
        _ if std::path::Path::new(icd).exists() => icd.into(),
        _ => String::new(),
    }
}

fn at<'a>(v: &'a Value, path: &str) -> &'a Value {
    path.split('/').fold(v, |cur, k| match cur {
        Value::Array(a) => &a[k.parse::<usize>().unwrap()],
        _ => cur
            .get(k)
            .unwrap_or_else(|| panic!("no {k} in {path}: {cur:?}")),
    })
}

#[test]
fn the_binary_answers_in_one_line_with_stable_exit_codes() {
    let dir = project("lines");
    let p = dir.0.display().to_string();
    let (status, _, v) = ganbate(&["help"], "");
    assert_eq!((status, at(&v, "ok")), (0, &Value::Bool(true)));
    let (status, _, v) = ganbate(&[], "");
    assert_eq!(
        (status, at(&v, "error/code")),
        (2, &Value::String("usage".into()))
    );
    let (status, _, v) = ganbate(&["scene", "inspect", &p, "content/missing.scene.json"], "");
    assert_eq!(
        (status, at(&v, "error/code")),
        (1, &Value::String("not-found".into()))
    );
    let (status, text, v) = ganbate(&["scene", "inspect", &p, SCENE, "--entity", "Cube"], "");
    assert_eq!(status, 0);
    assert!(text.starts_with(r#"{"ok":true,"scene":{"#), "{text}");
    assert_eq!(at(&v, "entities/0/name"), &Value::String("Cube".into()));
    let (_, again, _) = ganbate(&["scene", "inspect", &p, SCENE, "--entity", "Cube"], "");
    assert_eq!(text, again);
    let (status, pretty, _) = ganbate(&["scene", "validate", &p, SCENE, "--pretty"], "");
    assert_eq!(status, 0);
    assert!(pretty.starts_with("{\n  \"ok\": true,\n"), "{pretty}");
}

#[test]
fn the_binary_applies_edits_from_standard_input_and_undoes_them() {
    let dir = project("stdin");
    let p = dir.0.display().to_string();
    let file = dir.0.join(SCENE);
    let original = std::fs::read(&file).unwrap();
    let edits = r#"{"edits":[{"op":"set","entity":"Cube","path":"components.transform.translation","value":[1,0.5,0]},
                             {"op":"duplicate","entity":"Cube","name":"Cube 2"}]}"#;
    let (status, _, v) = ganbate(&["scene", "apply", &p, SCENE, "-"], edits);
    assert_eq!(status, 0, "{v:?}");
    assert_eq!(at(&v, "changes").array().unwrap().len(), 2);
    let (status, _, v) = ganbate(&["scene", "inspect", &p, SCENE, "--entity", "Cube 2"], "");
    assert_eq!(status, 0);
    assert_eq!(
        at(&v, "entities/0/components/transform/translation"),
        &json::parse(b"[1, 0.5, 0]").unwrap()
    );
    let (status, _, v) = ganbate(&["scene", "apply", &p, SCENE, "-"], "{oops");
    assert_eq!(
        (status, at(&v, "error/code")),
        (1, &Value::String("bad-edit".into()))
    );
    let (status, _, _) = ganbate(&["scene", "undo", &p, SCENE], "");
    assert_eq!(status, 0);
    assert_eq!(std::fs::read(&file).unwrap(), original);
}

#[test]
#[ignore = "requires a Vulkan driver"]
fn the_binary_renders_a_png() {
    let dir = project("render");
    let p = dir.0.display().to_string();
    let out = dir.0.join("renders/still.png");
    let out_s = out.display().to_string();
    let args = [
        "render",
        &p,
        SCENE,
        "--out",
        &out_s,
        "--size",
        "160x90",
        "--samples",
        "1",
    ];
    let (status, _, v) = ganbate(&args, "");
    assert_eq!(status, 0, "{v:?}");
    let png = std::fs::read(&out).unwrap();
    assert_eq!(at(&v, "bytes"), &Value::Number(png.len() as f64));
    assert_eq!(at(&v, "view/source"), &Value::String("camera".into()));
    assert_eq!(at(&v, "time"), &Value::Number(15.0));
    let image = ganbate_engine::assets::png::decode(&png).unwrap();
    assert_eq!((image.width, image.height), (160, 90));
    // The red cube is in the middle of the frame, on the green ground, under the sky.
    let px = |x: u32, y: u32| {
        let i = 4 * (y * image.width + x) as usize;
        [image.rgba[i], image.rgba[i + 1], image.rgba[i + 2]]
    };
    let centre = px(80, 52);
    assert!(
        centre[0] > centre[1] + 30 && centre[0] > centre[2] + 30,
        "the cube is not red at the centre: {centre:?}"
    );
    let ground = px(80, 86);
    assert!(
        ground[1] >= ground[0] && ground[1] >= ground[2],
        "ground {ground:?}"
    );
    // The same scene and settings give the same image.
    let (_, _, again) = ganbate(&args, "");
    assert_eq!(at(&again, "sha256"), at(&v, "sha256"));
    eprintln!("rendered on {:?}", at(&v, "device"));
}
