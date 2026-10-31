//! Compile one entry point of a WGSL file to SPIR-V and print its reflection.
//!
//! ```sh
//! cargo run -p abe-shader --example abe-shaderc -- shader.wgsl fragment fs_main out.spv
//! ```

use std::process::ExitCode;

use abe_shader::{Stage, compile};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 5 {
        eprintln!("usage: abe-shaderc <file.wgsl> <vertex|fragment|compute> <entry> <out.spv>");
        return ExitCode::from(2);
    }
    let stage = match args[2].as_str() {
        "vertex" => Stage::Vertex,
        "fragment" => Stage::Fragment,
        "compute" => Stage::Compute,
        other => {
            eprintln!("unknown stage '{other}' (expected vertex, fragment or compute)");
            return ExitCode::from(2);
        }
    };
    let source = match std::fs::read_to_string(&args[1]) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}: {e}", args[1]);
            return ExitCode::FAILURE;
        }
    };
    let start = std::time::Instant::now();
    let compiled = match compile(&source, stage, &args[3]) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}:{e}", args[1]);
            return ExitCode::FAILURE;
        }
    };
    let elapsed = start.elapsed();
    if let Err(e) = std::fs::write(&args[4], compiled.to_bytes()) {
        eprintln!("{}: {e}", args[4]);
        return ExitCode::FAILURE;
    }
    println!(
        "{} {:?} entry '{}': {} words in {:.2} ms -> {}",
        args[1],
        compiled.stage,
        args[3],
        compiled.words.len(),
        elapsed.as_secs_f64() * 1e3,
        args[4]
    );
    for b in &compiled.bindings {
        println!(
            "  binding group {} binding {}: {:?}",
            b.group, b.binding, b.kind
        );
    }
    if compiled.push_constant_size > 0 {
        println!("  push constants: {} bytes", compiled.push_constant_size);
    }
    for i in &compiled.inputs {
        println!("  input  @location({}) {:?}", i.location, i.ty);
    }
    for o in &compiled.outputs {
        println!("  output @location({}) {:?}", o.location, o.ty);
    }
    if compiled.stage == Stage::Compute {
        println!("  workgroup size {:?}", compiled.workgroup_size);
    }
    ExitCode::SUCCESS
}
