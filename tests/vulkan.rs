#![cfg(any(target_os = "linux", target_os = "windows", target_os = "android"))]
use ganbate_engine::{Vertex, vulkan::VulkanRenderer};

fn triangle(depth: f32, rgb: [f32; 3]) -> [Vertex; 3] {
    [[-0.8, -0.8], [0.8, -0.8], [0.0, 0.8]].map(|[x, y]| Vertex {
        clip: [x, y, depth, 1.0],
        rgba: [rgb[0], rgb[1], rgb[2], 1.0],
    })
}
#[test]
#[ignore = "requires a Vulkan 1.0 driver; run cargo test --test vulkan -- --ignored"]
fn rasterizes_depth_and_reads_back_repeated_frames() {
    let mut gpu = VulkanRenderer::new().unwrap();
    let near = triangle(0.2, [0.0, 1.0, 0.0]);
    let far = triangle(0.8, [1.0, 0.0, 0.0]);
    let clear = [0.0, 0.0, 1.0, 1.0];
    let (w, h) = (65, 47);
    let a = gpu.render(w, h, clear, &[near, far].concat()).unwrap();
    let b = gpu.render(w, h, clear, &[far, near].concat()).unwrap();
    assert_eq!(
        a, b,
        "depth, not submission order, determines visible triangle"
    );
    let center = ((h / 2 * w + w / 2) * 4) as usize;
    assert_eq!(&a[center..center + 4], &[0, 255, 0, 255]);
    assert_eq!(&a[..4], &[0, 0, 255, 255]);
    let empty = gpu.render(17, 13, clear, &[]).unwrap();
    assert_eq!(empty.len(), 17 * 13 * 4);
    assert!(empty.chunks_exact(4).all(|p| p == [0, 0, 255, 255]));
    assert!(gpu.render(0, 12, clear, &[]).is_err());
    assert!(gpu.render(w, h, clear, &near[..2]).is_err());
}

#[test]
#[ignore = "requires a Vulkan graphics device"]
fn resident_vertices_match_cpu_projection_and_complete_benchmark_queries() {
    use ganbate_engine::{Camera, Scene, Vec3, vulkan::bench};
    let mut scene = Scene::default();
    scene.triangle(
        Vec3(-0.7, 0., 0.),
        Vec3(0.9, 0., 0.),
        Vec3(0., 1.2, 0.),
        [0.7, 0.4, 0.2],
    );
    let bounds = scene.bounds().unwrap();
    let mut gpu = VulkanRenderer::new().unwrap();
    for angle in [0.2, 0.65] {
        let cam = Camera::orbit(bounds, angle);
        let reference = gpu
            .render(91, 59, [0.; 4], &cam.vertices(&scene, 91, 59).unwrap())
            .unwrap();
        let projected = gpu.render_scene(91, 59, [0.; 4], &scene, cam).unwrap();
        assert!(reference.chunks_exact(4).filter(|px| px[0] > 0).count() > 100);
        let mismatch = projected
            .chunks_exact(4)
            .zip(reference.chunks_exact(4))
            .filter(|(a, b)| a != b)
            .count();
        assert!(mismatch <= 2, "projection differs in {mismatch} pixels");
    }
    let run = bench::run(&mut gpu, &scene, (91, 59), 3, 7, |i| {
        Camera::orbit(bounds, i as f32 * 0.05)
    })
    .unwrap();
    assert_eq!(run.cpu_ms.len(), 7);
    assert_eq!(run.fence_ms.len(), 7);
    assert!(run.gpu_ms.is_empty() || run.gpu_ms.len() == 7);
    assert!(run.seconds > 0.);
}

#[test]
#[ignore = "requires a Vulkan graphics device"]
fn mixed_scene_shares_depth_and_preserves_both_draws() {
    use ganbate_engine::{Camera, Scene, Vec3};
    let mut base = Scene::default();
    base.triangle(
        Vec3(-1., -1., 0.),
        Vec3(1., -1., 0.),
        Vec3(0., 1., 0.),
        [1., 0., 0.],
    );
    let mut overlay = Scene::default();
    overlay.triangle(
        Vec3(-0.4, -0.4, 0.4),
        Vec3(0.4, -0.4, 0.4),
        Vec3(0., 0.4, 0.4),
        [0., 1., 0.],
    );
    overlay.triangle(
        Vec3(-0.2, -0.2, -0.4),
        Vec3(0.2, -0.2, -0.4),
        Vec3(0., 0.2, -0.4),
        [0., 0., 1.],
    );
    let camera = Camera {
        eye: Vec3(0., 0., 3.),
        target: Vec3(0., 0., 0.),
        vertical_fov_degrees: 52.,
        near: 0.1,
        far: 20.,
    };
    let mut gpu = VulkanRenderer::new().unwrap();
    let pixels = gpu
        .render_with_overlay(100, 100, [0.; 4], &base, &overlay, camera)
        .unwrap();
    assert!(pixels.chunks_exact(4).any(|p| p[0] == 255));
    assert!(pixels.chunks_exact(4).any(|p| p[1] == 255));
    assert!(pixels.chunks_exact(4).all(|p| p[2] == 0));
}
