//! Driver tests for the stylized renderer on a synthetic scene (run with `--ignored`; set
//! `ABE_VULKAN_VALIDATION=1` to also check the Vulkan validation layers).
use abe_engine::Vec3;
use abe_engine::gpu::Device;
use abe_engine::render::{
    DetailPreset, DetailRule, Environment, Look, MaterialDesc, MeshData, Output, Quality, Renderer,
    Terrain, TimeOfDay, View, Weather,
};
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
        ..View::default()
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
    // The sky at the top of the frame is blue and bright at noon (on average: a few clouds).
    let channel = |c: usize| -> f32 {
        let top = &noon[..(8 * W * 4) as usize];
        top.chunks(4).map(|p| f32::from(p[c])).sum::<f32>() / (8 * W) as f32
    };
    let (red, blue) = (channel(0), channel(2));
    assert!(blue > red + 15.0, "noon sky red {red} blue {blue}");
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
        ..View::default()
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
fn grass_and_surface_detail() {
    // A lawn with a tiled roof slab over part of it, seen from just above the grass.
    let mut device = Device::headless().expect("device");
    let (w, h) = (192, 108);
    let roof = Mesh {
        positions: vec![
            Vec3(-3.0, 2.0, -9.0),
            Vec3(3.0, 2.0, -9.0),
            Vec3(3.0, 3.5, -12.0),
            Vec3(-3.0, 3.5, -12.0),
        ],
        normals: vec![Vec3(0.0, 0.894, 0.447); 4],
        uvs: vec![[0.0, 0.0]; 4],
        indices: vec![0, 1, 2, 0, 2, 3],
        color: [0.5; 3],
        material: Some(1),
    };
    let meshes = vec![quad(0.0, 30.0, 0), roof];
    let world = World {
        instances: (0..meshes.len() as u32)
            .map(|mesh| Instance {
                mesh,
                transform: Transform::IDENTITY,
            })
            .collect(),
        meshes,
        materials: vec![
            material("lawn", [0.2, 0.3, 0.1, 1.0], Shading::Standard),
            material("roof-tile", [0.1, 0.15, 0.3, 1.0], Shading::Standard),
        ],
        textures: vec![],
        lights: vec![],
    };
    let view = View {
        name: "meadow".into(),
        eye: [0.0, 1.2, 0.0],
        target: [0.0, 1.0, -10.0],
        fov: 60.0,
        ..View::default()
    };
    let rule = |pattern: &str, preset| DetailRule {
        patterns: vec![pattern.into()],
        preset,
        scale: [0.3, 0.34],
        strength: 1.0,
        variation: 0.1,
        color: None,
        gloss: 0.0,
    };
    let mut frame = |grass: f32, details: bool| {
        let mut look = Look::default();
        look.grass.density = grass;
        if details {
            look.details = vec![
                rule("lawn", DetailPreset::Grass),
                rule("roof-*", DetailPreset::RoofTiles),
            ];
        }
        let mut quality = Quality::realtime();
        quality.shadow_size = 512;
        quality.ground_size = 256;
        let mut r = Renderer::new(&mut device, &world, look, quality, (w, h), Output::Still)
            .expect("renderer");
        let time = TimeOfDay {
            hours: 12.0,
            ..TimeOfDay::default()
        };
        let env = Environment::resolve(&time, &Weather::default(), &r.look().palette, 2.0);
        r.render_still(&mut device, &view, &env, 1).expect("render")
    };
    let plain = frame(0.0, false);
    let detailed = frame(0.0, true);
    let grassy = frame(1.0, true);
    // Roof tiles and grassy ground change the picture; grass blades change it again.
    let detail = difference(&plain, &detailed);
    let blades = difference(&detailed, &grassy);
    eprintln!("detail changes the frame by {detail:.2}, grass blades by {blades:.2}");
    assert!(detail > 1.0, "surface detail had no effect: {detail}");
    assert!(blades > 1.0, "no grass blades: {blades}");
    let errors = device.validation_errors();
    assert!(errors.is_empty(), "validation: {errors:?}");
}

#[test]
#[ignore = "requires a Vulkan driver"]
fn colour_rules_recolour_any_material() {
    // A colour-only rule (preset `none`) recolours foliage, which takes no surface detail.
    let mut device = Device::headless().expect("device");
    let (w, h) = (96, 54);
    let meshes = vec![quad(0.0, 30.0, 0), cube([0.0, 1.5, -6.0], 1.5, 1)];
    let world = World {
        instances: (0..meshes.len() as u32)
            .map(|mesh| Instance {
                mesh,
                transform: Transform::IDENTITY,
            })
            .collect(),
        meshes,
        materials: vec![
            material("ground", [0.3, 0.3, 0.3, 1.0], Shading::Standard),
            material("leaf-1", [0.1, 0.3, 0.25, 1.0], Shading::Foliage),
        ],
        textures: vec![],
        lights: vec![],
    };
    let view = View {
        name: "tree".into(),
        eye: [0.0, 2.0, 2.0],
        target: [0.0, 1.5, -6.0],
        fov: 50.0,
        ..View::default()
    };
    let mut frame = |rules: Vec<DetailRule>| {
        let look = Look {
            details: rules,
            ..Look::default()
        };
        let mut quality = Quality::realtime();
        quality.shadow_size = 512;
        quality.ground_size = 256;
        quality.world_size = 256;
        let mut r = Renderer::new(&mut device, &world, look, quality, (w, h), Output::Still)
            .expect("renderer");
        let env = Environment::resolve(
            &TimeOfDay::default(),
            &Weather::default(),
            &r.look().palette,
            2.0,
        );
        r.render_still(&mut device, &view, &env, 1).expect("render")
    };
    let red = |image: &[u8]| {
        image
            .chunks(4)
            .filter(|p| u32::from(p[0]) > 2 * u32::from(p[1]) + 20)
            .count()
    };
    let plain = frame(vec![]);
    let recoloured = frame(vec![DetailRule {
        patterns: vec!["leaf-*".into()],
        preset: DetailPreset::None,
        scale: [1.0, 1.0],
        strength: 1.0,
        variation: 0.0,
        color: Some([0.8, 0.05, 0.05]),
        gloss: 0.0,
    }]);
    let (before, after) = (red(&plain), red(&recoloured));
    eprintln!("red pixels: {before} before, {after} with the rule");
    assert!(
        before == 0 && after > 300,
        "foliage not recoloured: {before} / {after}"
    );
}

#[test]
#[ignore = "requires a Vulkan driver"]
fn lattice_bars_shade_a_lamp() {
    // A glowing paper box; a lattice rule draws dark bars across it, glow included.
    let mut device = Device::headless().expect("device");
    let (w, h) = (96, 54);
    let mut paper = material("paper-lamp", [0.9, 0.85, 0.7, 1.0], Shading::Standard);
    paper.emissive = [2.0, 1.6, 1.0];
    let meshes = vec![quad(0.0, 30.0, 0), cube([0.0, 1.0, -4.0], 0.8, 1)];
    let world = World {
        instances: (0..meshes.len() as u32)
            .map(|mesh| Instance {
                mesh,
                transform: Transform::IDENTITY,
            })
            .collect(),
        meshes,
        materials: vec![
            material("ground", [0.2, 0.2, 0.2, 1.0], Shading::Standard),
            paper,
        ],
        textures: vec![],
        lights: vec![],
    };
    let view = View {
        name: "lamp".into(),
        eye: [0.0, 1.2, -1.0],
        target: [0.0, 1.0, -4.0],
        fov: 50.0,
        ..View::default()
    };
    let mut frame = |rules: Vec<DetailRule>| {
        let look = Look {
            details: rules,
            ..Look::default()
        };
        let mut quality = Quality::realtime();
        quality.shadow_size = 512;
        quality.ground_size = 256;
        quality.world_size = 256;
        let mut r = Renderer::new(&mut device, &world, look, quality, (w, h), Output::Still)
            .expect("renderer");
        let time = TimeOfDay {
            hours: 22.0,
            ..TimeOfDay::default()
        };
        let env = Environment::resolve(&time, &Weather::default(), &r.look().palette, 2.0);
        r.render_still(&mut device, &view, &env, 1).expect("render")
    };
    let bright = |image: &[u8]| image.chunks(4).filter(|p| p[0] > 200 && p[1] > 160).count();
    let plain = frame(vec![]);
    let barred = frame(vec![DetailRule {
        patterns: vec!["paper-*".into()],
        preset: DetailPreset::Lattice,
        scale: [0.2, 0.2],
        strength: 0.25,
        variation: 0.0,
        color: None,
        gloss: 0.0,
    }]);
    let ribbed = frame(vec![DetailRule {
        patterns: vec!["paper-*".into()],
        preset: DetailPreset::Lattice,
        scale: [0.0, 0.2],
        strength: 0.25,
        variation: 0.0,
        color: None,
        gloss: 0.0,
    }]);
    let (before, after, ribs) = (bright(&plain), bright(&barred), bright(&ribbed));
    eprintln!("bright pixels: {before} plain, {after} with a lattice, {ribs} with ribs only");
    assert!(
        before > 500 && after < before * 9 / 10,
        "no bars: {before} / {after}"
    );
    // Ribs alone (no bars across) cover less than the full lattice, but still some.
    assert!(
        ribs > after && ribs < before,
        "ribs: {before} / {ribs} / {after}"
    );
}

#[test]
#[ignore = "requires a Vulkan driver"]
fn tide_marks_stain_what_stands_in_water() {
    // A pale block standing in a sea at 0 m: with tide marks on, its foot is darker.
    let mut device = Device::headless().expect("device");
    let (w, h) = (96, 54);
    let meshes = vec![quad(0.0, 200.0, 0), cube([0.0, 0.5, -4.0], 1.0, 1)];
    let world = World {
        instances: (0..meshes.len() as u32)
            .map(|mesh| Instance {
                mesh,
                transform: Transform::IDENTITY,
            })
            .collect(),
        meshes,
        materials: vec![
            material("sea", [0.1, 0.2, 0.3, 1.0], Shading::Water),
            material("pier", [0.8, 0.8, 0.8, 1.0], Shading::Standard),
        ],
        textures: vec![],
        lights: vec![],
    };
    let view = View {
        name: "pier".into(),
        eye: [0.0, 0.8, -1.0],
        target: [0.0, 0.4, -4.0],
        fov: 50.0,
        ..View::default()
    };
    let mut frame = |tide: f32| {
        let look = Look {
            tide,
            tide_color: [0.2, 0.25, 0.15],
            ..Look::default()
        };
        let mut quality = Quality::realtime();
        quality.shadow_size = 512;
        quality.ground_size = 256;
        quality.world_size = 256;
        let mut r = Renderer::new(&mut device, &world, look, quality, (w, h), Output::Still)
            .expect("renderer");
        let time = TimeOfDay {
            hours: 12.0,
            ..TimeOfDay::default()
        };
        let env = Environment::resolve(&time, &Weather::default(), &r.look().palette, 2.0);
        r.render_still(&mut device, &view, &env, 1).expect("render")
    };
    let luma = |image: &[u8]| image.chunks(4).map(|p| u64::from(p[1])).sum::<u64>();
    let (dry, marked) = (luma(&frame(0.0)), luma(&frame(0.8)));
    eprintln!("green sum: {dry} without tide marks, {marked} with");
    assert!(marked * 100 < dry * 97, "no tide marks: {dry} / {marked}");
}

/// Land sloping from 3 m down to a seabed 4 m under a sea at 0 m: flat to z = -60, the water
/// line at z = -10, 1.4 m deep at z = 13.
fn shore() -> World {
    let mut land = Mesh {
        material: Some(0),
        ..Mesh::default()
    };
    let height = |z: f32| (3.0 - (z + 60.0) * 0.06).clamp(-4.0, 3.0);
    let (cols, rows) = (31u32, 81u32);
    for j in 0..rows {
        for i in 0..cols {
            let (x, z) = (-60.0 + 4.0 * i as f32, -100.0 + 2.0 * j as f32);
            land.positions.push(Vec3(x, height(z), z));
            // d(height)/dz = -0.06 on the slope.
            let n = if (-60.0..56.0).contains(&z) {
                0.06
            } else {
                0.0
            };
            let l = (1.0f32 + n * n).sqrt();
            land.normals.push(Vec3(0.0, 1.0 / l, n / l));
            land.uvs.push([0.0, 0.0]);
        }
    }
    for j in 0..rows - 1 {
        for i in 0..cols - 1 {
            let k = j * cols + i;
            land.indices
                .extend([k, k + cols, k + 1, k + 1, k + cols, k + cols + 1]);
        }
    }
    let meshes = vec![land, quad(0.0, 400.0, 1)];
    World {
        instances: (0..meshes.len() as u32)
            .map(|mesh| Instance {
                mesh,
                transform: Transform::IDENTITY,
            })
            .collect(),
        meshes,
        materials: vec![
            material("land", [0.2, 0.3, 0.1, 1.0], Shading::Standard),
            material("sea", [0.1, 0.2, 0.3, 1.0], Shading::Water),
        ],
        textures: vec![],
        lights: vec![],
    }
}

#[test]
#[ignore = "requires a Vulkan driver"]
fn terrain_beaches_and_distant_shallows() {
    // Terrain turns to sand above the water line, and 200 m away, well past the ground map,
    // the world map still tints the shallows (red here, so they can be counted).
    let mut device = Device::headless().expect("device");
    let (w, h) = (256, 144);
    let world = shore();
    let rule = |preset| DetailRule {
        patterns: vec!["land".into()],
        preset,
        scale: [0.3, 0.3],
        strength: 1.0,
        variation: 0.1,
        color: None,
        gloss: 0.0,
    };
    let mut frame = |preset, view: &View, shallow| {
        let look = Look {
            details: vec![rule(preset)],
            fog: 0.0,
            water_reflection: 0.0,
            water_shallow: shallow,
            terrain: Terrain {
                sand: [0.2, 0.2, 0.9],
                wet_sand: [0.1, 0.1, 0.6],
                ..Terrain::default()
            },
            ..Look::default()
        };
        let mut quality = Quality::realtime();
        quality.shadow_size = 512;
        quality.ground_size = 256;
        quality.world_size = 512;
        let mut r = Renderer::new(&mut device, &world, look, quality, (w, h), Output::Still)
            .expect("renderer");
        let time = TimeOfDay {
            hours: 12.0,
            ..TimeOfDay::default()
        };
        let env = Environment::resolve(&time, &Weather::default(), &r.look().palette, 2.0);
        r.render_still(&mut device, view, &env, 1).expect("render")
    };
    let count = |image: &[u8], test: &dyn Fn(f32, f32, f32) -> bool| {
        image
            .chunks(4)
            .filter(|p| test(f32::from(p[0]), f32::from(p[1]), f32::from(p[2])))
            .count()
    };
    // Sand (blue here) along the beach, seen from just above it.
    let near = View {
        name: "beach".into(),
        eye: [0.0, 10.0, 10.0],
        target: [0.0, 0.0, -20.0],
        fov: 60.0,
        ..View::default()
    };
    let blue = |r: f32, g: f32, b: f32| b > 1.5 * r + 10.0 && b > 1.5 * g + 10.0;
    let red_shallows = [1.0, 0.0, 0.0];
    let grassy = count(&frame(DetailPreset::Grass, &near, red_shallows), &blue);
    let sandy = count(&frame(DetailPreset::Terrain, &near, red_shallows), &blue);
    // Red shallows along the far shore.
    let far = View {
        name: "offshore".into(),
        eye: [0.0, 25.0, 190.0],
        target: [0.0, 0.0, 0.0],
        fov: 40.0,
        ..View::default()
    };
    let red = |r: f32, g: f32, b: f32| r > 1.4 * g + 8.0 && r > 1.4 * b + 8.0;
    let shallows = count(&frame(DetailPreset::Terrain, &far, red_shallows), &red);
    let plain = count(
        &frame(DetailPreset::Terrain, &far, Look::default().water_shallow),
        &red,
    );
    eprintln!(
        "sand pixels: {grassy} with grass, {sandy} with terrain; \
         far red pixels: {shallows} with red shallows, {plain} without"
    );
    assert!(sandy > grassy + 1000, "no beach: {grassy} / {sandy}");
    assert!(
        shallows > 40 && plain < 5,
        "no shallows past the ground map: {shallows} / {plain}"
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

/// Props: a run-time mesh drawn twice from one batch, moved, recoloured, hidden, picked and
/// projected, casting a shadow on the world. A prop renders like the same mesh cooked into the
/// world.
#[test]
#[ignore = "requires a Vulkan driver"]
fn props_draw_move_pick_and_cast_shadows() {
    // Late afternoon: long shadows.
    const HOURS: f32 = 16.5;
    let mut device = Device::headless().expect("device");
    let mut quality = Quality::realtime();
    quality.shadow_size = 512;
    quality.ground_size = 256;
    quality.world_size = 256;
    let ground = material("ground", [0.6, 0.6, 0.6, 1.0], Shading::Standard);
    let red = MaterialDesc {
        name: "prop-red".into(),
        base_color: [0.9, 0.05, 0.05, 1.0],
        ..MaterialDesc::default()
    };
    let new = |device: &mut Device<'_>, world: &World| {
        Renderer::new(
            device,
            world,
            Look::default(),
            quality,
            (W, H),
            Output::Still,
        )
        .expect("renderer")
    };
    let instance = |mesh| Instance {
        mesh,
        transform: Transform::IDENTITY,
    };
    // The reference: the cube cooked into the world at x = 2.
    let mut cooked_red = material("prop-red", red.base_color, Shading::Standard);
    cooked_red.metallic = red.metallic;
    cooked_red.roughness = red.roughness;
    let mut reference = new(
        &mut device,
        &World {
            instances: vec![instance(0), instance(1)],
            meshes: vec![quad(0.0, 20.0, 0), cube([2.0, 0.5, 0.0], 0.5, 1)],
            materials: vec![ground.clone(), cooked_red],
            textures: vec![],
            lights: vec![],
        },
    );
    let cooked = render(&mut device, &mut reference, HOURS, "clear");
    let mut r = new(
        &mut device,
        &World {
            instances: vec![instance(0)],
            meshes: vec![quad(0.0, 20.0, 0)],
            materials: vec![ground],
            textures: vec![],
            lights: vec![],
        },
    );
    // A unit cube in model space.
    let c = cube([0.0, 0.0, 0.0], 0.5, 0);
    let positions: Vec<[f32; 3]> = c.positions.iter().map(|p| [p.0, p.1, p.2]).collect();
    let normals: Vec<[f32; 3]> = c.normals.iter().map(|n| [n.0, n.1, n.2]).collect();
    let mesh = r
        .create_prop_mesh(
            &mut device,
            &MeshData {
                positions: &positions,
                normals: &normals,
                uvs: &c.uvs,
                indices: &c.indices,
            },
        )
        .expect("mesh");
    let red = r.create_prop_material(&mut device, &red).expect("material");
    let at = |x: f32, y: f32, z: f32| {
        let mut m = [0.0f32; 16];
        m[0] = 1.0;
        m[5] = 1.0;
        m[10] = 1.0;
        m[15] = 1.0;
        m[12] = x;
        m[13] = y;
        m[14] = z;
        m
    };
    let right = r
        .create_prop_instance(mesh, red, &at(2.0, 0.5, 0.0))
        .expect("right");
    // Pixels that differ visibly between two frames.
    let differing = |a: &[u8], b: &[u8]| {
        a.chunks(4)
            .zip(b.chunks(4))
            .filter(|(p, q)| (0..3).any(|c| p[c].abs_diff(q[c]) > 12))
            .count()
    };
    let alone = render(&mut device, &mut r, HOURS, "clear");
    let off = differing(&alone, &cooked);
    // Without its shadow the prop no longer matches: the shadow pass draws it.
    let mut unshadowed = r.prop_material(red).expect("desc").clone();
    unshadowed.casts_shadow = false;
    r.update_prop_material(&mut device, red, &unshadowed)
        .expect("no shadow");
    let missing = differing(&render(&mut device, &mut r, HOURS, "clear"), &cooked);
    eprintln!(
        "pixels unlike the cooked cube: {off} as a prop, {missing} without its shadow \
         (mean difference {:.3})",
        difference(&alone, &cooked)
    );
    assert!(
        off <= 8,
        "a prop renders like the cooked mesh: {off} pixels differ"
    );
    assert!(missing > off + 20, "the prop's shadow: {missing} vs {off}");
    unshadowed.casts_shadow = true;
    r.update_prop_material(&mut device, red, &unshadowed)
        .expect("shadow again");
    let left = r
        .create_prop_instance(mesh, red, &at(-2.0, 0.5, 0.0))
        .expect("left");
    // Red-dominant pixels (in the bright afternoon light the cube reads as a warm pink).
    let reddish = |px: &[u8], x0: u32, x1: u32| {
        let mut n = 0;
        for y in 0..H {
            for x in x0..x1 {
                let i = ((y * W + x) * 4) as usize;
                let (r, g, b) = (px[i], px[i + 1], px[i + 2]);
                if u32::from(r) > u32::from(g) + 40 && u32::from(r) > u32::from(b) + 40 {
                    n += 1;
                }
            }
        }
        n
    };
    let both = render(&mut device, &mut r, HOURS, "clear");
    let (l, rr) = (reddish(&both, 0, W / 2), reddish(&both, W / 2, W));
    eprintln!("red pixels: {l} left, {rr} right");
    assert!(l > 30 && rr > 30, "both instances drawn: {l} / {rr}");
    // Projection lands on the cube, and picking there finds it.
    let size = (W, H);
    let p = r.project(&view(), size, [2.0, 0.5, 0.0]).expect("in front");
    assert!(
        p[0] > W as f32 / 2.0 && p[1] > 0.0 && p[1] < H as f32,
        "{p:?}"
    );
    // Up in the world is up in the image, and the projected centre is on the drawn cube.
    let top = r.project(&view(), size, [2.0, 1.0, 0.0]).expect("in front");
    assert!(top[1] < p[1], "image rows run down: {top:?} {p:?}");
    let i = ((p[1] as u32 * W + p[0] as u32) * 4) as usize;
    assert!(
        u32::from(both[i]) > u32::from(both[i + 1]) + 40,
        "the cube is under its projection: {:?}",
        &both[i..i + 4]
    );
    let hit = r.pick(&view(), size, (p[0], p[1])).expect("a pick");
    assert_eq!(hit.instance, right);
    assert!(
        (hit.point[0] - 2.0).abs() < 0.6 && hit.distance > 5.0,
        "{hit:?}"
    );
    assert!(
        r.pick(&view(), size, (2.0, 2.0)).is_none(),
        "sky has no prop"
    );
    // Hide one, move the other far left: the right half has no red.
    r.set_prop_instance(right, &at(2.0, 0.5, 0.0), false)
        .expect("hide");
    r.set_prop_instance(left, &at(-3.0, 0.5, 0.0), true)
        .expect("move");
    let moved = render(&mut device, &mut r, HOURS, "clear");
    let (l2, r2) = (reddish(&moved, 0, W / 2), reddish(&moved, W / 2, W));
    eprintln!("after hiding and moving: {l2} left, {r2} right");
    assert!(l2 > 30 && r2 == 0, "{l2} / {r2}");
    r.destroy_prop_instance(left);
    assert_eq!(r.prop_instances(), 1);
    // Recolour through the material: the hidden instance stays hidden, so show it again.
    let mut blue = r.prop_material(red).expect("desc").clone();
    blue.base_color = [0.05, 0.1, 0.9, 1.0];
    r.update_prop_material(&mut device, red, &blue)
        .expect("update");
    r.set_prop_instance(right, &at(2.0, 0.5, 0.0), true)
        .expect("show");
    let recoloured = render(&mut device, &mut r, HOURS, "clear");
    assert_eq!(
        reddish(&recoloured, 0, W),
        0,
        "no red after the material changed"
    );
    r.destroy_props(&mut device);
    assert_eq!(r.prop_instances(), 0);
    assert!(r.pick(&view(), size, (p[0], p[1])).is_none());
    let errors = device.validation_errors();
    assert!(errors.is_empty(), "validation: {errors:?}");
}

/// Lens shift translates the image without rotating the camera, and a camera can look straight
/// down when it names another up direction.
#[test]
#[ignore = "requires a Vulkan driver"]
fn lens_shift_and_looking_straight_down() {
    let mut device = Device::headless().expect("device");
    let mut box_ = material("box", [0.9, 0.05, 0.05, 1.0], Shading::Standard);
    box_.metallic = 0.0;
    let world = World {
        instances: (0..2)
            .map(|mesh| Instance {
                mesh,
                transform: Transform::IDENTITY,
            })
            .collect(),
        meshes: vec![quad(0.0, 20.0, 0), cube([0.0, 0.5, 0.0], 0.5, 1)],
        materials: vec![
            material("ground", [0.6, 0.6, 0.6, 1.0], Shading::Standard),
            box_,
        ],
        textures: vec![],
        lights: vec![],
    };
    let mut quality = Quality::realtime();
    quality.shadow_size = 512;
    let mut r = Renderer::new(
        &mut device,
        &world,
        Look::default(),
        quality,
        (W, H),
        Output::Still,
    )
    .expect("renderer");
    let env = Environment::resolve(
        &TimeOfDay {
            hours: 15.0,
            ..TimeOfDay::default()
        },
        &Weather::default(),
        &r.look().palette,
        3.0,
    );
    // The centroid of red-dominant pixels.
    let red = |px: &[u8]| {
        let (mut sx, mut sy, mut n) = (0.0, 0.0, 0.0);
        for y in 0..H {
            for x in 0..W {
                let i = ((y * W + x) * 4) as usize;
                if u32::from(px[i]) > u32::from(px[i + 1]) + 40 {
                    sx += x as f32 + 0.5;
                    sy += y as f32 + 0.5;
                    n += 1.0;
                }
            }
        }
        assert!(n > 10.0, "the box is in view");
        [sx / n, sy / n]
    };
    let size = (W, H);
    let plain = view();
    let base = red(&r
        .render_still(&mut device, &plain, &env, 1)
        .expect("render"));
    for shift in [[0.0, 0.2], [0.3, 0.0], [-0.25, -0.15]] {
        let shifted = View { shift, ..view() };
        // The view axis lands `shift` from the centre (y up), whatever the depth.
        let target = r.project(&shifted, size, shifted.target).expect("in front");
        let want = [
            W as f32 * 0.5 * (1.0 + shift[0]),
            H as f32 * 0.5 * (1.0 - shift[1]),
        ];
        assert!(
            (target[0] - want[0]).abs() < 0.01 && (target[1] - want[1]).abs() < 0.01,
            "{shift:?}: {target:?} vs {want:?}"
        );
        let moved = red(&r
            .render_still(&mut device, &shifted, &env, 1)
            .expect("render"));
        let (dx, dy) = (moved[0] - base[0], moved[1] - base[1]);
        let (wx, wy) = (shift[0] * W as f32 * 0.5, -shift[1] * H as f32 * 0.5);
        eprintln!("shift {shift:?}: the box moved {dx:.2}, {dy:.2} px (want {wx:.2}, {wy:.2})");
        assert!((dx - wx).abs() < 1.0 && (dy - wy).abs() < 1.0);
    }
    // Straight down, with north (-z) at the top of the image.
    let down = View {
        eye: [0.0, 12.0, 0.0],
        target: [0.0, 0.0, 0.0],
        up: [0.0, 0.0, -1.0],
        ..view()
    };
    let top = red(&r.render_still(&mut device, &down, &env, 1).expect("render"));
    assert!(
        (top[0] - W as f32 * 0.5).abs() < 2.0 && (top[1] - H as f32 * 0.5).abs() < 2.0,
        "{top:?}"
    );
    let north = r.project(&down, size, [0.0, 0.0, -3.0]).expect("in front");
    let east = r.project(&down, size, [3.0, 0.0, 0.0]).expect("in front");
    assert!(
        north[1] < H as f32 * 0.5 && east[0] > W as f32 * 0.5,
        "{north:?} {east:?}"
    );
    // The same view with the default up (parallel to the line of sight) still renders.
    let degenerate = View {
        up: [0.0, 1.0, 0.0],
        ..down.clone()
    };
    let px = r
        .render_still(&mut device, &degenerate, &env, 1)
        .expect("render");
    assert!(region(&px, 0, 0, W, H) > 20.0, "not black");
    let errors = device.validation_errors();
    assert!(errors.is_empty(), "validation: {errors:?}");
}

/// A renderer without a cooked world draws the sky, and props on their own. Prop bounds widen
/// the shadow depth range: a prop floor 30 m above the (placeholder) world bounds still takes
/// a prop cube's shadow.
#[test]
#[ignore = "requires a Vulkan driver"]
fn empty_world_with_props() {
    let mut device = Device::headless().expect("device");
    let empty = World {
        instances: vec![],
        meshes: vec![],
        materials: vec![],
        textures: vec![],
        lights: vec![],
    };
    let mut quality = Quality::realtime();
    quality.shadow_size = 512;
    let mut r = Renderer::new(
        &mut device,
        &empty,
        Look::default(),
        quality,
        (W, H),
        Output::Still,
    )
    .expect("renderer");
    let sky = render(&mut device, &mut r, 12.0, "clear");
    assert!(region(&sky, 0, 0, W, H) > 40.0, "the sky is drawn");
    let mesh_of = |device: &mut Device<'_>, r: &mut Renderer, m: &Mesh| {
        let positions: Vec<[f32; 3]> = m.positions.iter().map(|p| [p.0, p.1, p.2]).collect();
        r.create_prop_mesh(
            device,
            &MeshData {
                positions: &positions,
                normals: &[],
                uvs: &[],
                indices: &m.indices,
            },
        )
        .expect("mesh")
    };
    let cube_mesh = mesh_of(&mut device, &mut r, &cube([0.0, 0.0, 0.0], 0.5, 0));
    let floor_mesh = mesh_of(&mut device, &mut r, &quad(0.0, 6.0, 0));
    let mut red = MaterialDesc {
        name: "red".into(),
        base_color: [0.9, 0.05, 0.05, 1.0],
        ..MaterialDesc::default()
    };
    let red_id = r.create_prop_material(&mut device, &red).expect("red");
    let grey = r
        .create_prop_material(
            &mut device,
            &MaterialDesc {
                name: "floor".into(),
                base_color: [0.6, 0.6, 0.6, 1.0],
                ..MaterialDesc::default()
            },
        )
        .expect("grey");
    let at = |y: f32| {
        let mut m = [0.0f32; 16];
        for i in [0, 5, 10, 15] {
            m[i] = 1.0;
        }
        m[13] = y;
        m
    };
    let cube = r
        .create_prop_instance(cube_mesh, red_id, &at(0.5))
        .expect("cube");
    let with = render(&mut device, &mut r, 12.0, "clear");
    let i = (((H / 2) * W + W / 2) * 4) as usize;
    assert!(
        u32::from(with[i]) > u32::from(with[i + 1]) + 40,
        "the prop is drawn: {:?}",
        &with[i..i + 4]
    );
    r.set_prop_instance(cube, &at(33.0), true).expect("raise");
    r.create_prop_instance(floor_mesh, grey, &at(30.0))
        .expect("floor");
    let lit = |device: &mut Device<'_>, r: &mut Renderer| {
        let v = View {
            eye: [0.0, 37.0, 9.0],
            target: [0.0, 30.5, 0.0],
            ..view()
        };
        let time = TimeOfDay {
            hours: 15.5,
            ..TimeOfDay::default()
        };
        let env = Environment::resolve(
            &time,
            &Weather::preset("clear").expect("clear"),
            &r.look().palette,
            3.0,
        );
        r.render_still(device, &v, &env, 1).expect("render")
    };
    let shadowed = lit(&mut device, &mut r);
    red.casts_shadow = false;
    r.update_prop_material(&mut device, red_id, &red)
        .expect("no shadow");
    let unshadowed = lit(&mut device, &mut r);
    let darker = shadowed
        .chunks(4)
        .zip(unshadowed.chunks(4))
        .filter(|(a, b)| u32::from(a[1]) + 12 < u32::from(b[1]))
        .count();
    eprintln!("pixels darkened by the prop's shadow on the prop floor: {darker}");
    assert!(darker > 25, "the cube shades the floor: {darker}");
    let errors = device.validation_errors();
    assert!(errors.is_empty(), "validation: {errors:?}");
}
