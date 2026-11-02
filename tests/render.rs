//! Driver tests for the stylized renderer on a synthetic scene (run with `--ignored`; set
//! `ABE_VULKAN_VALIDATION=1` to also check the Vulkan validation layers).
use abe_engine::Vec3;
use abe_engine::gpu::Device;
use abe_engine::render::{
    DetailPreset, DetailRule, Environment, LightDesc, Look, MaterialDesc, MeshData, Output,
    Quality, Renderer, SpotCone, Terrain, TimeOfDay, View, Weather, math,
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
                encoded: None,
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
                encoded: None,
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

/// A game's lights: a spot lights the ground inside its cone at midnight and not outside it, a
/// light that follows the lamps is dark by day, and one put out leaves the frame as it was.
#[test]
#[ignore = "requires a Vulkan driver"]
fn game_lights_spot_and_follow_the_lamps() {
    let mut device = Device::headless().expect("device");
    let world = World {
        instances: vec![Instance {
            mesh: 0,
            transform: Transform::IDENTITY,
        }],
        meshes: vec![quad(0.0, 20.0, 0)],
        materials: vec![material("ground", [0.4, 0.4, 0.4, 1.0], Shading::Standard)],
        textures: vec![],
        lights: vec![],
    };
    let mut r = Renderer::new(
        &mut device,
        &world,
        Look::default(),
        Quality::realtime(),
        (W, H),
        Output::Still,
    )
    .expect("renderer");
    let dark = render(&mut device, &mut r, 0.0, "clear");
    // Straight down onto the ground in front of the camera, a narrow cone.
    let spot = LightDesc {
        position: [0.0, 3.0, 0.0],
        range: 8.0,
        color: [1.0, 0.85, 0.6],
        intensity: 40.0,
        spot: Some(SpotCone {
            direction: [0.0, -1.0, 0.0],
            inner: 0.25,
            outer: 0.4,
        }),
        follows_lamps: false,
        shadow: 0.0,
    };
    let light = r.create_light(&spot).expect("light");
    assert_eq!(r.game_lights(), 1);
    let lit = render(&mut device, &mut r, 0.0, "clear");
    // Where the cone's middle and a point well outside it land in the frame.
    let pixel = |p: [f32; 3]| {
        let q = r.project(&view(), (W, H), p).expect("in front");
        (q[0] as u32, q[1] as u32)
    };
    let (centre, aside) = (pixel([0.0, 0.0, 0.0]), pixel([3.0, 0.0, 0.0]));
    let at = |px: &[u8], (x, y): (u32, u32)| region(px, x - 2, y - 2, x + 2, y + 2);
    assert!(at(&lit, centre) > at(&dark, centre) + 20.0, "in the cone");
    assert!(
        (at(&lit, aside) - at(&dark, aside)).abs() < 3.0,
        "outside it"
    );
    // Following the lamps, it is dark at noon like them.
    let noon = render(&mut device, &mut r, 12.0, "clear");
    r.set_light(
        light,
        &LightDesc {
            follows_lamps: true,
            ..spot
        },
    )
    .expect("follow");
    let noon_follow = render(&mut device, &mut r, 12.0, "clear");
    assert!(
        at(&noon, centre) > at(&noon_follow, centre) + 5.0,
        "by day it follows the lamps"
    );
    r.destroy_light(light);
    assert_eq!(r.game_lights(), 0);
    let out = render(&mut device, &mut r, 0.0, "clear");
    assert!(difference(&out, &dark) < 0.5, "put out");
    assert!(r.set_light(light, &spot).is_err());
    let errors = device.validation_errors();
    assert!(errors.is_empty(), "validation: {errors:?}");
}

/// A spot casting shadows: a prop between it and the ground takes its light off the ground behind
/// the prop, as much as the light asks, and nowhere else; at most two lights cast at once.
#[test]
#[ignore = "requires a Vulkan driver"]
fn game_lights_cast_the_props_shadows() {
    let mut device = Device::headless().expect("device");
    let world = World {
        instances: vec![Instance {
            mesh: 0,
            transform: Transform::IDENTITY,
        }],
        meshes: vec![quad(0.0, 20.0, 0)],
        materials: vec![material("ground", [0.5, 0.5, 0.5, 1.0], Shading::Standard)],
        textures: vec![],
        lights: vec![],
    };
    let mut quality = Quality::realtime();
    quality.shadow_size = 512;
    quality.light_shadow_size = 512;
    let mut r = Renderer::new(
        &mut device,
        &world,
        Look::default(),
        quality,
        (W, H),
        Output::Still,
    )
    .expect("renderer");
    // A metre cube on the ground at the middle.
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
    let white = MaterialDesc {
        name: "prop-white".into(),
        base_color: [0.8, 0.8, 0.8, 1.0],
        ..MaterialDesc::default()
    };
    let white = r
        .create_prop_material(&mut device, &white)
        .expect("material");
    let mut place = math::IDENTITY;
    place[13] = 0.5;
    r.create_prop_instance(mesh, white, &place).expect("cube");
    // From up on the left, aimed past the cube: its shadow falls on the ground to its right.
    let spot = LightDesc {
        position: [-3.0, 3.0, 0.0],
        range: 12.0,
        color: [1.0, 0.9, 0.8],
        intensity: 60.0,
        spot: Some(SpotCone {
            direction: [4.5, -3.0, 0.0],
            inner: 0.3,
            outer: 0.6,
        }),
        follows_lamps: false,
        shadow: 0.0,
    };
    let light = r.create_light(&spot).expect("light");
    let pixel = |p: [f32; 3]| {
        let q = r.project(&view(), (W, H), p).expect("in front");
        (q[0] as u32, q[1] as u32)
    };
    let (behind, beside) = (pixel([1.4, 0.0, 0.0]), pixel([1.4, 0.0, 1.6]));
    let at = |px: &[u8], (x, y): (u32, u32)| region(px, x - 1, y - 1, x + 2, y + 2);
    let open = render(&mut device, &mut r, 0.0, "clear");
    let mut frame = |shadow: f32| {
        r.set_light(light, &LightDesc { shadow, ..spot })
            .expect("set");
        render(&mut device, &mut r, 0.0, "clear")
    };
    let full = frame(1.0);
    let half = frame(0.5);
    let none = frame(0.0);
    eprintln!(
        "behind the cube: {:.1} open, {:.1} half, {:.1} full shadow; beside it {:.1} / {:.1}",
        at(&open, behind),
        at(&half, behind),
        at(&full, behind),
        at(&open, beside),
        at(&full, beside),
    );
    assert!(
        at(&open, behind) > at(&full, behind) + 15.0,
        "the cube's shadow"
    );
    let mid = at(&half, behind);
    assert!(
        mid < at(&open, behind) - 4.0 && mid > at(&full, behind) + 4.0,
        "half a shadow"
    );
    assert!(
        (at(&open, beside) - at(&full, beside)).abs() < 3.0,
        "lit beside it"
    );
    assert!(difference(&none, &open) < 0.5, "none cast");
    // Two lights cast at most; a third waits until one stops.
    let shadowed = LightDesc {
        shadow: 1.0,
        ..spot
    };
    r.set_light(light, &shadowed).expect("first");
    let second = r.create_light(&shadowed).expect("second");
    assert!(r.create_light(&shadowed).is_err(), "a third");
    let third = r.create_light(&spot).expect("unshadowed");
    assert!(r.set_light(third, &shadowed).is_err(), "made a third");
    r.destroy_light(second);
    r.set_light(third, &shadowed).expect("now there is room");
    render(&mut device, &mut r, 0.0, "clear");
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
    let hit = r
        .pick(&view(), size, (p[0], p[1]), u32::MAX)
        .expect("a pick");
    assert_eq!(hit.instance, right);
    assert!(
        (hit.point[0] - 2.0).abs() < 0.6 && hit.distance > 5.0,
        "{hit:?}"
    );
    assert!(
        r.pick(&view(), size, (2.0, 2.0), u32::MAX).is_none(),
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
    assert!(r.pick(&view(), size, (p[0], p[1]), u32::MAX).is_none());
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

/// Prop layers: blended props mix with the scene by alpha (tinted per instance), additive
/// ones add, overlays land over the finished image in their exact colour (on top of
/// everything without a depth test, hidden behind nearer surfaces with one) in draw order,
/// and picking follows what is drawn on top.
#[test]
#[ignore = "requires a Vulkan driver"]
fn prop_layers_blend_overlay_tint_and_pick() {
    use abe_engine::render::BlendMode;
    let mut device = Device::headless().expect("device");
    let world = World {
        instances: vec![Instance {
            mesh: 0,
            transform: Transform::IDENTITY,
        }],
        meshes: vec![quad(0.0, 20.0, 0)],
        materials: vec![material(
            "ground",
            [0.15, 0.15, 0.15, 1.0],
            Shading::Standard,
        )],
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
    let mesh_of = |device: &mut Device<'_>, r: &mut Renderer, m: &Mesh| {
        let positions: Vec<[f32; 3]> = m.positions.iter().map(|p| [p.0, p.1, p.2]).collect();
        let normals: Vec<[f32; 3]> = m.normals.iter().map(|n| [n.0, n.1, n.2]).collect();
        r.create_prop_mesh(
            device,
            &MeshData {
                positions: &positions,
                normals: &normals,
                uvs: &m.uvs,
                indices: &m.indices,
            },
        )
        .expect("mesh")
    };
    let cube_mesh = mesh_of(&mut device, &mut r, &cube([0.0, 0.0, 0.0], 0.5, 0));
    // A 1.2 m card facing +z (the camera).
    let card = Mesh {
        positions: vec![
            Vec3(-0.6, -0.6, 0.0),
            Vec3(0.6, -0.6, 0.0),
            Vec3(0.6, 0.6, 0.0),
            Vec3(-0.6, 0.6, 0.0),
        ],
        normals: vec![Vec3(0.0, 0.0, 1.0); 4],
        uvs: vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
        indices: vec![0, 1, 2, 0, 2, 3],
        color: [0.5; 3],
        material: Some(0),
    };
    let card_mesh = mesh_of(&mut device, &mut r, &card);
    let at = |x: f32, y: f32, z: f32| {
        let mut m = [0.0f32; 16];
        for i in [0, 5, 10, 15] {
            m[i] = 1.0;
        }
        m[12] = x;
        m[13] = y;
        m[14] = z;
        m
    };
    let new_material = |device: &mut Device<'_>, r: &mut Renderer, d: MaterialDesc| {
        r.create_prop_material(device, &d).expect("material")
    };
    let white = new_material(
        &mut device,
        &mut r,
        MaterialDesc {
            name: "white".into(),
            base_color: [0.2, 0.2, 0.2, 1.0],
            ..MaterialDesc::default()
        },
    );
    let cube_at = r
        .create_prop_instance(cube_mesh, white, &at(0.0, 0.5, 0.0))
        .expect("cube");
    let size = (W, H);
    let centre = r.project(&view(), size, [0.0, 0.5, 0.5]).expect("in front");
    let (cx, cy) = (centre[0] as u32, centre[1] as u32);
    let px = |img: &[u8]| {
        let i = ((cy * W + cx) * 4) as usize;
        [img[i], img[i + 1], img[i + 2]]
    };
    let plain = render(&mut device, &mut r, 15.0, "clear");
    let base = px(&plain);
    eprintln!("grey cube: {base:?}");
    assert!(base.iter().all(|&c| c < 230), "not saturated: {base:?}");

    // Blended: a red card in front of the cube, faded by its instance tint.
    let glass = new_material(
        &mut device,
        &mut r,
        MaterialDesc {
            name: "glass".into(),
            base_color: [0.6, 0.02, 0.02, 1.0],
            shading: Shading::Unlit,
            blend: BlendMode::Alpha,
            ..MaterialDesc::default()
        },
    );
    let pane = r
        .create_prop_instance(card_mesh, glass, &at(0.0, 0.5, 1.0))
        .expect("pane");
    let mut seen = vec![];
    for a in [1.0, 0.5, 0.0] {
        r.set_prop_instance_tint(pane, [1.0, 1.0, 1.0, a])
            .expect("tint");
        seen.push(px(&render(&mut device, &mut r, 15.0, "clear")));
    }
    eprintln!("red pane at alpha 1, 0.5, 0: {seen:?}");
    let redness = |p: [u8; 3]| i32::from(p[0]) - i32::from(p[1]);
    assert!(redness(seen[0]) > 100, "opaque red: {:?}", seen[0]);
    assert!(
        redness(seen[1]) > 30 && redness(seen[1]) < redness(seen[0]) - 30,
        "half red: {:?}",
        seen[1]
    );
    assert!(
        seen[2].iter().zip(base).all(|(a, b)| a.abs_diff(b) <= 2),
        "invisible: {:?} vs {base:?}",
        seen[2]
    );
    r.set_prop_instance(pane, &at(0.0, 0.5, 1.0), false)
        .expect("hide");

    // Additive: adds blue.
    let glow = new_material(
        &mut device,
        &mut r,
        MaterialDesc {
            name: "glow".into(),
            base_color: [0.0, 0.0, 0.3, 1.0],
            shading: Shading::Unlit,
            blend: BlendMode::Add,
            ..MaterialDesc::default()
        },
    );
    let flare = r
        .create_prop_instance(card_mesh, glow, &at(0.0, 0.5, 1.0))
        .expect("flare");
    let added = px(&render(&mut device, &mut r, 15.0, "clear"));
    eprintln!("blue glow over the cube: {added:?}");
    assert!(
        added[2] > base[2].saturating_add(2) || base[2] >= 250,
        "{added:?} vs {base:?}"
    );
    assert!(added[0].abs_diff(base[0]) <= 3, "only blue is added");
    r.destroy_prop_instance(flare);

    // Overlays behind the cube: on top without a depth test, hidden with one; exact colour.
    let overlay = |device: &mut Device<'_>, r: &mut Renderer, c: [f32; 4], depth_test, order| {
        r.create_prop_material(
            device,
            &MaterialDesc {
                name: "badge".into(),
                base_color: c,
                overlay: true,
                blend: BlendMode::Alpha,
                depth_test,
                order,
                ..MaterialDesc::default()
            },
        )
        .expect("overlay")
    };
    let green = overlay(&mut device, &mut r, [0.0, 1.0, 0.0, 1.0], false, 0.0);
    // Behind the cube on the line of sight through the measured pixel.
    let eye = view().eye;
    let front = [0.0, 0.5, 0.5];
    let ray = [front[0] - eye[0], front[1] - eye[1], front[2] - eye[2]];
    let len = (ray[0] * ray[0] + ray[1] * ray[1] + ray[2] * ray[2]).sqrt();
    let behind = |d: f32| {
        at(
            front[0] + ray[0] / len * d,
            front[1] + ray[1] / len * d,
            front[2] + ray[2] / len * d,
        )
    };
    let badge = r
        .create_prop_instance(card_mesh, green, &behind(2.0))
        .expect("badge");
    let full = render(&mut device, &mut r, 15.0, "clear");
    let on_top = px(&full);
    eprintln!("green overlay behind the cube, no depth test: {on_top:?}");
    assert!(
        on_top[0] <= 2 && on_top[1] >= 253 && on_top[2] <= 2,
        "exact colour on top: {on_top:?}"
    );
    let hit = r.pick(&view(), size, (cx as f32, cy as f32), u32::MAX);
    assert_eq!(
        hit.map(|h| h.instance),
        Some(badge),
        "the overlay on top is picked"
    );
    r.set_prop_instance_pick(badge, 2).expect("layer 2");
    let hit = r.pick(&view(), size, (cx as f32, cy as f32), 1);
    assert_eq!(
        hit.map(|h| h.instance),
        Some(cube_at),
        "layer 1 skips the badge"
    );
    let mut tested = r.prop_material(green).expect("desc").clone();
    tested.depth_test = true;
    r.update_prop_material(&mut device, green, &tested)
        .expect("depth test");
    let hidden = px(&render(&mut device, &mut r, 15.0, "clear"));
    assert!(
        hidden.iter().zip(base).all(|(a, b)| a.abs_diff(b) <= 2),
        "hidden behind the cube: {hidden:?} vs {base:?}"
    );
    // Draw order: blue (order 1) over green (order 0), then the other way round.
    tested.depth_test = false;
    r.update_prop_material(&mut device, green, &tested)
        .expect("on top");
    let blue = overlay(&mut device, &mut r, [0.0, 0.0, 1.0, 1.0], false, 1.0);
    let second = r
        .create_prop_instance(card_mesh, blue, &behind(1.5))
        .expect("blue");
    let order01 = px(&render(&mut device, &mut r, 15.0, "clear"));
    tested.order = 2.0;
    r.update_prop_material(&mut device, green, &tested)
        .expect("order");
    let order21 = px(&render(&mut device, &mut r, 15.0, "clear"));
    eprintln!("blue over green: {order01:?}; green over blue: {order21:?}");
    assert!(order01[2] >= 253 && order01[1] <= 2, "{order01:?}");
    assert!(order21[1] >= 253 && order21[2] <= 2, "{order21:?}");
    // An order between two others draws between them: blue at 1.5 over green at 1.25.
    tested.order = 1.25;
    r.update_prop_material(&mut device, green, &tested)
        .expect("order 1.25");
    let mut over = r.prop_material(blue).expect("blue").clone();
    over.order = 1.5;
    r.update_prop_material(&mut device, blue, &over)
        .expect("order 1.5");
    let between = px(&render(&mut device, &mut r, 15.0, "clear"));
    assert!(between[2] >= 253 && between[1] <= 2, "{between:?}");
    tested.order = 2.0;
    r.update_prop_material(&mut device, green, &tested)
        .expect("order back");
    r.set_prop_instance_pick(badge, 1).expect("layer 1");
    let hit = r.pick(&view(), size, (cx as f32, cy as f32), u32::MAX);
    assert_eq!(
        hit.map(|h| h.instance),
        Some(badge),
        "the last drawn overlay is picked"
    );
    r.destroy_prop_instance(second);
    r.destroy_prop_instance(badge);
    // Opaque overlays writing depth: the nearer red one, drawn first, hides the farther yellow
    // one drawn after it; testing alone, the later one covers it.
    let solid = |device: &mut Device<'_>, r: &mut Renderer, c: [f32; 4], order| {
        r.create_prop_material(
            device,
            &MaterialDesc {
                name: "solid".into(),
                base_color: c,
                overlay: true,
                depth_test: true,
                depth_write: true,
                order,
                ..MaterialDesc::default()
            },
        )
        .expect("solid overlay")
    };
    let near = solid(&mut device, &mut r, [1.0, 0.0, 0.0, 1.0], 0.0);
    let far = solid(&mut device, &mut r, [1.0, 1.0, 0.0, 1.0], 1.0);
    r.create_prop_instance(card_mesh, near, &behind(-2.0))
        .expect("near");
    r.create_prop_instance(card_mesh, far, &behind(-1.0))
        .expect("far");
    let written = px(&render(&mut device, &mut r, 15.0, "clear"));
    let mut unwritten = r.prop_material(far).expect("desc").clone();
    unwritten.depth_write = false;
    r.update_prop_material(
        &mut device,
        near,
        &MaterialDesc {
            base_color: [1.0, 0.0, 0.0, 1.0],
            order: 0.0,
            ..unwritten.clone()
        },
    )
    .expect("near without writing");
    let tested = px(&render(&mut device, &mut r, 15.0, "clear"));
    eprintln!("near red over far yellow, writing depth: {written:?}; only testing: {tested:?}");
    assert!(written[0] >= 253 && written[1] <= 2, "{written:?}");
    assert!(tested[0] >= 253 && tested[1] >= 253, "{tested:?}");
    // Writing depth is for opaque overlays that test it.
    for bad in [
        MaterialDesc {
            depth_write: true,
            ..MaterialDesc::default()
        },
        MaterialDesc {
            overlay: true,
            depth_test: false,
            depth_write: true,
            ..MaterialDesc::default()
        },
        MaterialDesc {
            overlay: true,
            depth_write: true,
            blend: BlendMode::Alpha,
            ..MaterialDesc::default()
        },
    ] {
        assert!(
            r.create_prop_material(&mut device, &bad).is_err(),
            "{bad:?}"
        );
    }
    let errors = device.validation_errors();
    assert!(errors.is_empty(), "validation: {errors:?}");
}

/// A game's prop shader colours an overlay by its own rule from its parameters (left half the
/// first colour, right half the second plus the last four), takes new parameters without a
/// rebuild, and is refused when it reads anything but its input, or draws anything but an
/// overlay.
#[test]
#[ignore = "requires a Vulkan driver"]
fn prop_shaders_colour_overlays_from_their_params() {
    use abe_engine::render::BlendMode;
    let mut device = Device::headless().expect("device");
    let world = World {
        instances: vec![Instance {
            mesh: 0,
            transform: Transform::IDENTITY,
        }],
        meshes: vec![quad(0.0, 20.0, 0)],
        materials: vec![material(
            "ground",
            [0.15, 0.15, 0.15, 1.0],
            Shading::Standard,
        )],
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
    let card = r
        .create_prop_mesh(
            &mut device,
            &MeshData {
                positions: &[
                    [-1.2, -0.6, 0.0],
                    [1.2, -0.6, 0.0],
                    [1.2, 0.6, 0.0],
                    [-1.2, 0.6, 0.0],
                ],
                normals: &[[0.0, 0.0, 1.0]; 4],
                uvs: &[[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
                indices: &[0, 1, 2, 0, 2, 3],
            },
        )
        .expect("card");
    let halves = r
        .create_prop_shader(
            &mut device,
            "fn shade(in: ShadeInput) -> vec4f {
               let c = select(in.p1.rgb + in.p4.rgb, in.p0.rgb, in.uv.x < 0.5);
               return vec4f(c * in.tint.rgb, 1.0);
             }",
        )
        .expect("shader");
    let mut params = [0.0f32; abe_engine::render::PROP_SHADER_PARAMS];
    params[..3].copy_from_slice(&[1.0, 0.0, 0.0]);
    params[4..7].copy_from_slice(&[0.0, 0.0, 1.0]);
    let desc = MaterialDesc {
        name: "halves".into(),
        overlay: true,
        depth_test: false,
        blend: BlendMode::Alpha,
        shader: Some(halves),
        params,
        ..MaterialDesc::default()
    };
    let material = r
        .create_prop_material(&mut device, &desc)
        .expect("material");
    let mut at = [0.0f32; 16];
    for i in [0, 5, 10, 15] {
        at[i] = 1.0;
    }
    at[13] = 1.0;
    at[14] = 2.0;
    r.create_prop_instance(card, material, &at)
        .expect("instance");
    let size = (W, H);
    let spot = |r: &Renderer, p: [f32; 3]| {
        let s = r.project(&view(), size, p).expect("in front");
        ((s[1] as u32 * W + s[0] as u32) * 4) as usize
    };
    let (at_left, at_right) = (spot(&r, [-0.6, 1.0, 2.0]), spot(&r, [0.6, 1.0, 2.0]));
    let pixel = |img: &[u8], i: usize| [img[i], img[i + 1], img[i + 2]];
    let img = render(&mut device, &mut r, 15.0, "clear");
    let (left, right) = (pixel(&img, at_left), pixel(&img, at_right));
    eprintln!("prop shader halves: {left:?} {right:?}");
    assert_eq!((left, right), ([255, 0, 0], [0, 0, 255]));

    // New parameters, no rebuild: green on the left now, and the last four (`p4`) add red on
    // the right.
    params[..3].copy_from_slice(&[0.0, 1.0, 0.0]);
    params[16..19].copy_from_slice(&[1.0, 0.0, 0.0]);
    r.set_prop_material_params(material, &params)
        .expect("params");
    let img = render(&mut device, &mut r, 15.0, "clear");
    assert_eq!(
        (pixel(&img, at_left), pixel(&img, at_right)),
        ([0, 255, 0], [255, 0, 255])
    );

    // A shader reads all three of its material's textures as pictures: the base (red) on the
    // left, the emissive (blue) on the right, the normal (green) sampled at another uv where
    // the base's alpha is low.
    let solid = |r: &mut Renderer, device: &mut Device<'_>, c: [u8; 4]| {
        r.create_prop_texture(device, (2, 2), &[c, c, c, c].concat(), true)
            .expect("texture")
    };
    let (red, green, blue) = (
        solid(&mut r, &mut device, [255, 0, 0, 255]),
        solid(&mut r, &mut device, [0, 255, 0, 255]),
        solid(&mut r, &mut device, [0, 0, 255, 255]),
    );
    let pictures = r
        .create_prop_shader(
            &mut device,
            "fn shade(in: ShadeInput) -> vec4f {
               let elsewhere = textureSample(normal_tex, material_sampler, in.uv * 0.5);
               let left = select(elsewhere.rgb, in.base.rgb, in.base.a > 0.5);
               let c = select(in.emissive_map.rgb, left, in.uv.x < 0.5);
               return vec4f(c * (in.normal_map.g + 0.0) , 1.0);
             }",
        )
        .expect("pictures");
    r.update_prop_material(
        &mut device,
        material,
        &MaterialDesc {
            shader: Some(pictures),
            base_texture: Some(red),
            normal_texture: Some(green),
            emissive_texture: Some(blue),
            ..desc.clone()
        },
    )
    .expect("textured");
    let img = render(&mut device, &mut r, 15.0, "clear");
    assert_eq!(
        (pixel(&img, at_left), pixel(&img, at_right)),
        ([255, 0, 0], [0, 0, 255]),
        "the base on the left, the emissive on the right"
    );

    // Refused: a shader that reads the frame block, one that does not compile, and a prop
    // shader on a material drawn in the scene.
    let reads_frame = r
        .create_prop_shader(
            &mut device,
            "fn shade(in: ShadeInput) -> vec4f { return vec4f(frame.camera.xyz, 1.0); }",
        )
        .expect_err("reads the frame");
    assert!(reads_frame.contains("reads only"), "{reads_frame}");
    assert!(
        r.create_prop_shader(&mut device, "fn shade(in: ShadeInput) -> vec4f { oops }")
            .is_err()
    );
    let in_scene = r
        .create_prop_material(
            &mut device,
            &MaterialDesc {
                overlay: false,
                ..desc.clone()
            },
        )
        .expect_err("not an overlay");
    assert!(in_scene.contains("overlays only"), "{in_scene}");
}

/// A run-time model: a static box on a node and a skinned bar bent by a clip; poses move what
/// is drawn, node visibility hides parts, and a node's world transform follows the pose.
#[test]
#[ignore = "requires a Vulkan driver"]
fn models_pose_skin_and_place() {
    use abe_engine::anim::Pose;
    use abe_engine::assets::model::{
        Channel, Clip, Interpolation, Material as ModelMaterial, Mesh as ModelMesh, Model, Node,
        Path, Primitive, Skin,
    };
    let mut device = Device::headless().expect("device");
    let world = World {
        instances: vec![Instance {
            mesh: 0,
            transform: Transform::IDENTITY,
        }],
        meshes: vec![quad(0.0, 20.0, 0)],
        materials: vec![material(
            "ground",
            [0.15, 0.15, 0.15, 1.0],
            Shading::Standard,
        )],
        textures: vec![],
        lights: vec![],
    };
    let mut r = Renderer::new(
        &mut device,
        &world,
        Look::default(),
        Quality::realtime(),
        (W, H),
        Output::Still,
    )
    .expect("renderer");
    let node = |name: &str, parent: Option<usize>, t: [f32; 3]| Node {
        name: name.into(),
        parent,
        children: vec![],
        translation: t,
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0; 3],
        mesh: None,
        skin: None,
        weights: None,
    };
    let to_prim = |m: &Mesh, joints: Vec<[u16; 4]>, weights: Vec<[f32; 4]>, material| Primitive {
        positions: m.positions.iter().map(|p| [p.0, p.1, p.2]).collect(),
        normals: m.normals.iter().map(|n| [n.0, n.1, n.2]).collect(),
        uvs: m.uvs.clone(),
        indices: m.indices.clone(),
        material: Some(material),
        joints,
        weights,
        targets: vec![],
    };
    // A 0.3 m thick, 2 m tall bar of two blocks: the lower follows the root, the upper the
    // elbow.
    let mut bar_prim = to_prim(&cube([0.0, 0.5, 0.0], 0.5, 0), vec![], vec![], 1);
    let upper = to_prim(&cube([0.0, 1.5, 0.0], 0.5, 0), vec![], vec![], 1);
    let n = bar_prim.positions.len() as u32;
    bar_prim.joints = vec![[0, 0, 0, 0]; bar_prim.positions.len()];
    bar_prim
        .joints
        .extend(vec![[1, 0, 0, 0]; upper.positions.len()]);
    bar_prim.positions.extend(upper.positions);
    bar_prim.normals.extend(upper.normals);
    bar_prim.uvs.extend(upper.uvs);
    bar_prim.indices.extend(upper.indices.iter().map(|i| i + n));
    for p in &mut bar_prim.positions {
        p[0] *= 0.3;
        p[2] *= 0.3;
    }
    bar_prim.weights = vec![[1.0, 0.0, 0.0, 0.0]; bar_prim.positions.len()];
    let mut model = Model {
        nodes: vec![
            node("root", None, [0.0; 3]),
            node("elbow", Some(0), [0.0, 1.0, 0.0]),
            node("bar", None, [0.0; 3]),
            node("box", Some(0), [1.5, 0.3, 0.0]),
            node("tip", Some(1), [0.0, 1.0, 0.0]),
        ],
        ..Model::default()
    };
    model.nodes[0].children = vec![1, 3];
    model.nodes[1].children = vec![4];
    model.nodes[2].mesh = Some(0);
    model.nodes[2].skin = Some(0);
    model.nodes[3].mesh = Some(1);
    model.roots = vec![0, 2];
    model.meshes = vec![
        ModelMesh {
            name: "bar".into(),
            primitives: vec![bar_prim],
            weights: vec![],
            target_names: vec![],
        },
        ModelMesh {
            name: "box".into(),
            primitives: vec![to_prim(&cube([0.0; 3], 0.3, 0), vec![], vec![], 0)],
            weights: vec![],
            target_names: vec![],
        },
    ];
    let unlit = |name: &str, c: [f32; 4]| ModelMaterial {
        name: name.into(),
        base_color: c,
        unlit: true,
        ..ModelMaterial::default()
    };
    model.materials = vec![
        unlit("blue", [0.05, 0.05, 0.9, 1.0]),
        unlit("red", [0.9, 0.05, 0.05, 1.0]),
    ];
    let mut inv = [0.0f32; 16];
    for i in [0, 5, 10, 15] {
        inv[i] = 1.0;
    }
    let mut inv_elbow = inv;
    inv_elbow[13] = -1.0;
    model.skins = vec![Skin {
        name: "arm".into(),
        joints: vec![0, 1],
        inverse_bind: vec![inv, inv_elbow],
    }];
    let s = std::f32::consts::FRAC_1_SQRT_2;
    model.clips = vec![Clip {
        name: "bend".into(),
        channels: vec![Channel {
            node: 1,
            path: Path::Rotation,
            interpolation: Interpolation::Linear,
            times: vec![0.0, 1.0],
            // 90 degrees about z: the upper half points to -x.
            values: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, s, s],
            width: 4,
        }],
        duration: 1.0,
    }];
    let id = r.create_model(&mut device, model, &[]).expect("model");
    let mut place = [0.0f32; 16];
    for i in [0, 5, 10, 15] {
        place[i] = 1.0;
    }
    let inst = r
        .create_model_instance(&mut device, id, &place)
        .expect("instance");
    let size = (W, H);
    let colour_at = |img: &[u8], p: [f32; 3], r: &Renderer| {
        let q = r.project(&view(), size, p).expect("in front");
        let i = ((q[1] as u32 * W + q[0] as u32) * 4) as usize;
        [img[i], img[i + 1], img[i + 2]]
    };
    let red = |c: [u8; 3]| u32::from(c[0]) > u32::from(c[2]) + 60;
    let blue = |c: [u8; 3]| u32::from(c[2]) > u32::from(c[0]) + 60;
    let rest = render(&mut device, &mut r, 15.0, "clear");
    assert!(red(colour_at(&rest, [0.0, 1.6, 0.15], &r)), "upright bar");
    assert!(
        blue(colour_at(&rest, [1.5, 0.3, 0.3], &r)),
        "the box on its node"
    );
    // Bend: the upper half now lies along -x at 1 m.
    {
        let (pose, model) = r.model_pose_and_model(inst).expect("pose");
        pose.apply(model, 0, 1.0, 1.0);
    }
    r.update_model_instance(&mut device, inst).expect("update");
    let bent = render(&mut device, &mut r, 15.0, "clear");
    assert!(
        !red(colour_at(&bent, [0.0, 1.8, 0.15], &r)),
        "nothing upright now"
    );
    assert!(
        red(colour_at(&bent, [-0.6, 1.0, 0.15], &r)),
        "bent to the side"
    );
    let tip = r.model_node_world(inst, 4).expect("tip");
    assert!(
        (tip[12] + 1.0).abs() < 1e-4 && (tip[13] - 1.0).abs() < 1e-4,
        "{:?}",
        &tip[12..15]
    );
    // Move the instance, hide the box node, and the model's rest is untouched.
    place[12] = -2.0;
    r.set_model_instance(inst, &place, true).expect("move");
    r.set_model_node_visible(inst, 3, false).expect("hide");
    let moved = render(&mut device, &mut r, 15.0, "clear");
    assert!(
        red(colour_at(&moved, [-2.6, 1.0, 0.15], &r)),
        "moved with its pose"
    );
    assert!(!blue(colour_at(&moved, [-0.5, 0.3, 0.3], &r)), "box hidden");
    let mut want = Pose::rest(r.model(id).expect("model"));
    want.apply(r.model(id).expect("model"), 0, 1.0, 1.0);
    assert_eq!(
        *r.model_pose_mut(inst).expect("pose"),
        want,
        "moving keeps the pose"
    );
    // A second instance starts at rest; destroying the model takes both.
    let second = r
        .create_model_instance(&mut device, id, &math_translate(2.0))
        .expect("second");
    let two = render(&mut device, &mut r, 15.0, "clear");
    assert!(
        red(colour_at(&two, [2.0, 1.6, 0.15], &r)),
        "the second is upright"
    );
    // The first goes, and with it its mesh's skin: the second still bends on its own joints.
    r.destroy_model_instance(&mut device, inst);
    {
        let (pose, model) = r.model_pose_and_model(second).expect("pose");
        pose.apply(model, 0, 1.0, 1.0);
    }
    r.update_model_instance(&mut device, second)
        .expect("update");
    let alone = render(&mut device, &mut r, 15.0, "clear");
    assert!(
        red(colour_at(&alone, [1.4, 1.0, 0.15], &r)),
        "the second bends"
    );
    assert!(
        !red(colour_at(&alone, [-2.6, 1.0, 0.15], &r)),
        "the first is gone"
    );
    r.destroy_model(&mut device, id);
    assert!(r.model_node_world(second, 0).is_err());
    assert_eq!(r.prop_instances(), 0);
    let errors = device.validation_errors();
    assert!(errors.is_empty(), "validation: {errors:?}");
}

/// Morph targets on a model node with two primitives: a target stretches two boxes whose top
/// corners lie scattered through their vertices, another lifts a third box in the second
/// primitive only. Each weight moves what it should, whichever changed last.
#[test]
#[ignore = "requires a Vulkan driver"]
fn models_morph_scattered_vertices() {
    use abe_engine::anim::morphed_vertices;
    use abe_engine::assets::model::{
        Material as ModelMaterial, Mesh as ModelMesh, Model, MorphTarget, Node, Primitive,
    };
    let mut device = Device::headless().expect("device");
    let world = World {
        instances: vec![Instance {
            mesh: 0,
            transform: Transform::IDENTITY,
        }],
        meshes: vec![quad(0.0, 20.0, 0)],
        materials: vec![material(
            "ground",
            [0.15, 0.15, 0.15, 1.0],
            Shading::Standard,
        )],
        textures: vec![],
        lights: vec![],
    };
    let mut r = Renderer::new(
        &mut device,
        &world,
        Look::default(),
        Quality::realtime(),
        (W, H),
        Output::Still,
    )
    .expect("renderer");
    // Boxes 0.6 m across on the ground; a target raises every top corner 1 m.
    let boxes = |xs: &[f32]| {
        let mut p = Primitive {
            material: Some(0),
            ..Primitive::default()
        };
        let mut raise = vec![];
        for &x in xs {
            let m = cube([x, 0.3, 0.0], 0.3, 0);
            let base = p.positions.len() as u32;
            for v in &m.positions {
                p.positions.push([v.0, v.1, v.2]);
                raise.push([0.0, if v.1 > 0.5 { 1.0 } else { 0.0 }, 0.0]);
            }
            p.normals.extend(m.normals.iter().map(|n| [n.0, n.1, n.2]));
            p.uvs.extend(&m.uvs);
            p.indices.extend(m.indices.iter().map(|i| i + base));
        }
        (p, raise)
    };
    let (mut pair, stretch) = boxes(&[-1.6, 0.0]);
    let (mut single, lift) = boxes(&[1.6]);
    let none = |p: &Primitive| MorphTarget {
        positions: vec![[0.0; 3]; p.positions.len()],
        normals: vec![],
    };
    pair.targets = vec![
        MorphTarget {
            positions: stretch,
            normals: vec![],
        },
        none(&pair),
    ];
    single.targets = vec![
        none(&single),
        MorphTarget {
            positions: lift,
            normals: vec![],
        },
    ];
    let mut model = Model {
        nodes: vec![Node {
            name: "boxes".into(),
            parent: None,
            children: vec![],
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
            mesh: Some(0),
            skin: None,
            weights: None,
        }],
        roots: vec![0],
        ..Model::default()
    };
    model.meshes = vec![ModelMesh {
        name: "boxes".into(),
        primitives: vec![pair, single],
        weights: vec![0.0, 0.0],
        target_names: vec!["stretch".into(), "lift".into()],
    }];
    model.materials = vec![ModelMaterial {
        name: "red".into(),
        base_color: [0.9, 0.05, 0.05, 1.0],
        unlit: true,
        ..ModelMaterial::default()
    }];
    let id = r.create_model(&mut device, model, &[]).expect("model");
    // The top corners, scattered through both boxes' faces, are gathered first.
    let gathered = &r.model(id).expect("model").meshes[0].primitives[0];
    assert_eq!(
        morphed_vertices(gathered),
        (0..24).collect::<Vec<u32>>(),
        "moved vertices first"
    );
    let mut place = [0.0f32; 16];
    for i in [0, 5, 10, 15] {
        place[i] = 1.0;
    }
    let inst = r
        .create_model_instance(&mut device, id, &place)
        .expect("instance");
    let size = (W, H);
    let red_at = |img: &[u8], p: [f32; 3], r: &Renderer| {
        let q = r.project(&view(), size, p).expect("in front");
        let i = ((q[1] as u32 * W + q[0] as u32) * 4) as usize;
        u32::from(img[i]) > u32::from(img[i + 2]) + 60
    };
    let tall = [[-1.6, 1.2, 0.3], [0.0, 1.2, 0.3], [1.6, 1.2, 0.3]];
    let mut weigh = |r: &mut Renderer, weights: [f32; 2]| {
        r.model_pose_mut(inst).expect("pose").weights[0] = weights.to_vec();
        r.update_model_instance(&mut device, inst).expect("update");
        let img = render(&mut device, r, 15.0, "clear");
        tall.map(|p| red_at(&img, p, r))
    };
    assert_eq!(weigh(&mut r, [0.0, 0.0]), [false; 3], "all at rest");
    assert_eq!(weigh(&mut r, [1.0, 0.0]), [true, true, false], "stretched");
    assert_eq!(weigh(&mut r, [1.0, 1.0]), [true; 3], "and lifted");
    assert_eq!(
        weigh(&mut r, [0.0, 1.0]),
        [false, false, true],
        "lifted only"
    );
    assert_eq!(weigh(&mut r, [0.0, 0.0]), [false; 3], "at rest again");
    r.destroy_model(&mut device, id);
    let errors = device.validation_errors();
    assert!(errors.is_empty(), "validation: {errors:?}");
}

fn math_translate(x: f32) -> [f32; 16] {
    let mut m = [0.0f32; 16];
    for i in [0, 5, 10, 15] {
        m[i] = 1.0;
    }
    m[12] = x;
    m
}

/// Depth of field: past the focus the scene softens (a far wall of black and white squares loses
/// its edges), what is nearer stays exactly as it was, and overlays are drawn sharp over the blur.
#[test]
#[ignore = "requires a Vulkan driver"]
fn focus_softens_what_lies_past_it() {
    use abe_engine::render::{BlendMode, Focus};
    let mut device = Device::headless().expect("device");
    let mut wall = material("wall", [1.0; 4], Shading::Standard);
    wall.base_color_texture = Some(0);
    // 2 m squares on a wall 40 m out, facing the camera; a red box 10 m out.
    let far = Mesh {
        positions: vec![
            Vec3(-40.0, -5.0, -30.0),
            Vec3(40.0, -5.0, -30.0),
            Vec3(40.0, 35.0, -30.0),
            Vec3(-40.0, 35.0, -30.0),
        ],
        normals: vec![Vec3(0.0, 0.0, 1.0); 4],
        uvs: vec![[0.0, 10.0], [20.0, 10.0], [20.0, 0.0], [0.0, 0.0]],
        indices: vec![0, 1, 2, 0, 2, 3],
        color: [0.5; 3],
        material: Some(0),
    };
    let world = World {
        instances: (0..2)
            .map(|mesh| Instance {
                mesh,
                transform: Transform::IDENTITY,
            })
            .collect(),
        meshes: vec![far, cube([0.0, 0.5, 0.0], 0.8, 1)],
        materials: vec![
            wall,
            material("box", [0.9, 0.05, 0.05, 1.0], Shading::Standard),
        ],
        textures: vec![Texture {
            width: 2,
            height: 2,
            srgb: true,
            levels: vec![
                vec![
                    0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 255,
                ],
                vec![128, 128, 128, 255],
            ],
            encoded: None,
        }],
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
    // A green badge over the wall, drawn on top as an overlay.
    let badge_mesh = r
        .create_prop_mesh(
            &mut device,
            &MeshData {
                positions: &[
                    [-14.0, 0.0, -29.0],
                    [-10.0, 0.0, -29.0],
                    [-10.0, 4.0, -29.0],
                    [-14.0, 4.0, -29.0],
                ],
                normals: &[],
                uvs: &[],
                indices: &[0, 1, 2, 0, 2, 3],
            },
        )
        .expect("badge mesh");
    let green = r
        .create_prop_material(
            &mut device,
            &MaterialDesc {
                name: "badge".into(),
                base_color: [0.0, 1.0, 0.0, 1.0],
                overlay: true,
                blend: BlendMode::Alpha,
                depth_test: false,
                ..MaterialDesc::default()
            },
        )
        .expect("badge");
    let mut identity = [0.0f32; 16];
    for i in [0, 5, 10, 15] {
        identity[i] = 1.0;
    }
    r.create_prop_instance(badge_mesh, green, &identity)
        .expect("badge instance");
    let env = Environment::resolve(
        &TimeOfDay {
            hours: 12.0,
            ..TimeOfDay::default()
        },
        &Weather::default(),
        &r.look().palette,
        3.0,
    );
    let sharp = r
        .render_still(&mut device, &view(), &env, 1)
        .expect("render");
    let focused = View {
        focus: Some(Focus {
            distance: 12.0,
            depth: 6.0,
            blur: 0.08,
        }),
        ..view()
    };
    let soft = r
        .render_still(&mut device, &focused, &env, 1)
        .expect("render");
    // How much neighbouring pixels differ across a region: the squares' edges.
    let edges = |px: &[u8], x0: u32, y0: u32, x1: u32, y1: u32| {
        let mut sum = 0.0;
        let mut n = 0.0;
        for y in y0..y1 {
            for x in x0..x1 {
                sum += (luma(px, x + 1, y) - luma(px, x, y)).abs()
                    + (luma(px, x, y + 1) - luma(px, x, y)).abs();
                n += 1.0;
            }
        }
        sum / n
    };
    // Right of the badge, which is drawn alike in both.
    let (crisp, blurred) = (edges(&sharp, 60, 2, 120, 18), edges(&soft, 60, 2, 120, 18));
    eprintln!("the far wall's edges: {crisp:.1} sharp, {blurred:.1} with the focus at 12 m");
    assert!(crisp > 8.0, "the squares show: {crisp}");
    assert!(blurred < crisp * 0.4, "{blurred} vs {crisp}");
    // The box, nearer than the focus, is untouched.
    let mut most = 0u8;
    for y in 33..40 {
        for x in 60..68 {
            let i = ((y * W + x) * 4) as usize;
            for c in 0..3 {
                most = most.max(sharp[i + c].abs_diff(soft[i + c]));
            }
        }
    }
    assert!(most <= 1, "the box changed by {most}");
    let i = ((H / 2 * W + W / 2) * 4) as usize;
    assert!(
        u32::from(soft[i]) > u32::from(soft[i + 1]) + 40,
        "the box is in view"
    );
    // The badge keeps its exact colour over the blur.
    let at = r
        .project(&focused, (W, H), [-12.0, 2.0, -29.0])
        .expect("badge in front");
    let b = ((at[1] as u32 * W + at[0] as u32) * 4) as usize;
    assert_eq!(&soft[b..b + 3], &[0, 255, 0], "the badge stays sharp");
    assert_eq!(&soft[b..b + 3], &sharp[b..b + 3]);
    let errors = device.validation_errors();
    assert!(errors.is_empty(), "validation: {errors:?}");
}
