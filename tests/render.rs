//! Driver tests for the stylized renderer on a synthetic scene (run with `--ignored`; set
//! `ABE_VULKAN_VALIDATION=1` to also check the Vulkan validation layers).
use abe_engine::Vec3;
use abe_engine::gpu::Device;
use abe_engine::render::{Environment, Look, Output, Quality, Renderer, TimeOfDay, View, Weather};
use abe_engine::world::{
    AlphaMode, Emission, Instance, Material, Mesh, PointLight, Shading, Texture, Transform, World,
};

fn quad(y: f32, half: f32, material: u32) -> Mesh {
    let p = |x: f32, z: f32| Vec3(x, y, z);
    Mesh {
        positions: vec![
            p(-half, -half),
            p(half, -half),
            p(half, half),
            p(-half, half),
        ],
        normals: vec![Vec3(0.0, 1.0, 0.0); 4],
        uvs: vec![[0.0, 0.0], [half, 0.0], [half, half], [0.0, half]],
        indices: vec![0, 2, 1, 0, 3, 2],
        color: [0.5; 3],
        material: Some(material),
    }
}

fn cube(c: [f32; 3], s: f32, material: u32) -> Mesh {
    let mut m = Mesh {
        material: Some(material),
        ..Mesh::default()
    };
    let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
        ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]),
    ];
    for (n, u, v) in faces {
        let base = m.positions.len() as u32;
        for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            m.positions.push(Vec3(
                c[0] + s * (n[0] + a * u[0] + b * v[0]),
                c[1] + s * (n[1] + a * u[1] + b * v[1]),
                c[2] + s * (n[2] + a * u[2] + b * v[2]),
            ));
            m.normals.push(Vec3(n[0], n[1], n[2]));
            m.uvs.push([a * 0.5 + 0.5, b * 0.5 + 0.5]);
        }
        m.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    m
}

fn material(name: &str, color: [f32; 4], shading: Shading) -> Material {
    Material {
        name: name.into(),
        base_color: color,
        shading,
        double_sided: true,
        alpha: AlphaMode::Opaque,
        ..Material::default()
    }
}

fn world() -> World {
    let mut ground = material("ground", [0.35, 0.55, 0.25, 1.0], Shading::Standard);
    ground.base_color_texture = Some(0);
    let mut lamp = material("lamp", [1.0, 0.6, 0.3, 1.0], Shading::Standard);
    lamp.emissive = [3.0, 1.8, 0.7];
    lamp.emission = Emission::Lamp;
    let checker = vec![
        255, 255, 255, 255, 200, 200, 200, 255, 200, 200, 200, 255, 255, 255, 255, 255,
    ];
    let meshes = vec![
        quad(0.0, 12.0, 0),
        cube([0.0, 1.0, 0.0], 1.0, 1),
        quad(-0.3, 40.0, 2),
        cube([-2.5, 1.0, -1.5], 0.9, 3),
        cube([3.0, 1.6, 1.0], 0.12, 4),
        // An alpha-masked card: its shadow must use the texture's cut-out.
        Mesh {
            positions: vec![
                Vec3(-4.0, 0.0, 2.0),
                Vec3(-2.0, 0.0, 2.0),
                Vec3(-2.0, 2.0, 2.0),
                Vec3(-4.0, 2.0, 2.0),
            ],
            normals: vec![Vec3(0.0, 0.0, 1.0); 4],
            uvs: vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            indices: vec![0, 1, 2, 0, 2, 3],
            color: [0.5; 3],
            material: Some(5),
        },
    ];
    // The ground sits above the water and covers the middle; water surrounds it.
    let instances = (0..meshes.len() as u32)
        .map(|mesh| Instance {
            mesh,
            transform: Transform::IDENTITY,
        })
        .collect();
    World {
        meshes,
        instances,
        materials: vec![
            ground,
            material("box", [0.8, 0.15, 0.1, 1.0], Shading::Standard),
            material("water", [0.02, 0.1, 0.15, 1.0], Shading::Water),
            material("canopy", [0.9, 0.5, 0.6, 1.0], Shading::Foliage),
            lamp,
            {
                let mut card = material("card", [0.3, 0.6, 0.3, 1.0], Shading::Foliage);
                card.alpha = AlphaMode::Mask(0.5);
                card.base_color_texture = Some(1);
                card
            },
        ],
        textures: vec![
            Texture {
                width: 2,
                height: 2,
                srgb: true,
                levels: vec![checker, vec![227, 227, 227, 255]],
            },
            Texture {
                width: 2,
                height: 2,
                srgb: true,
                levels: vec![
                    vec![
                        255, 255, 255, 255, 255, 255, 255, 0, 255, 255, 255, 0, 255, 255, 255, 255,
                    ],
                    vec![255, 255, 255, 128],
                ],
            },
        ],
        lights: vec![PointLight {
            position: Vec3(3.0, 1.6, 1.0),
            color: [1.0, 0.6, 0.3],
            intensity: 30.0,
            range: 7.0,
        }],
    }
}

const W: u32 = 128;
const H: u32 = 72;

fn view() -> View {
    View {
        name: "test".into(),
        eye: [0.0, 4.0, 10.0],
        target: [0.0, 0.5, 0.0],
        fov: 50.0,
    }
}

fn render(device: &mut Device<'_>, r: &mut Renderer, hours: f32, weather: &str) -> Vec<u8> {
    let time = TimeOfDay {
        hours,
        ..TimeOfDay::default()
    };
    let weather = Weather::preset(weather).expect("preset");
    let env = Environment::resolve(&time, &weather, &r.look().palette, 3.0);
    r.render_still(device, &view(), &env, 1).expect("render")
}

fn luma(px: &[u8], x: u32, y: u32) -> f32 {
    let i = ((y * W + x) * 4) as usize;
    0.2126 * f32::from(px[i]) + 0.7152 * f32::from(px[i + 1]) + 0.0722 * f32::from(px[i + 2])
}

fn region(px: &[u8], x0: u32, y0: u32, x1: u32, y1: u32) -> f32 {
    let mut sum = 0.0;
    let mut n = 0.0;
    for y in y0..y1 {
        for x in x0..x1 {
            sum += luma(px, x, y);
            n += 1.0;
        }
    }
    sum / n
}

#[test]
#[ignore = "requires a Vulkan driver"]
fn time_of_day_weather_and_lamps() {
    let mut device = Device::headless().expect("device");
    let mut quality = Quality::realtime();
    quality.shadow_size = 512;
    let mut r = Renderer::new(
        &mut device,
        &world(),
        Look::default(),
        quality,
        (W, H),
        Output::Still,
    )
    .expect("renderer");
    assert_eq!(r.lights(), 1);
    assert!((r.water_level().unwrap() + 0.3).abs() < 1e-4);

    let noon = render(&mut device, &mut r, 12.0, "clear");
    // The sky at the top of the frame is blue and bright at noon.
    let i = ((2 * W + W / 2) * 4) as usize;
    assert!(noon[i + 2] > noon[i] + 20, "noon sky {:?}", &noon[i..i + 3]);
    assert!(region(&noon, 0, 0, W, 8) > 90.0);
    let midnight = render(&mut device, &mut r, 0.0, "clear");
    assert!(
        region(&midnight, 0, 0, W, 8) < region(&noon, 0, 0, W, 8) * 0.5,
        "midnight sky should be much darker"
    );
    // At midnight the lamp (right of centre, near the box) lights the ground around it.
    let near = region(&midnight, 84, 34, 104, 50);
    let far = region(&midnight, 8, 40, 28, 56);
    assert!(near > far + 8.0, "lamp-lit ground {near} vs unlit {far}");
    for weather in Weather::PRESETS {
        let px = render(&mut device, &mut r, 17.5, weather);
        assert!(region(&px, 0, 0, W, H) > 5.0, "{weather} rendered black");
    }
    let errors = device.validation_errors();
    assert!(errors.is_empty(), "validation: {errors:?}");
}

/// Mean absolute difference per channel between two RGBA8 images.
fn difference(a: &[u8], b: &[u8]) -> f32 {
    let sum: u64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| u64::from(x.abs_diff(*y)))
        .sum();
    sum as f32 / a.len() as f32
}

#[test]
#[ignore = "requires a Vulkan driver"]
fn rain_stops_under_a_roof() {
    // A camera under a wide roof in a storm sees no drops; in the open it does. Each case
    // compares the frame with and without particles.
    let mut device = Device::headless().expect("device");
    let (w, h) = (320, 180);
    let view = View {
        name: "under".into(),
        eye: [0.0, 1.5, 0.0],
        target: [0.0, 1.0, -10.0],
        fov: 60.0,
    };
    let mut frame = |roof: bool, particles: bool| {
        let mut meshes = vec![quad(0.0, 40.0, 0)];
        if roof {
            meshes.push(quad(3.0, 40.0, 1));
        }
        let world = World {
            instances: (0..meshes.len() as u32)
                .map(|mesh| Instance {
                    mesh,
                    transform: Transform::IDENTITY,
                })
                .collect(),
            meshes,
            materials: vec![
                material("ground", [0.4, 0.5, 0.3, 1.0], Shading::Standard),
                material("roof", [0.5, 0.3, 0.2, 1.0], Shading::Standard),
            ],
            textures: vec![],
            lights: vec![],
        };
        let mut quality = Quality::realtime();
        quality.shadow_size = 512;
        quality.particles = particles;
        let mut r = Renderer::new(
            &mut device,
            &world,
            Look::default(),
            quality,
            (w, h),
            Output::Still,
        )
        .expect("renderer");
        let time = TimeOfDay {
            hours: 12.0,
            ..TimeOfDay::default()
        };
        let storm = Weather::preset("storm").expect("preset");
        let env = Environment::resolve(&time, &storm, &r.look().palette, 2.0);
        r.render_still(&mut device, &view, &env, 1).expect("render")
    };
    let open = difference(&frame(false, true), &frame(false, false));
    let covered = difference(&frame(true, true), &frame(true, false));
    eprintln!("drops change the open frame by {open:.3}, the covered one by {covered:.4}");
    assert!(open > 0.2, "drops should show in the open: {open}");
    assert!(
        covered < open * 0.05,
        "drops under the roof: {covered} vs {open} in the open"
    );
    let errors = device.validation_errors();
    assert!(errors.is_empty(), "validation: {errors:?}");
}

#[test]
#[ignore = "requires a Vulkan driver"]
fn accumulated_stills_and_resize() {
    let mut device = Device::headless().expect("device");
    let mut quality = Quality::still();
    quality.shadow_size = 512;
    let mut r = Renderer::new(
        &mut device,
        &world(),
        Look::default(),
        quality,
        (W, H),
        Output::Still,
    )
    .expect("renderer");
    let env = Environment::resolve(
        &TimeOfDay::default(),
        &Weather::default(),
        &r.look().palette,
        1.0,
    );
    let a = r
        .render_still(&mut device, &view(), &env, 4)
        .expect("four samples");
    assert_eq!(a.len(), (W * H * 4) as usize);
    r.resize(&mut device, (W / 2, H / 2)).expect("resize");
    let b = r
        .render_still(&mut device, &view(), &env, 1)
        .expect("after resize");
    assert_eq!(b.len(), (W * H) as usize);
    let errors = device.validation_errors();
    assert!(errors.is_empty(), "validation: {errors:?}");
}
