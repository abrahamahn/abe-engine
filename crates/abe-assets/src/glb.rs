//! Static glTF 2.0 GLB import. Geometry and material base colors only.
use crate::{
    cooked::MAX_BYTES,
    json::{self, Value},
};
use abe_core::{
    Vec3,
    world::{Instance, MAX_TRIANGLES, Mesh, Transform, World},
};

fn arr<'a>(o: &'a Value, k: &str) -> Result<&'a [Value], String> {
    o.get(k).map_or(Ok(&[][..]), Value::array)
}
fn need<'a>(o: &'a Value, k: &str) -> Result<&'a Value, String> {
    o.get(k).ok_or_else(|| format!("missing glTF field {k}"))
}
fn index<T>(a: &[T], i: usize) -> Result<&T, String> {
    a.get(i)
        .ok_or_else(|| format!("glTF index {i} outside collection of {}", a.len()))
}
fn opt_u(o: &Value, k: &str, default: usize) -> Result<usize, String> {
    o.get(k).map_or(Ok(default), Value::integer)
}
fn floats<const N: usize>(v: &Value) -> Result<[f32; N], String> {
    let a = v.array()?;
    if a.len() != N {
        return Err(format!("expected {N} coordinates"));
    }
    let mut out = [0.0; N];
    for (i, n) in a.iter().enumerate() {
        out[i] = n.float()?;
    }
    Ok(out)
}
fn word(b: &[u8], offset: usize) -> Result<u32, String> {
    let bytes = b.get(offset..offset + 4).ok_or("truncated GLB")?;
    Ok(u32::from_le_bytes(bytes.try_into().expect("four bytes")))
}
fn chunks(data: &[u8]) -> Result<(Value, &[u8]), String> {
    if data.len() > MAX_BYTES
        || data.get(..4) != Some(b"glTF")
        || word(data, 4)? != 2
        || word(data, 8)? as usize != data.len()
    {
        return Err("invalid GLB header, length or version".into());
    }
    let mut offset = 12;
    let mut document = None;
    let mut bin = None;
    while offset < data.len() {
        let length = word(data, offset)? as usize;
        let kind = word(data, offset + 4)?;
        offset += 8;
        let end = offset.checked_add(length).ok_or("chunk length overflow")?;
        let chunk = data.get(offset..end).ok_or("truncated GLB chunk")?;
        if length % 4 != 0 {
            return Err("unaligned GLB chunk".into());
        }
        match kind {
            0x4e4f534a if document.is_none() && offset == 20 => {
                document = Some(json::parse(chunk)?)
            }
            0x004e4942 if document.is_some() && bin.is_none() => bin = Some(chunk),
            0x4e4f534a | 0x004e4942 => return Err("duplicate or misplaced GLB chunk".into()),
            _ if document.is_none() => return Err("first GLB chunk must be JSON".into()),
            _ => {}
        }
        offset = end;
    }
    Ok((document.ok_or("missing JSON")?, bin.unwrap_or(&[])))
}
struct Access<'a> {
    bytes: &'a [u8],
    stride: usize,
    count: usize,
    component: usize,
}
fn access<'a>(
    doc: &Value,
    bin: &'a [u8],
    id: usize,
    positions: bool,
) -> Result<Access<'a>, String> {
    let a = index(arr(doc, "accessors")?, id)?;
    if a.get("sparse").is_some() || a.get("normalized") == Some(&Value::Bool(true)) {
        return Err("sparse and normalized accessors are unsupported".into());
    }
    let count = need(a, "count")?.integer()?;
    let component = need(a, "componentType")?.integer()?;
    let shape = need(a, "type")?.string()?;
    let elem = match (positions, shape, component) {
        (true, "VEC3", 5126) => 12,
        (false, "SCALAR", 5121) => 1,
        (false, "SCALAR", 5123) => 2,
        (false, "SCALAR", 5125) => 4,
        _ => return Err("unsupported position/index accessor format".into()),
    };
    if count == 0 || count > MAX_TRIANGLES * 3 {
        return Err("accessor element count exceeds limits".into());
    }
    let view = index(arr(doc, "bufferViews")?, need(a, "bufferView")?.integer()?)?;
    if need(view, "buffer")?.integer()? != 0 {
        return Err("external buffers unsupported".into());
    }
    let start = opt_u(view, "byteOffset", 0)?;
    let length = need(view, "byteLength")?.integer()?;
    let end = start.checked_add(length).ok_or("view range overflow")?;
    let range = bin.get(start..end).ok_or("buffer view out of range")?;
    let offset = opt_u(a, "byteOffset", 0)?;
    let stride = opt_u(view, "byteStride", elem)?;
    let align = if positions { 4 } else { elem };
    if stride < elem
        || stride > 252
        || stride % align != 0
        || start % align != 0
        || offset % align != 0
    {
        return Err("invalid accessor alignment or stride".into());
    }
    let last = (count - 1)
        .checked_mul(stride)
        .and_then(|n| n.checked_add(elem))
        .ok_or("accessor overflow")?;
    let bytes = range
        .get(offset..)
        .and_then(|s| s.get(..last))
        .ok_or("accessor outside buffer view")?;
    Ok(Access {
        bytes,
        stride,
        count,
        component,
    })
}
fn node_transform(n: &Value) -> Result<Transform, String> {
    if let Some(m) = n.get("matrix") {
        if ["translation", "rotation", "scale"]
            .iter()
            .any(|k| n.get(k).is_some())
        {
            return Err("matrix and TRS specified together".into());
        }
        let t = Transform(floats(m)?);
        if !t.valid() {
            return Err("invalid node matrix".into());
        }
        return Ok(t);
    }
    let t = n
        .get("translation")
        .map(floats)
        .transpose()?
        .unwrap_or([0.; 3]);
    let r = n
        .get("rotation")
        .map(floats)
        .transpose()?
        .unwrap_or([0., 0., 0., 1.]);
    let s = n.get("scale").map(floats).transpose()?.unwrap_or([1.; 3]);
    Transform::from_trs(t, r, s)
}
fn visit(
    nodes: &[Value],
    ranges: &[std::ops::Range<u32>],
    instances: &mut Vec<Instance>,
    seen: &mut [bool],
    id: usize,
    parent: Transform,
    depth: usize,
) -> Result<(), String> {
    if depth > 128 || instances.len() > 250_000 {
        return Err("node hierarchy exceeds limits".into());
    }
    let node = index(nodes, id)?;
    if std::mem::replace(&mut seen[id], true) {
        return Err("cyclic or multiply-parented glTF node".into());
    }
    if node.get("skin").is_some() || node.get("weights").is_some() {
        return Err("skinned or morphed nodes require the animation importer".into());
    }
    let t = parent.compose(node_transform(node)?);
    if let Some(mesh) = node.get("mesh") {
        for m in index(ranges, mesh.integer()?)?.clone() {
            if instances.len() >= 250_000 {
                return Err("too many mesh instances".into());
            }
            instances.push(Instance {
                mesh: m,
                transform: t,
            });
        }
    }
    for child in arr(node, "children")? {
        visit(
            nodes,
            ranges,
            instances,
            seen,
            child.integer()?,
            t,
            depth + 1,
        )?;
    }
    Ok(())
}
pub fn import(data: &[u8]) -> Result<World, String> {
    let (doc, bin) = chunks(data)?;
    if need(need(&doc, "asset")?, "version")?.string()? != "2.0" {
        return Err("expected glTF 2.0".into());
    }
    if let Some(ext) = arr(&doc, "extensionsRequired")?.first() {
        return Err(format!(
            "unsupported required glTF extension: {}",
            ext.string()?
        ));
    }
    if !arr(&doc, "animations")?.is_empty() {
        return Err("animated source requires the animation importer".into());
    }
    let buffers = arr(&doc, "buffers")?;
    let declared = match buffers {
        [] => 0,
        [b] if b.get("uri").is_none() => need(b, "byteLength")?.integer()?,
        _ => return Err("expected one embedded GLB buffer".into()),
    };
    let bytes = bin
        .get(..declared)
        .ok_or("declared buffer length exceeds BIN chunk")?;
    if bin.len() - declared > 3 {
        return Err("unexpected excess BIN data".into());
    }
    let mut w = World::default();
    let mut ranges = Vec::new();
    let mut remaining = MAX_BYTES;
    for gltf_mesh in arr(&doc, "meshes")? {
        let start = w.meshes.len() as u32;
        for p in need(gltf_mesh, "primitives")?.array()? {
            if opt_u(p, "mode", 4)? != 4 || !arr(p, "targets")?.is_empty() {
                return Err("expected static triangle primitives".into());
            }
            if p.get("extensions")
                .and_then(|e| e.get("KHR_draco_mesh_compression"))
                .is_some()
            {
                return Err(
                    "decode KHR_draco_mesh_compression in the source asset pipeline".into(),
                );
            }
            let ap = access(
                &doc,
                bytes,
                need(need(p, "attributes")?, "POSITION")?.integer()?,
                true,
            )?;
            let ai = p
                .get("indices")
                .map(|v| access(&doc, bytes, v.integer()?, false))
                .transpose()?;
            let num = ai.as_ref().map_or(ap.count, |a| a.count);
            if num % 3 != 0 || w.meshes.len() >= 65_536 {
                return Err("invalid primitive count".into());
            }
            remaining = remaining
                .checked_sub(12 * ap.count + 4 * num)
                .ok_or("decoded geometry exceeds memory budget")?;
            let mut positions = Vec::with_capacity(ap.count);
            for i in 0..ap.count {
                let b = &ap.bytes[i * ap.stride..];
                let f = |n| f32::from_bits(word(b, n).expect("validated position range"));
                positions.push(Vec3(f(0), f(4), f(8)));
            }
            let indices = if let Some(a) = ai {
                (0..a.count)
                    .map(|i| {
                        let b = &a.bytes[i * a.stride..];
                        match a.component {
                            5121 => u32::from(b[0]),
                            5123 => u32::from(u16::from_le_bytes([b[0], b[1]])),
                            _ => word(b, 0).expect("validated index range"),
                        }
                    })
                    .collect()
            } else {
                (0..ap.count as u32).collect()
            };
            let mut color = [1.; 3];
            if let Some(m) = p.get("material") {
                let mat = index(arr(&doc, "materials")?, m.integer()?)?;
                if let Some(pbr) = mat.get("pbrMetallicRoughness") {
                    if pbr.get("baseColorTexture").is_some() {
                        return Err(
                            "textured base color is unsupported by the static color importer"
                                .into(),
                        );
                    }
                    if let Some(f) = pbr.get("baseColorFactor") {
                        let [r, g, b, _] = floats(f)?;
                        color = [r, g, b];
                    }
                }
            }
            w.meshes.push(Mesh {
                positions,
                indices,
                color,
            });
        }
        ranges.push(start..(w.meshes.len() as u32));
    }
    let nodes = arr(&doc, "nodes")?;
    let scenes = arr(&doc, "scenes")?;
    let selected = index(scenes, opt_u(&doc, "scene", 0)?)?;
    let mut seen = vec![false; nodes.len()];
    for root in arr(selected, "nodes")? {
        visit(
            nodes,
            &ranges,
            &mut w.instances,
            &mut seen,
            root.integer()?,
            Transform::IDENTITY,
            0,
        )?;
    }
    w.validate()?;
    Ok(w)
}
