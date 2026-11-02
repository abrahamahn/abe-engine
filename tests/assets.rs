//! The `abe-build` entry points on a synthetic textured, lit scene.
use abe_engine::{
    Vec3,
    assets::{cooked, png, read_asset},
    world::{Emission, Shading},
};
use std::{path::Path, process::Command};

fn glb(doc: &str, bin: &[u8]) -> Vec<u8> {
    let mut json = doc.as_bytes().to_vec();
    json.resize(json.len().next_multiple_of(4), b' ');
    let mut bin = bin.to_vec();
    bin.resize(bin.len().next_multiple_of(4), 0);
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

/// A lantern triangle with a 2x2 texture, lit by a point light under a translated node.
fn scene(mime: &str) -> Vec<u8> {
    let mut bin: Vec<u8> = [0f32, 0., 0., 1., 0., 0., 0., 1., 0.]
        .iter()
        .chain(&[0., 0., 1., 0., 0., 1.])
        .flat_map(|f| f.to_le_bytes())
        .collect();
    let image = png::encode(
        2,
        2,
        &[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 9, 9, 9, 255],
    )
    .expect("synthetic texture");
    let image_len = image.len();
    bin.extend_from_slice(&image);
    let doc = format!(
        r#"{{"asset":{{"version":"2.0"}},"extensionsUsed":["KHR_lights_punctual","KHR_materials_emissive_strength"],
        "extensions":{{"KHR_lights_punctual":{{"lights":[{{"type":"point","color":[1,0.6,0.2],"intensity":8,"range":6}}]}}}},
        "buffers":[{{"byteLength":{len}}}],
        "bufferViews":[{{"buffer":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":24}},{{"buffer":0,"byteOffset":60,"byteLength":{image_len}}}],
        "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":1,"componentType":5126,"count":3,"type":"VEC2"}}],
        "images":[{{"bufferView":2,"mimeType":"{mime}"}}],"textures":[{{"source":0}}],
        "materials":[{{"name":"lantern","pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}}}},"emissiveFactor":[1,0.6,0.2],
            "extensions":{{"KHR_materials_emissive_strength":{{"emissiveStrength":4}}}},"extras":{{"abe":{{"emission":"lamp"}}}}}}],
        "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"TEXCOORD_0":1}},"material":0}}]}}],
        "nodes":[{{"translation":[3,0,0],"children":[1,2]}},{{"mesh":0}},{{"translation":[0,2,0],"extensions":{{"KHR_lights_punctual":{{"light":0}}}}}}],
        "scenes":[{{"nodes":[0]}}]}}"#,
        len = bin.len(),
    );
    glb(&doc, &bin)
}

fn build(args: &[&Path]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_abe-build"))
        .args(args)
        .output()
        .expect("run abe-build");
    let text = String::from_utf8_lossy(&out.stdout) + String::from_utf8_lossy(&out.stderr);
    (out.status.success(), text.into_owned())
}

#[test]
fn abe_build_cooks_and_inspects_a_textured_lit_scene() {
    let dir = std::env::temp_dir().join(format!("abe-assets-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (source, cooked_path) = (dir.join("scene.glb"), dir.join("scene.abew"));
    std::fs::write(&source, scene("image/png")).unwrap();
    let (ok, text) = build(&[Path::new("cook"), &source, &cooked_path]);
    assert!(ok, "{text}");
    assert!(
        text.contains("1 meshes | 1 instances | 1 triangles | 1 materials | 1 textures (20 texel bytes) | 1 lights"),
        "{text}"
    );
    let (ok, text) = build(&[Path::new("inspect"), &cooked_path]);
    assert!(ok, "{text}");
    assert!(text.starts_with("ABEW v3 |"), "{text}");
    assert!(
        text.contains("material 0: lantern [standard, lamp, base 2x2]"),
        "{text}"
    );

    let w = cooked::decode(&read_asset(&cooked_path).unwrap()).unwrap();
    let m = &w.materials[0];
    assert_eq!(
        (m.shading, m.emission, m.emissive),
        (Shading::Standard, Emission::Lamp, [4., 2.4, 0.8])
    );
    assert_eq!(w.lights[0].position, Vec3(3., 2., 0.));
    assert_eq!(
        w.scene().unwrap().triangles[0].positions[1],
        Vec3(4., 0., 0.)
    );

    // Compact, with the image kept as a file for the host to decode: the same world.
    let compact_path = dir.join("scene-compact.abew");
    let (ok, text) = build(&[
        Path::new("cook"),
        Path::new("--compact"),
        Path::new("--keep-images"),
        &source,
        &compact_path,
    ]);
    assert!(ok, "{text}");
    assert!(
        text.contains("0 texel bytes,") && text.contains("encoded image bytes"),
        "{text}"
    );
    let c = cooked::decode(&read_asset(&compact_path).unwrap()).unwrap();
    assert_eq!(
        (c.materials.clone(), c.lights.clone()),
        (w.materials.clone(), w.lights.clone())
    );
    assert_eq!(
        c.textures[0].encoded.as_ref().map(|e| e.mime.as_str()),
        Some("image/png")
    );

    // The same file as a run-time model: what `loadModel` would read.
    let (ok, text) = build(&[Path::new("model"), &source]);
    assert!(ok, "{text}");
    assert!(
        text.starts_with(
            "model | 3 nodes | 1 meshes | 3 vertices | 1 triangles | 1 materials | 1 textures ("
        ),
        "{text}"
    );
    assert!(
        text.contains("mesh 0  primitive 0: 3 vertices, 1 triangles [material lantern, uv]"),
        "{text}"
    );

    // A JPEG texture is named in the error and leaves the previous output in place.
    std::fs::write(&source, scene("image/jpeg")).unwrap();
    let (ok, text) = build(&[Path::new("cook"), &source, &cooked_path]);
    assert!(!ok && text.contains("image/jpeg"), "{text}");
    assert!(cooked::decode(&read_asset(&cooked_path).unwrap()).is_ok());
    // A model keeps its JPEG for the host to decode.
    let (ok, text) = build(&[Path::new("model"), &source]);
    assert!(ok && text.contains("1 textures ("), "{text}");
    std::fs::remove_dir_all(&dir).unwrap();
}
