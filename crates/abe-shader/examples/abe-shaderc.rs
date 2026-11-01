//! Compile one entry point of a WGSL file to SPIR-V and print its reflection. An output path
//! ending in `.wgsl` writes the WebGPU WGSL for the entry point instead.
//!
//! ```sh
//! cargo run -p abe-shader --example abe-shaderc -- shader.wgsl fragment fs_main out.spv
//! cargo run -p abe-shader --example abe-shaderc -- shader.wgsl fragment fs_main out.wgsl
//! ```

use std::process::ExitCode;

use abe_shader::{Stage, compile, compile_wgsl};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 5 {
        eprintln!(
            "usage: abe-shaderc <file.wgsl> <vertex|fragment|compute> <entry> <out.spv|out.wgsl>"
        );
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
    let webgpu = args[4].ends_with(".wgsl");
    let result = if webgpu {
        compile_wgsl(&source, stage, &args[3])
    } else {
        compile(&source, stage, &args[3])
    };
    let compiled = match result {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}:{e}", args[1]);
            return ExitCode::FAILURE;
        }
    };
    let elapsed = start.elapsed();
    let bytes = match &compiled.wgsl {
        Some(w) if webgpu => w.source.clone().into_bytes(),
        _ => compiled.to_bytes(),
    };
    if let Err(e) = std::fs::write(&args[4], bytes) {
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
    if let Some(w) = &compiled.wgsl {
        println!("  WGSL entry point '{}'", w.entry);
        if let Some((group, binding)) = w.push_constants {
            println!(
                "  push constants: uniform at @group({group}) @binding({binding}), dynamic offset"
            );
        }
    }
    ExitCode::SUCCESS
}
