//! `abe-build import` and `cook-scene` on a project: new files are written, and documents and
//! assets that already exist are left alone unless `--force` asks to replace them.
use std::path::{Path, PathBuf};
use std::process::Command;

use abe_engine::id::Ids;
use abe_pipeline::project::{Kind, Project};

/// A fresh folder under the system temp directory, removed on drop.
struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Runs abe-build; returns whether it succeeded and what it printed.
fn abe_build(args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_abe-build"))
        .args(args)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout) + String::from_utf8_lossy(&out.stderr);
    (out.status.success(), text.into_owned())
}

/// A GLB with one node holding one triangle.
fn glb() -> Vec<u8> {
    let doc = br#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":44}],"bufferViews":[{"buffer":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":6}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"},{"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}],"meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":1}]}],"nodes":[{"name":"Block","mesh":0}],"scenes":[{"nodes":[0]}],"scene":0}"#;
    let mut json = doc.to_vec();
    json.resize(json.len().next_multiple_of(4), b' ');
    let mut bin = vec![];
    for f in [0f32, 0., 0., 1., 0., 0., 0., 1., 0.] {
        bin.extend_from_slice(&f.to_le_bytes());
    }
    for i in [0u16, 1, 2, 0] {
        bin.extend_from_slice(&i.to_le_bytes());
    }
    let mut out = b"glTF".to_vec();
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(28 + json.len() as u32 + bin.len() as u32).to_le_bytes());
    for (kind, chunk) in [(0x4e4f534au32, json), (0x004e4942, bin)] {
        out.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
        out.extend_from_slice(&kind.to_le_bytes());
        out.extend_from_slice(&chunk);
    }
    out
}

fn scene_ids(root: &Path) -> Vec<(String, String)> {
    let mut p = Project::open(root).unwrap();
    p.scan(None).unwrap();
    p.entries()
        .filter(|e| e.kind == Kind::Scene)
        .map(|e| (e.path.clone(), e.id.clone()))
        .collect()
}

#[test]
fn import_and_cook_scene_replace_project_files_only_when_forced() {
    let t = Temp(std::env::temp_dir().join(format!("abe-build-cli-{}", Ids::from_os().entity())));
    let root = &t.0;
    Project::create(root, "CLI", &mut Ids::seeded(7)).unwrap();
    std::fs::create_dir_all(root.join("content/models")).unwrap();
    std::fs::write(root.join("content/models/block.glb"), glb()).unwrap();
    let p = root.to_str().unwrap();
    let path = |rel: &str| root.join(rel).to_str().unwrap().to_string();
    let read = |rel: &str| std::fs::read(root.join(rel)).unwrap();
    let ok = |args: &[&str]| {
        let (ok, out) = abe_build(args);
        assert!(ok, "{args:?} failed: {out}");
    };
    let refused = |args: &[&str], naming: &str| {
        let (ok, out) = abe_build(args);
        assert!(!ok, "{args:?} succeeded: {out}");
        assert!(out.contains(naming), "{args:?}: {out}");
        out
    };
    let (model, scene) = (
        "content/models/block.glb",
        "content/scenes/block.scene.json",
    );

    ok(&["import", p, model, scene]);
    let before = read(scene);
    let first = scene_ids(root);
    assert_eq!(first.len(), 1);
    // A second import does not replace the scene, nor the model, nor write outside content.
    let out = refused(&["import", p, model, scene], scene);
    assert!(out.contains("--force"), "{out}");
    refused(&["import", p, model, model], model);
    refused(
        &["import", p, model, "schemas/block.scene.json"],
        "schemas/block.scene.json",
    );
    assert_eq!(read(scene), before);
    assert_eq!(read(model), glb());
    assert!(!root.join("schemas/block.scene.json").exists());

    // The cooked scene goes anywhere but over the project's own files.
    for dest in [scene, model, "abe.project.json"] {
        let out = refused(&["cook-scene", p, scene, &path(dest)], &path(dest));
        assert!(out.contains("--force"), "{out}");
    }
    assert_eq!(read(scene), before);
    for _ in 0..2 {
        ok(&["cook-scene", p, scene, &path("build/web/block.abew")]);
    }
    assert!(read("build/web/block.abew").starts_with(b"ABEW"));

    // Asked to, import replaces the scene: one scene at that path, under its new id.
    ok(&["import", "--force", p, model, scene]);
    let second = scene_ids(root);
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].0, scene);
    assert_ne!(second[0].1, first[0].1);
    // And cook-scene writes over a project file.
    std::fs::write(root.join("content/old.abew"), b"old").unwrap();
    ok(&["cook-scene", "--force", p, scene, &path("content/old.abew")]);
    assert!(read("content/old.abew").starts_with(b"ABEW"));
}
