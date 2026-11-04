//! Writes a small synthetic project for Ganbate Studio: a model of a few named blocks, a tower with
//! a child, a lamp with its light and a ground plate (`content/models/blocks.glb`, built here),
//! a Look file, and a scene imported from the model with an environment and a camera added.
//!
//! cargo run --example studio_sample -- DIR
//! cargo run -p ganbate-studio -- DIR      # then open http://localhost:7480/
use std::collections::BTreeMap;
use std::path::Path;

use ganbate_engine::assets::glb;
use ganbate_engine::id::Ids;
use ganbate_engine::json;
use ganbate_pipeline::cook::scene_from_model;
use ganbate_pipeline::project::{Kind, Project, new_sidecar, write_json};
use ganbate_scene::doc::Document;
use ganbate_scene::scene;

const LOOK: &str = r#"{
  "about": "Synthetic studio sample.",
  "time": { "hours": 16.5 },
  "weather": { "preset": "clear" },
  "views": [{ "name": "overview", "eye": [10, 7, 12], "target": [0, 0.5, 0], "fov": 50 }]
}
"#;

/// A unit cube (24 vertices with face normals) and a 30 m ground plate, in one buffer.
fn geometry() -> (Vec<u8>, Vec<String>) {
    let mut bin = Vec::new();
    let f32s = |v: &[f32], bin: &mut Vec<u8>| {
        for x in v {
            bin.extend_from_slice(&x.to_le_bytes());
        }
    };
    let mut pos = vec![];
    let mut nor = vec![];
    let mut idx: Vec<u16> = vec![];
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
    let ground_pos = [-15., 0., -15., 15., 0., -15., 15., 0., 15., -15., 0., 15.];
    let ground_nor = [0., 1., 0., 0., 1., 0., 0., 1., 0., 0., 1., 0.];
    let ground_idx: [u16; 6] = [0, 2, 1, 0, 3, 2];
    // Views: cube positions, normals, indices; ground positions, normals, indices.
    let mut views = vec![];
    let mut view = |bin: &mut Vec<u8>, write: &dyn Fn(&mut Vec<u8>)| {
        let start = bin.len();
        write(bin);
        let len = bin.len() - start;
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
        views.push(format!(
            r#"{{"buffer":0,"byteOffset":{start},"byteLength":{len}}}"#
        ));
    };
    let (p, n) = (pos.clone(), nor.clone());
    view(&mut bin, &|b| f32s(&p, b));
    view(&mut bin, &|b| f32s(&n, b));
    let i = idx.clone();
    view(&mut bin, &|b| {
        for x in &i {
            b.extend_from_slice(&x.to_le_bytes());
        }
    });
    view(&mut bin, &|b| f32s(&ground_pos, b));
    view(&mut bin, &|b| f32s(&ground_nor, b));
    view(&mut bin, &|b| {
        for x in &ground_idx {
            b.extend_from_slice(&x.to_le_bytes());
        }
    });
    (bin, views)
}

fn glb_bytes() -> Vec<u8> {
    let (bin, views) = geometry();
    let materials = [
        ("ground", [0.32, 0.42, 0.28, 1.0], [0.0; 3]),
        ("red", [0.75, 0.16, 0.12, 1.0], [0.0; 3]),
        ("blue", [0.12, 0.25, 0.7, 1.0], [0.0; 3]),
        ("green", [0.2, 0.55, 0.25, 1.0], [0.0; 3]),
        ("lamp", [1.0, 0.85, 0.6, 1.0], [2.0, 1.6, 0.9]),
    ];
    let mats: Vec<String> = materials
        .iter()
        .map(|(name, c, e)| {
            format!(
                r#"{{"name":"{name}","pbrMetallicRoughness":{{"baseColorFactor":[{},{},{},{}],"metallicFactor":0,"roughnessFactor":0.8}},"emissiveFactor":[{},{},{}]}}"#,
                c[0], c[1], c[2], c[3], e[0], e[1], e[2]
            )
        })
        .collect();
    let cube = |m: usize| {
        format!(
            r#"{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1}},"indices":2,"material":{m}}}]}}"#
        )
    };
    let meshes = [
        r#"{"name":"ground","primitives":[{"attributes":{"POSITION":3,"NORMAL":4},"indices":5,"material":0}]}"#
            .to_string(),
        cube(1),
        cube(2),
        cube(3),
        cube(4),
    ];
    let doc = format!(
        r#"{{"asset":{{"version":"2.0","generator":"ganbate studio_sample"}},
"extensionsUsed":["KHR_lights_punctual"],
"extensions":{{"KHR_lights_punctual":{{"lights":[{{"type":"point","color":[1,0.8,0.55],"intensity":18,"range":9}}]}}}},
"buffers":[{{"byteLength":{len}}}],
"bufferViews":[{views}],
"accessors":[
 {{"bufferView":0,"componentType":5126,"count":24,"type":"VEC3","min":[-0.5,-0.5,-0.5],"max":[0.5,0.5,0.5]}},
 {{"bufferView":1,"componentType":5126,"count":24,"type":"VEC3"}},
 {{"bufferView":2,"componentType":5123,"count":36,"type":"SCALAR"}},
 {{"bufferView":3,"componentType":5126,"count":4,"type":"VEC3","min":[-15,0,-15],"max":[15,0,15]}},
 {{"bufferView":4,"componentType":5126,"count":4,"type":"VEC3"}},
 {{"bufferView":5,"componentType":5123,"count":6,"type":"SCALAR"}}],
"materials":[{mats}],
"meshes":[{meshes}],
"nodes":[
 {{"name":"Ground","mesh":0}},
 {{"name":"Block A","mesh":1,"translation":[-3,0.5,0]}},
 {{"name":"Block B","mesh":2,"translation":[0,0.5,2.5],"rotation":[0,0.3826834,0,0.9238795]}},
 {{"name":"Tower","mesh":3,"translation":[3,1,-1],"scale":[1,2,1],"children":[5]}},
 {{"name":"Lamp","mesh":4,"translation":[0,3,0],"scale":[0.3,0.3,0.3],"extensions":{{"KHR_lights_punctual":{{"light":0}}}}}},
 {{"name":"Tower top","mesh":1,"translation":[0,0.75,0],"scale":[0.5,0.25,0.5]}}],
"scenes":[{{"nodes":[0,1,2,3,4]}}],"scene":0}}"#,
        len = bin.len(),
        views = views.join(","),
        mats = mats.join(","),
        meshes = meshes.join(","),
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

fn main() -> Result<(), String> {
    let dir = std::env::args()
        .nth(1)
        .ok_or("usage: studio_sample DIR (a new or empty folder)")?;
    let root = Path::new(&dir);
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    // Fixed ids: the sample is the same every time it is written.
    let mut ids = Ids::seeded(0x5747_5544_494f);
    let mut project = Project::create(root, "Studio sample", &mut ids)?;
    let model_rel = "content/models/blocks.glb";
    let glb = glb_bytes();
    std::fs::create_dir_all(root.join("content/models")).map_err(|e| e.to_string())?;
    std::fs::write(root.join(model_rel), &glb).map_err(|e| e.to_string())?;
    let model = ids.asset();
    write_json(
        &root.join(format!("{model_rel}.meta.json")),
        &new_sidecar(&model, Kind::Model),
    )?;
    std::fs::create_dir_all(root.join("content/looks")).map_err(|e| e.to_string())?;
    std::fs::write(root.join("content/looks/day.look.json"), LOOK).map_err(|e| e.to_string())?;
    let look = ids.asset();
    write_json(
        &root.join("content/looks/day.look.json.meta.json"),
        &new_sidecar(&look, Kind::Look),
    )?;

    let nodes = glb::import_nodes(&glb, true)?;
    let scene_id = ids.asset();
    let mut doc = Document::new(scene_from_model(
        &nodes, &model, &scene_id, "Blocks", &mut ids,
    ))?;
    let comp = |k: &str, v: &str| (k.to_string(), json::parse(v.as_bytes()).expect("component"));
    let env = ids.entity();
    doc.apply_all(&scene::create(
        &env,
        "Environment",
        None,
        None,
        BTreeMap::from([comp(
            "environment",
            &format!(r#"{{"look":"{look}","time":16.5,"weather":"clear"}}"#),
        )]),
    ))?;
    let cam = ids.entity();
    doc.apply_all(&scene::create(
        &cam,
        "Overview camera",
        None,
        None,
        BTreeMap::from([
            comp(
                "transform",
                r#"{"translation":[10,7,12],"rotation":[-0.1477,0.3496,0.0558,0.9235]}"#,
            ),
            comp("camera", r#"{"fov":50}"#),
        ]),
    ))?;
    // Physics: the ground is static scenery, and a tilted crate (Block A's mesh) falls onto it
    // when the editor simulates.
    let ground = scene::entities(doc.root())
        .iter()
        .find(|(_, e)| e.get("name").and_then(|n| n.string().ok()) == Some("Ground"))
        .map(|(id, _)| id.clone())
        .ok_or("the sample model has a Ground")?;
    doc.apply_all(&[scene::set_field(
        &ground,
        "collider",
        &[],
        json::parse(br#"{"center":[0,-0.1,0],"half_extents":[15,0.1,15]}"#)?,
    )])?;
    let crate_id = ids.entity();
    doc.apply_all(&scene::create(
        &crate_id,
        "Crate",
        None,
        None,
        BTreeMap::from([
            comp(
                "transform",
                r#"{"translation":[0,4,-3],"rotation":[0.1001,0.2003,0.0501,0.9733]}"#,
            ),
            comp(
                "mesh",
                &format!(r#"{{"model":"{model}","node":"Block A"}}"#),
            ),
            comp("rigid_body", "{}"),
            comp("collider", r#"{"shape":"box"}"#),
        ]),
    ))?;
    let issues = scene::check(doc.root(), &ganbate_scene::schema::engine_registry());
    if let Some(i) = issues.first() {
        return Err(format!("{}: {}", i.path, i.message));
    }
    project.write_document("content/scenes/blocks.scene.json", Kind::Scene, doc.root())?;
    project.manifest.startup = Some(scene_id);
    write_json(
        &root.join("ganbate.project.json"),
        &project.manifest.to_value(),
    )?;
    println!(
        "{}: studio sample with scene content/scenes/blocks.scene.json",
        root.display()
    );
    Ok(())
}
