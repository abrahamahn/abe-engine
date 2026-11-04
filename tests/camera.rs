use ganbate_engine::{Camera, Scene, Vec3};

fn camera() -> Camera {
    Camera {
        eye: Vec3(0.0, 0.0, 0.0),
        target: Vec3(0.0, 0.0, -1.0),
        vertical_fov_degrees: 90.0,
        near: 1.0,
        far: 9.0,
    }
}
#[test]
fn camera_maps_to_vulkan_depth_and_aspect() {
    let mut s = Scene::default();
    s.triangle(
        Vec3(0.0, 0.0, -1.0),
        Vec3(1.0, 1.0, -2.0),
        Vec3(0.0, 0.0, -9.0),
        [0.2, 0.3, 0.4],
    );
    let vs = camera().vertices(&s, 200, 100).unwrap();
    assert_eq!(vs[0].clip, [0.0, 0.0, 0.0, 1.0]);
    assert!((vs[1].clip[0] - 0.5).abs() < 0.00001);
    assert!((vs[1].clip[1] + 1.0).abs() < 0.00001);
    assert!((vs[2].clip[2] / vs[2].clip[3] - 1.0).abs() < 0.00001);
    assert_eq!(vs[1].rgba, [0.2, 0.3, 0.4, 1.0]);
}
#[test]
fn invalid_frustum_and_singular_camera_are_rejected() {
    let scene = Scene::default();
    assert!(camera().vertices(&scene, 0, 1).is_err());
    assert!(
        Camera {
            near: 9.0,
            ..camera()
        }
        .vertices(&scene, 1, 1)
        .is_err()
    );
    assert!(
        Camera {
            target: Vec3(0.0, 1.0, 0.0),
            ..camera()
        }
        .vertices(&scene, 1, 1)
        .is_err()
    );
    assert!(
        Camera {
            vertical_fov_degrees: f32::NAN,
            ..camera()
        }
        .vertices(&scene, 1, 1)
        .is_err()
    );
}
