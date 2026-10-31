//! Uncapped offscreen throughput on the selected Vulkan driver.
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "android"))]
#[path = "../../samples/observatory/scene.rs"]
mod demo;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "android"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use abe_engine::{
        Camera, assets,
        timing::Distribution,
        vulkan::{VulkanRenderer, bench},
    };
    let mut args = std::env::args().skip(1);
    let (mut w, mut h, mut warmup, mut frames) = (1920, 1080, 40, 240);
    let mut scene_path = None;
    while let Some(flag) = args.next() {
        if flag == "--help" {
            println!(
                "abe-bench [--scene world.abew] [--width 1920] [--height 1080] [--warmup 40] [--frames 240]"
            );
            return Ok(());
        }
        let value = args.next().ok_or("missing option value")?;
        match flag.as_str() {
            "--scene" => scene_path = Some(value),
            "--width" => w = value.parse()?,
            "--height" => h = value.parse()?,
            "--warmup" => warmup = value.parse()?,
            "--frames" => frames = value.parse()?,
            _ => return Err(format!("unknown option {flag}").into()),
        }
    }
    let scene = if let Some(path) = scene_path {
        assets::cooked::decode(&assets::read_asset(std::path::Path::new(&path))?)?.scene()?
    } else {
        demo::gallery(0.65)
    };
    let bounds = scene
        .bounds()
        .ok_or("benchmark needs a finite nonempty scene")?;
    let mut gpu = VulkanRenderer::new()?;
    println!(
        "ABE offscreen | {} | {w}x{h} | {} triangles | {warmup} warmup + {frames} measured | 1 static upload | 2 frames in flight",
        gpu.device_name(),
        scene.triangles.len()
    );
    let run = bench::run(&mut gpu, &scene, (w, h), warmup, frames, |i| {
        Camera::orbit(
            bounds,
            0.4 + (i % 360) as f32 * std::f32::consts::TAU / 360.,
        )
    })?;
    println!(
        "Completed throughput {:.2} FPS in {:.3} s",
        run.throughput(),
        run.seconds
    );
    for (name, samples) in [
        ("CPU prepare + submit", &run.cpu_ms),
        ("GPU draw", &run.gpu_ms),
        ("CPU wait for slot", &run.fence_ms),
    ] {
        if let Some(d) = Distribution::from_ms(samples) {
            println!(
                "{name}: p50 {:.3} ms | p95 {:.3} ms | p99 {:.3} ms (n={})",
                d.p50, d.p95, d.p99, d.count
            );
        } else {
            println!("{name}: unavailable");
        }
    }
    println!(
        "Render throughput only. Display pacing, gameplay, networking, UI and asset loads are outside this run."
    );
    Ok(())
}
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "android")))]
fn main() {
    eprintln!("abe-bench needs a supported Vulkan host");
    std::process::exit(1);
}
