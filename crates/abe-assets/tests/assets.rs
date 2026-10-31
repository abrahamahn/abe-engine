use abe_assets::{cooked, glb, json};
use abe_core::Vec3;

fn glb_file(doc: &str, bytes: &[u8]) -> Vec<u8> {
    let mut j = doc.as_bytes().to_vec();
    while j.len() % 4 != 0 {
        j.push(b' ')
    }
    let mut b = bytes.to_vec();
    while b.len() % 4 != 0 {
        b.push(0)
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2_u32.to_le_bytes());
    out.extend_from_slice(&(28 + j.len() as u32 + b.len() as u32).to_le_bytes());
    for (id, chunk) in [(0x4e4f534au32, j), (0x004e4942, b)] {
        out.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&chunk);
    }
    out
}
const DOC: &str = r#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":54}],"bufferViews":[{"buffer":0,"byteLength":48,"byteStride":16},{"buffer":0,"byteOffset":48,"byteLength":6}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"},{"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}],"meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":1}]}],"nodes":[{"mesh":0,"translation":[1,0,0],"children":[1]},{"mesh":0,"translation":[0,2,0]}],"scenes":[{"nodes":[0]}],"scene":0}"#;
fn bytes() -> Vec<u8> {
    let mut b = Vec::new();
    for v in [[0., 0., 0., 9.], [1., 0., 0., 9.], [0., 1., 0., 9.]] {
        for f in v {
            b.extend_from_slice(&f32::to_le_bytes(f));
        }
    }
    for i in [0u16, 1, 2] {
        b.extend_from_slice(&i.to_le_bytes());
    }
    b
}
#[test]
fn source_to_cooked_roundtrip_preserves_hierarchy_and_sharing() {
    let w = glb::import(&glb_file(DOC, &bytes())).unwrap();
    assert_eq!(
        (w.meshes.len(), w.instances.len(), w.validate().unwrap()),
        (1, 2, 2)
    );
    assert_eq!(
        w.scene().unwrap().triangles[1].positions[2],
        Vec3(1., 3., 0.)
    );
    let b = cooked::encode(&w).unwrap();
    let w2 = cooked::decode(&b).unwrap();
    assert_eq!(w, w2);
    assert_eq!(cooked::encode(&w2).unwrap(), b);
    for n in 0..b.len() {
        assert!(cooked::decode(&b[..n]).is_err())
    }
    let mut future = b.clone();
    future[4] = 2;
    assert!(cooked::decode(&future).is_err());
    let mut extra = b;
    extra.push(0);
    assert!(cooked::decode(&extra).is_err());
}
#[test]
fn malformed_input_and_unsupported_features_fail_before_rendering() {
    for doc in [
        DOC.replace("\"children\":[1]", "\"children\":[0]"),
        DOC.replace("\"byteLength\":48", "\"byteLength\":4"),
        DOC.replace("\"nodes\":[0]", "\"nodes\":[2]"),
        DOC.replace("\"version\":\"2.0\"", "\"version\":\"9.0\""),
    ] {
        assert!(glb::import(&glb_file(&doc, &bytes())).is_err());
    }
    let mut b = bytes();
    b[48] = 9;
    assert!(glb::import(&glb_file(DOC, &b)).is_err());
    let ext = DOC.replacen('{', r#"{"extensionsRequired":["UNKNOWN_VENDOR"],"#, 1);
    assert!(
        glb::import(&glb_file(&ext, &bytes()))
            .unwrap_err()
            .contains("UNKNOWN_VENDOR")
    );
    let message = json::parse(br#"{"name":"\uD83D\uDE80"}"#).unwrap();
    assert_eq!(message.get("name").unwrap().string().unwrap(), "🚀");
    for bad in [
        &br#"[1,]"#[..],
        br#"{"a":1,"a":2}"#,
        br#""\uDFFF""#,
        b"01",
        b"1e999",
    ] {
        assert!(json::parse(bad).is_err())
    }
}
