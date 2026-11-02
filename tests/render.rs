//! Driver tests for the stylized renderer on a synthetic scene (run with `--ignored`; set
//! `ABE_VULKAN_VALIDATION=1` to also check the Vulkan validation layers).
use abe_engine::Vec3;
use abe_engine::gpu::Device;
use abe_engine::render::{
    DetailPreset, DetailRule, Environment, Look, Output, Quality, Renderer, Terrain, TimeOfDay,
    View, Weather,
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
