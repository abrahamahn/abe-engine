//! Minimal standalone player. The example scene is also used by the offscreen oracle.
#[cfg(any(target_os = "linux", target_os = "windows"))]
#[path = "../../samples/observatory/scene.rs"]
mod demo;

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use abe_engine::{
        Camera, Vec3,
        vulkan::present::{DesktopRenderer, PresentStatus},
        window::{Event, Key, Window},
    };
    use std::time::{Duration, Instant};
    let mut args = std::env::args().skip(1);
    let limit = match args.next().as_deref() {
        None => None,
        Some("--frames") => {
            let n = args.next().ok_or("missing frame count")?.parse::<u64>()?;
            if n == 0 {
                return Err("frame count must be positive".into());
            }
            Some(n)
        }
        Some("--help") => {
            println!(
                "abe-player [--frames N]\nSpace or click: pause/resume. Arrows: orbit. Esc: close."
            );
            return Ok(());
        }
        _ => return Err("usage: abe-player [--frames N]".into()),
    };
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let window = Window::new("ABE Player", 800, 480)?;
    let mut gpu = DesktopRenderer::new(&window)?;
    println!(
        "ABE Player | {} | space/click: pause | arrows: orbit | Esc: close",
        gpu.device_name()
    );
    let mut presenter = gpu.presenter();
    let mut last = Instant::now();
    let mut animation = 0.45f32;
    let mut angle = 0.4f32;
    let mut paused = false;
    'player: loop {
        while let Some(event) = window.poll() {
            match event {
                Event::Close | Event::Pressed(Key::Escape) => break 'player,
                Event::Pressed(Key::Space) | Event::Clicked(_, _) => paused = !paused,
                Event::Pressed(Key::Left) => angle -= 0.15,
                Event::Pressed(Key::Right) => angle += 0.15,
                _ => {}
            }
        }
        let now = Instant::now();
        if !paused && window.visible() {
            animation += (now - last).as_secs_f32().min(0.1) * 0.65;
        }
        last = now;
        let camera = Camera {
            eye: Vec3(angle.sin() * 5.7, 2.0, angle.cos() * 5.7),
            target: Vec3(0.0, 0.15, 0.0),
            vertical_fov_degrees: 43.0,
            near: 0.1,
            far: 60.0,
        };
        let status = presenter.draw(&demo::gallery(animation), camera, [0.03, 0.045, 0.07, 1.0])?;
        if limit.is_some_and(|n| presenter.stats().frames >= n) {
            break;
        }
        if status != PresentStatus::Presented {
            std::thread::sleep(Duration::from_millis(12));
        }
    }
    println!("Presentation {:?}", presenter.stats());
    Ok(())
}
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn main() {
    eprintln!("This version of the presented player requires Windows or Linux/X11.");
    std::process::exit(1);
}
