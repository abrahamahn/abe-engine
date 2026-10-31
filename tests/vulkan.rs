#![cfg(any(target_os = "linux", target_os = "windows", target_os = "android"))]
use abe_engine::{Vertex, vulkan::VulkanRenderer};

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
