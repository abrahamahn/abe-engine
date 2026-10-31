//! Offline asset cooker and inspector.
use abe_engine::assets::{cooked, glb, read_asset};
use std::{ffi::OsString, path::Path};
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
            println!(
                "{} meshes, {} instances, {} triangles, {} bytes",
                w.meshes.len(),
                w.instances.len(),
                w.validate()?,
                data.len()
            );
        }
        [cmd, file] if cmd == "inspect" => {
            let w = cooked::decode(&read_asset(Path::new(file))?)?;
            println!(
                "ABEW v1 | {} meshes | {} instances | {} triangles",
                w.meshes.len(),
                w.instances.len(),
                w.validate()?
            );
        }
        _ => return Err("usage: abe-build cook source.glb output.abew | inspect file.abew".into()),
    }
    Ok(())
}
