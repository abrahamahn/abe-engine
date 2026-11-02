//! Offline asset cooker and inspector.
use abe_engine::{
    assets::{cooked, glb, read_asset},
    world::{AlphaMode, Emission, Shading, World},
};
use std::{ffi::OsString, path::Path};

fn summary(w: &World) -> Result<String, String> {
    let texels: u64 = w
        .textures
        .iter()
        .flat_map(|t| &t.levels)
        .map(|l| l.len() as u64)
        .sum();
    let encoded: u64 = w
        .textures
        .iter()
        .filter_map(|t| t.encoded.as_ref())
        .map(|e| e.data.len() as u64)
        .sum();
    let images = if encoded > 0 {
        format!(", {encoded} encoded image bytes")
    } else {
        String::new()
    };
    Ok(format!(
        "{} meshes | {} instances | {} triangles | {} materials | {} textures ({texels} texel bytes{images}) | {} lights",
        w.meshes.len(),
        w.instances.len(),
        w.validate()?,
        w.materials.len(),
        w.textures.len(),
        w.lights.len(),
    ))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<OsString> = std::env::args_os().skip(1).collect();
    // Cook options: --compact quantizes geometry, --keep-images keeps PNG, JPEG and WebP
    // images encoded (for hosts that decode them, such as browsers).
    let mut flag = |name: &str| {
        let before = args.len();
        args.retain(|a| a != name);
        args.len() != before
    };
    let (compact, keep_images) = (flag("--compact"), flag("--keep-images"));
    match args.as_slice() {
        [cmd, source, dest] if cmd == "cook" => {
            let w = glb::import_with(&read_asset(Path::new(source))?, keep_images)?;
            let data = if compact {
                cooked::encode_compact(&w)?
            } else {
                cooked::encode(&w)?
            };
            if Path::new(dest).canonicalize().ok() == Some(Path::new(source).canonicalize()?) {
                return Err("cook destination must differ from the source".into());
            }
            abe_engine::assets::write_asset(Path::new(dest), &data)?;
            println!("{} | {} bytes", summary(&w)?, data.len());
        }
        [cmd, file] if cmd == "model" => {
            // A run-time model as `createEngine().loadModel` reads it.
            let data = read_asset(Path::new(file))?;
            let m = abe_engine::assets::model::load_glb(&data)?;
            let prims = m.meshes.iter().flat_map(|x| &x.primitives);
            let triangles: usize = prims.clone().map(|p| p.indices.len() / 3).sum();
            let vertices: usize = prims.map(|p| p.positions.len()).sum();
            let image_bytes: usize = m.images.iter().map(|i| i.data.len()).sum();
            println!(
                "model | {} nodes | {} meshes | {vertices} vertices | {triangles} triangles | {} materials | {} textures ({image_bytes} image bytes) | {} skins | {} clips | {} bytes",
                m.nodes.len(),
                m.meshes.len(),
                m.materials.len(),
                m.textures.len(),
                m.skins.len(),
                m.clips.len(),
                data.len(),
            );
            for (i, mesh) in m.meshes.iter().enumerate() {
                for (j, p) in mesh.primitives.iter().enumerate() {
                    let material = p
                        .material
                        .map_or("none".to_string(), |k| m.materials[k].name.clone());
                    let mut traits = vec![format!("material {material}")];
                    if !p.uvs.is_empty() {
                        traits.push("uv".into());
                    }
                    if !p.joints.is_empty() {
                        traits.push("skinned".into());
                    }
                    if !p.targets.is_empty() {
                        traits.push(format!("{} targets", p.targets.len()));
                    }
                    println!(
                        "  mesh {i} {} primitive {j}: {} vertices, {} triangles [{}]",
                        mesh.name,
                        p.positions.len(),
                        p.indices.len() / 3,
                        traits.join(", ")
                    );
                }
            }
            for c in &m.clips {
                println!(
                    "  clip {}: {:.2} s, {} channels",
                    c.name,
                    c.duration,
                    c.channels.len()
                );
            }
        }
        [cmd, file] if cmd == "inspect" => {
            let data = read_asset(Path::new(file))?;
            let w = cooked::decode(&data)?;
            let version = cooked::version(&data).unwrap_or_default();
            println!("ABEW v{version} | {} | {} bytes", summary(&w)?, data.len());
            for (i, m) in w.materials.iter().enumerate() {
                let mut traits = vec![
                    match m.shading {
                        Shading::Standard => "standard",
                        Shading::Foliage => "foliage",
                        Shading::Water => "water",
                        Shading::Unlit => "unlit",
                    }
                    .to_string(),
                ];
                if m.emission == Emission::Lamp {
                    traits.push("lamp".into());
                }
                match m.alpha {
                    AlphaMode::Opaque => {}
                    AlphaMode::Mask(c) => traits.push(format!("mask {c}")),
                    AlphaMode::Blend => traits.push("blend".into()),
                }
                for (name, map) in [
                    ("base", m.base_color_texture),
                    ("normal", m.normal_texture),
                    ("emissive", m.emissive_texture),
                ] {
                    if let Some(t) = map {
                        let t = &w.textures[t as usize];
                        traits.push(format!("{name} {}x{}", t.width, t.height));
                    }
                }
                println!("  material {i}: {} [{}]", m.name, traits.join(", "));
            }
        }
        _ => {
            return Err(
                "usage: abe-build cook [--compact] [--keep-images] source.glb output.abew \
                        | inspect file.abew | model file.glb"
                    .into(),
            );
        }
    }
    Ok(())
}
