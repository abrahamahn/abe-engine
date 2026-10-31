//! Compare actual presentation timestamps, in milliseconds, to a sustained budget.
use abe_engine::timing::Pacing;
use std::{
    fs::File,
    io::{BufRead, BufReader},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.as_slice() == ["--help"] {
        println!(
            "abe-framecheck TARGET_HZ MIN_SECONDS presented-ms.txt\nOne monotonically increasing timestamp per distinct displayed frame, in milliseconds."
        );
        return Ok(());
    }
    let [hz, seconds, file] = args.as_slice() else {
        return Err("usage: abe-framecheck TARGET_HZ MIN_SECONDS presented-ms.txt".into());
    };
    let hz = hz.parse::<u32>()?;
    let seconds = seconds.parse::<u32>()?;
    if hz == 0 || hz > 1000 || seconds < 60 {
        return Err("target must be 1..1000 Hz and duration at least 60 seconds".into());
    }
    let file = File::open(file)?;
    if file.metadata()?.len() > 128 * 1024 * 1024 {
        return Err("trace exceeds 128 MiB".into());
    }
    let mut times = Vec::new();
    for line in BufReader::new(file).lines() {
        if times.len() == 2_000_000 {
            return Err("trace exceeds two million timestamps".into());
        }
        times.push(line?.trim().parse()?);
    }
    let r = Pacing::analyze(&times, hz)?;
    let worst = r
        .worst_minute_fps
        .map_or_else(|| "unavailable".into(), |fps| format!("{fps:.2} FPS"));
    println!(
        "Actual presentation | {hz} Hz target | {:.1} s | {:.2} FPS overall | worst minute {worst}",
        r.duration_s, r.mean_fps
    );
    println!(
        "intervals p50 {:.3} ms | p95 {:.3} ms | p99 {:.3} ms | {} over 1.5 frame periods",
        r.intervals.p50, r.intervals.p95, r.intervals.p99, r.slow_intervals
    );
    if !r.meets(hz, seconds) {
        return Err("display pacing target missed".into());
    }
    println!("Display pacing target met.");
    Ok(())
}
