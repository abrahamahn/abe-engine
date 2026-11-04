//! Physics on the engine's own renderer: a pyramid of blocks on the ground and a ball thrown into
//! it, simulated by `ganbate-physics` at 60 steps per second and drawn by the stylized renderer at five
//! moments, side by side in one image. The same run gives the same picture and the same numbers
//! every time, on every platform.
//!
//!     cargo run --release --example physics_demo -- physics.png
//!
//! It needs a Vulkan driver (`VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json` uses lavapipe).

use ganbate_engine::Vec3 as V;
use ganbate_engine::gpu::Device;
use ganbate_engine::render::{
    Environment, Look, Output, Quality, Renderer, TimeOfDay, View, Weather,
};
use ganbate_engine::world::{AlphaMode, Instance, Material, Mesh, Shading, Transform, World};
use ganbate_physics::{BodyDesc, BodyId, BodyKind, ColliderDesc, PhysicsWorld, Shape, Vec3};

const W: u32 = 384;
const H: u32 = 216;
const DT: f32 = 1.0 / 60.0;

fn material(name: &str, color: [f32; 4]) -> Material {
    Material {
        name: name.into(),
        base_color: color,
        shading: Shading::Standard,
        double_sided: true,
        alpha: AlphaMode::Opaque,
        ..Material::default()
    }
}

fn ground_mesh(half: f32) -> Mesh {
    let p = |x: f32, z: f32| V(x, 0.0, z);
    Mesh {
        positions: vec![
            p(-half, -half),
            p(half, -half),
            p(half, half),
            p(-half, half),
        ],
        normals: vec![V(0.0, 1.0, 0.0); 4],
        uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        indices: vec![0, 2, 1, 0, 3, 2],
        color: [0.5; 3],
        material: Some(0),
    }
}

/// A unit cube centred on the origin, flat-shaded.
fn cube_mesh() -> Mesh {
    let mut m = Mesh {
        material: Some(1),
        color: [0.8, 0.6, 0.4],
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
            let p: [f32; 3] = std::array::from_fn(|i| 0.5 * (n[i] + a * u[i] + b * v[i]));
            m.positions.push(V(p[0], p[1], p[2]));
            m.normals.push(V(n[0], n[1], n[2]));
            m.uvs.push([(a + 1.0) * 0.5, (b + 1.0) * 0.5]);
        }
        m.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    m
}

fn sphere_mesh(radius: f32) -> Mesh {
    let (rings, segments) = (12u32, 20u32);
    let mut m = Mesh {
        material: Some(2),
        color: [0.8, 0.2, 0.2],
        ..Mesh::default()
    };
    for r in 0..=rings {
        let theta = std::f32::consts::PI * r as f32 / rings as f32;
        for s in 0..=segments {
            let phi = std::f32::consts::TAU * s as f32 / segments as f32;
            let n = [
                theta.sin() * phi.cos(),
                theta.cos(),
                theta.sin() * phi.sin(),
            ];
            m.positions
                .push(V(n[0] * radius, n[1] * radius, n[2] * radius));
            m.normals.push(V(n[0], n[1], n[2]));
            m.uvs
                .push([s as f32 / segments as f32, r as f32 / rings as f32]);
        }
    }
    for r in 0..rings {
        for s in 0..segments {
            let a = r * (segments + 1) + s;
            let b = a + segments + 1;
            m.indices.extend([a, a + 1, b, a + 1, b + 1, b]);
        }
    }
    m
}

fn main() -> Result<(), String> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "physics.png".into());

    // The simulation.
    let mut physics = PhysicsWorld::default();
    let ground = physics.create_body(&BodyDesc {
        kind: BodyKind::Static,
        position: Vec3::new(0.0, -0.5, 0.0),
        ..Default::default()
    })?;
    physics.add_collider(
        ground,
        &ColliderDesc::new(Shape::Box {
            half_extents: Vec3::new(20.0, 0.5, 20.0),
        }),
    )?;
    let mut blocks: Vec<BodyId> = Vec::new();
    let base = 5;
    for row in 0..base {
        for i in 0..(base - row) {
            let x = (i as f32 - (base - row - 1) as f32 * 0.5) * 1.02;
            let b = physics.create_body(&BodyDesc {
                position: Vec3::new(x, 0.5 + row as f32, 0.0),
                ..Default::default()
            })?;
            let mut d = ColliderDesc::new(Shape::Box {
                half_extents: Vec3::splat(0.5),
            });
            d.density = 300.0;
            physics.add_collider(b, &d)?;
            blocks.push(b);
        }
    }
    let ball = physics.create_body(&BodyDesc {
        position: Vec3::new(-9.0, 1.2, 0.3),
        linear_velocity: Vec3::new(16.0, 3.0, 0.0),
        bullet: true,
        ..Default::default()
    })?;
    let mut d = ColliderDesc::new(Shape::Sphere { radius: 0.45 });
    d.density = 4000.0;
    d.restitution = 0.2;
    physics.add_collider(ball, &d)?;

    // What the renderer draws: the ground, one cube instance per block and the ball.
    let mut world = World {
        meshes: vec![ground_mesh(20.0), cube_mesh(), sphere_mesh(0.45)],
        materials: vec![
            material("ground", [0.35, 0.5, 0.28, 1.0]),
            material("block", [0.78, 0.58, 0.38, 1.0]),
            material("ball", [0.75, 0.16, 0.14, 1.0]),
        ],
        ..World::default()
    };
    world.instances.push(Instance {
        mesh: 0,
        transform: Transform::IDENTITY,
    });
    let bodies: Vec<(BodyId, u32)> = blocks.iter().map(|&b| (b, 1)).chain([(ball, 2)]).collect();
    for &(b, mesh) in &bodies {
        let pose = physics.body_pose(b).ok_or("body")?;
        world.instances.push(Instance {
            mesh,
            transform: Transform(pose.to_matrix()),
        });
    }

    let mut device = Device::headless()?;
    let mut renderer = Renderer::new(
        &mut device,
        &world,
        Look::default(),
        Quality::still(),
        (W, H),
        Output::Still,
    )?;
    let view = View {
        name: "demo".into(),
        eye: [4.0, 5.0, 13.0],
        target: [-1.0, 1.3, 0.0],
        fov: 45.0,
        ..View::default()
    };
    let time = TimeOfDay {
        hours: 15.5,
        ..TimeOfDay::default()
    };
    let env = Environment::resolve(&time, &Weather::default(), &renderer.look().palette, 3.0);

    let moments = [0.0f32, 0.5, 0.9, 1.5, 3.0];
    let mut sheet = vec![0u8; (W * H * 4) as usize * moments.len()];
    let mut t = 0.0f32;
    for (k, &at) in moments.iter().enumerate() {
        while t + 0.5 * DT < at {
            physics.step(DT);
            t += DT;
        }
        for (i, &(b, _)) in bodies.iter().enumerate() {
            let pose = physics.body_pose(b).ok_or("body")?;
            renderer.set_world_instance(&mut device, i + 1, pose.to_matrix(), true)?;
        }
        let frame = renderer.render_still(&mut device, &view, &env, 1)?;
        for row in 0..H as usize {
            let src = &frame[row * W as usize * 4..(row + 1) * W as usize * 4];
            let at = (row * W as usize * moments.len() + k * W as usize) * 4;
            sheet[at..at + W as usize * 4].copy_from_slice(src);
        }
        println!(
            "t = {:.2} s: ball at {:?}, {} blocks awake, {} contacts",
            t,
            physics.body_pose(ball).ok_or("ball")?.position,
            blocks.iter().filter(|&&b| physics.is_awake(b)).count(),
            physics.contact_count()
        );
    }
    let png = ganbate_engine::assets::png::encode(W * moments.len() as u32, H, &sheet)?;
    std::fs::write(&out, png).map_err(|e| e.to_string())?;
    println!(
        "{out}: {} moments; state hash {:016x}",
        moments.len(),
        physics.state_hash()
    );
    Ok(())
}
