#![cfg(any(target_os = "linux", target_os = "windows"))]
use ganbate_engine::{
    Camera, Scene, Vec3,
    vulkan::present::{DesktopRenderer, PresentStatus, Presenter},
    window::Window,
};
use std::time::{Duration, Instant};

fn pump_until(window: &Window, condition: impl Fn() -> bool) {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        while window.poll().is_some() {}
        if condition() {
            return;
        }
        assert!(
            Instant::now() < end,
            "native window failed to reach requested state"
        );
        std::thread::sleep(Duration::from_millis(3));
    }
}
fn frames(window: &Window, p: &mut Presenter<'_>, s: &Scene, n: u64) {
    let cam = Camera {
        eye: Vec3(0.0, 0.0, 2.0),
        target: Vec3(0.0, 0.0, 0.0),
        vertical_fov_degrees: 55.0,
        near: 0.1,
        far: 10.0,
    };
    let target = p.stats().frames + n;
    let end = Instant::now() + Duration::from_secs(10);
    while p.stats().frames < target {
        assert!(Instant::now() < end, "timed out presenting frame");
        while window.poll().is_some() {}
        let status = p.draw(s, cam, [0.05, 0.08, 0.12, 1.0]).unwrap();
        if status != PresentStatus::Presented {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}
#[test]
#[ignore = "requires a native display and Vulkan graphics/present driver"]
fn presents_and_recovers_after_resize_hide_and_restore() {
    let window = Window::new("Ganbate presentation test", 320, 200).unwrap();
    pump_until(&window, || window.visible());
    let mut gpu = DesktopRenderer::new(&window).unwrap();
    let mut s = Scene::default();
    s.triangle(
        Vec3(-0.5, -0.5, 0.0),
        Vec3(0.5, -0.5, 0.0),
        Vec3(0.0, 0.5, 0.0),
        [1.0, 0.4, 0.2],
    );
    let mut p = gpu.presenter();
    frames(&window, &mut p, &s, 8);
    let first = p.stats();
    assert_eq!(first.vertex_allocations, 2);
    frames(&window, &mut p, &s, 12);
    assert_eq!(p.stats().vertex_allocations, first.vertex_allocations);
    for (w, h) in [(428, 240), (320, 200)] {
        let before = p.stats().swapchains;
        window.resize(w, h);
        pump_until(&window, || window.size() == (w, h));
        frames(&window, &mut p, &s, 6);
        assert!(p.stats().swapchains > before);
    }
    window.set_visible(false);
    pump_until(&window, || !window.visible());
    let cam = Camera {
        eye: Vec3(0.0, 0.0, 2.0),
        target: Vec3(0.0, 0.0, 0.0),
        vertical_fov_degrees: 55.0,
        near: 0.1,
        far: 10.0,
    };
    assert_eq!(p.draw(&s, cam, [0.0; 4]).unwrap(), PresentStatus::Suspended);
    window.set_visible(true);
    pump_until(&window, || window.visible());
    frames(&window, &mut p, &s, 5);
    // Exercise resource cleanup and another presenter on the same live window.
    drop(p);
    let mut p = gpu.presenter();
    frames(&window, &mut p, &s, 4);
    p.set_static_scene(&s).unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    let target = p.stats().frames + 5;
    while p.stats().frames < target {
        assert!(Instant::now() < end);
        while window.poll().is_some() {}
        p.draw_with_static(&s, cam, [0.0; 4]).unwrap();
    }
    assert_eq!(p.stats().scene_uploads, 1);
    assert_eq!(p.stats().vertex_allocations, 2);
}
