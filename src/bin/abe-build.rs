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
    Ok(format!(
        "{} meshes | {} instances | {} triangles | {} materials | {} textures ({texels} texel bytes) | {} lights",
        w.meshes.len(),
        w.instances.len(),
        w.validate()?,
        w.materials.len(),
        w.textures.len(),
        w.lights.len(),
    ))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    match args.as_slice() {
        [cmd, source, dest] if cmd == "cook" => {
            let w = glb::import(&read_asset(Path::new(source))?)?;
            let data = cooked::encode(&w)?;
            if Path::new(dest).canonicalize().ok() == Some(Path::new(source).canonicalize()?) {
                return Err("cook destination must differ from the source".into());
            }
            abe_engine::assets::write_asset(Path::new(dest), &data)?;
            println!("{} | {} bytes", summary(&w)?, data.len());
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
        _ => return Err("usage: abe-build cook source.glb output.abew | inspect file.abew".into()),
    }
    Ok(())
}
