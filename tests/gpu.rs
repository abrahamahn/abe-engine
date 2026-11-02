//! Driver tests for the GPU layer. Run on a Vulkan device, ideally with validation:
//! `ABE_VULKAN_VALIDATION=1 cargo test --locked --offline --test gpu -- --ignored`.
//! The presentation test needs a display (for example `Xvfb :99 &` and `DISPLAY=:99`).
//!
//! The shaders are GLSL sources in `tests/fixtures`, compiled with
//! `glslangValidator --target-env vulkan1.0 -V x -o x.spv`; their reflection is written by hand
//! below until the ABE shader compiler produces both.
#![cfg(any(target_os = "linux", target_os = "windows", target_os = "android"))]
use abe_engine::gpu::shader::{
    Binding, BindingKind, Compiled, Interface, IoType, SampleType, Stage, TexelFormat, TextureView,
};
use abe_engine::gpu::*;

fn spirv(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks_exact(4)
        .map(|w| u32::from_le_bytes(w.try_into().unwrap()))
        .collect()
}
fn io(location: u32, ty: IoType) -> Interface {
    Interface { location, ty }
}
fn binding(group: u32, binding: u32, kind: BindingKind) -> Binding {
    Binding {
        group,
        binding,
        kind,
    }
}
fn shader(
    stage: Stage,
    bytes: &[u8],
    bindings: Vec<Binding>,
    push_constant_size: u32,
    inputs: Vec<Interface>,
    outputs: Vec<Interface>,
) -> Compiled {
    Compiled {
        stage,
        words: spirv(bytes),
        bindings,
        push_constant_size,
        inputs,
        outputs,
        workgroup_size: [1, 1, 1],
        msl: None,
        wgsl: None,
        glsl: None,
    }
}
fn fullscreen_vs() -> Compiled {
    let spv = include_bytes!("fixtures/fullscreen.vert.spv");
    shader(
        Stage::Vertex,
        spv,
        vec![],
        0,
        vec![],
        vec![io(0, IoType::Vec2F)],
    )
}
fn sample_fs() -> Compiled {
    let texture = BindingKind::Texture {
        view: TextureView::D2,
        sample: SampleType::Float,
    };
    shader(
        Stage::Fragment,
        include_bytes!("fixtures/sample.frag.spv"),
        vec![
            binding(0, 0, texture),
            binding(0, 1, BindingKind::Sampler { comparison: false }),
        ],
        4,
        vec![io(0, IoType::Vec2F)],
        vec![io(0, IoType::Vec4F)],
    )
}
fn mesh_vs() -> Compiled {
    shader(
        Stage::Vertex,
        include_bytes!("fixtures/mesh.vert.spv"),
        vec![],
        32,
        vec![io(0, IoType::Vec3F), io(1, IoType::Vec4F)],
        vec![io(0, IoType::Vec4F)],
    )
}
fn color_fs() -> Compiled {
    shader(
        Stage::Fragment,
        include_bytes!("fixtures/color.frag.spv"),
        vec![],
        0,
        vec![io(0, IoType::Vec4F)],
        vec![io(0, IoType::Vec4F)],
    )
}
fn quad_vs() -> Compiled {
    let spv = include_bytes!("fixtures/quad.vert.spv");
    shader(Stage::Vertex, spv, vec![], 20, vec![], vec![])
}
fn shadow_fs(array: bool) -> Compiled {
    let texture = BindingKind::Texture {
        view: if array {
            TextureView::D2Array
        } else {
            TextureView::D2
        },
        sample: SampleType::Depth,
    };
    let spv: &[u8] = if array {
        include_bytes!("fixtures/shadow_array.frag.spv")
    } else {
        include_bytes!("fixtures/shadow.frag.spv")
    };
    shader(
        Stage::Fragment,
        spv,
        vec![
            binding(0, 0, texture),
            binding(0, 1, BindingKind::Sampler { comparison: true }),
        ],
        if array { 8 } else { 4 },
        vec![io(0, IoType::Vec2F)],
        vec![io(0, IoType::Vec4F)],
    )
}
fn cube_fs() -> Compiled {
    let texture = BindingKind::Texture {
        view: TextureView::Cube,
        sample: SampleType::Float,
    };
    shader(
        Stage::Fragment,
        include_bytes!("fixtures/cube.frag.spv"),
        vec![
            binding(0, 0, texture),
            binding(0, 1, BindingKind::Sampler { comparison: false }),
        ],
        16,
        vec![io(0, IoType::Vec2F)],
        vec![io(0, IoType::Vec4F)],
    )
}
fn fill_cs() -> Compiled {
    let mut c = shader(
        Stage::Compute,
        include_bytes!("fixtures/fill.comp.spv"),
        vec![
            binding(0, 0, BindingKind::UniformBuffer { size: 8 }),
            binding(0, 1, BindingKind::StorageBuffer { read_only: false }),
            binding(
                0,
                2,
                BindingKind::StorageTexture {
                    view: TextureView::D2,
                    format: TexelFormat::Rgba8Unorm,
                },
            ),
        ],
        0,
        vec![],
        vec![],
    );
    c.workgroup_size = [8, 8, 1];
    c
}
fn scale_cs() -> Compiled {
    let mut c = shader(
        Stage::Compute,
        include_bytes!("fixtures/scale.comp.spv"),
        vec![
            binding(0, 0, BindingKind::StorageBuffer { read_only: true }),
            binding(0, 1, BindingKind::StorageBuffer { read_only: false }),
        ],
        8,
        vec![],
        vec![],
    );
    c.workgroup_size = [64, 1, 1];
    c
}

fn device() -> Device<'static> {
    let gpu = Device::headless().expect("a Vulkan device");
    eprintln!(
        "device: {} (validation {})",
        gpu.name(),
        if gpu.validation_enabled() {
            "on"
        } else {
            "off"
        }
    );
    gpu
}
/// Asserts that neither the test nor the device teardown produced validation errors.
fn finish(gpu: Device<'_>) {
    let log = gpu.validation_log();
    let errors = gpu.validation_errors();
    assert!(errors.is_empty(), "validation errors: {errors:#?}");
    drop(gpu);
    let errors = log.errors();
    assert!(
        errors.is_empty(),
        "validation errors at teardown: {errors:#?}"
    );
}
fn readback(gpu: &mut Device<'_>, size: u64) -> BufferId {
    gpu.create_buffer(&BufferDesc {
        size,
        usage: BufferUsage::COPY_DST,
        memory: Memory::Readback,
    })
    .unwrap()
}
fn target(gpu: &mut Device<'_>, w: u32, h: u32, extra: TextureUsage) -> TextureId {
    let usage = TextureUsage::COLOR_TARGET | TextureUsage::COPY_SRC | extra;
    gpu.create_texture(&TextureDesc::new(w, h, Format::Rgba8Unorm, usage))
        .unwrap()
}
fn pixel(data: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * width + x) * 4) as usize;
    data[i..i + 4].try_into().unwrap()
}
fn solid(w: u32, h: u32, rgba: [u8; 4]) -> Vec<u8> {
    rgba.repeat((w * h) as usize)
}
/// Four quadrants: top-left, top-right, bottom-left, bottom-right.
fn quadrants(size: u32, colors: [[u8; 4]; 4]) -> Vec<u8> {
    let mut out = Vec::new();
    for y in 0..size {
        for x in 0..size {
            let q = (y >= size / 2) as usize * 2 + (x >= size / 2) as usize;
            out.extend_from_slice(&colors[q]);
        }
    }
    out
}
fn vertices(list: &[([f32; 3], [u8; 4])]) -> Vec<u8> {
    let mut out = Vec::new();
    for (p, c) in list {
        out.extend(f32_bytes(p));
        out.extend_from_slice(c);
    }
    out
}
fn mesh_layout() -> VertexLayout {
    VertexLayout {
        stride: 16,
        attributes: vec![
            VertexAttribute {
                location: 0,
                format: VertexFormat::F32x3,
                offset: 0,
            },
            VertexAttribute {
                location: 1,
                format: VertexFormat::Unorm8x4,
                offset: 12,
            },
        ],
    }
}
fn depth_state(compare: Compare) -> DepthState {
    DepthState {
        format: Format::Depth32Float,
        compare,
        write: true,
        bias_constant: 0.0,
        bias_slope: 0.0,
        clamp: false,
    }
}
fn color_target(format: Format) -> ColorTarget {
    ColorTarget {
        format,
        blend: Blend::Replace,
        write_mask: 0xF,
    }
}
fn fullscreen_pipeline(gpu: &mut Device<'_>, fragment: &Compiled) -> PipelineId {
    let vs = fullscreen_vs();
    gpu.create_render_pipeline(&RenderPipelineDesc {
        vertex: &vs,
        fragment: Some(fragment),
        vertex_layout: None,
        topology: Topology::TriangleList,
        cull: Cull::None,
        front_ccw: true,
        depth: None,
        colors: vec![color_target(Format::Rgba8Unorm)],
    })
    .unwrap()
}
fn nearest(gpu: &mut Device<'_>, compare: Option<Compare>) -> SamplerId {
    gpu.create_sampler(&SamplerDesc {
        filter: Filter::Nearest,
        mip_filter: Filter::Nearest,
        address: Address::Clamp,
        anisotropy: 1.0,
        compare,
        max_lod: 16.0,
    })
    .unwrap()
}

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const WHITE: [u8; 4] = [255; 4];
const BLACK: [u8; 4] = [0, 0, 0, 255];

#[test]
#[ignore = "requires a Vulkan device; run cargo test --test gpu -- --ignored"]
fn clears_multiple_targets_and_reads_them_back() {
    let mut gpu = device();
    let (w, h) = (64, 32);
    let color = target(&mut gpu, w, h, TextureUsage::empty());
    let usage = TextureUsage::COLOR_TARGET | TextureUsage::COPY_SRC;
    let hdr = gpu
        .create_texture(&TextureDesc::new(w, h, Format::Rgba16Float, usage))
        .unwrap();
    let depth_usage = TextureUsage::DEPTH_TARGET | TextureUsage::COPY_SRC;
    let depth = gpu
        .create_texture(&TextureDesc::new(w, h, Format::Depth32Float, depth_usage))
        .unwrap();
    let pixels = u64::from(w * h);
    let out = readback(&mut gpu, pixels * 16);
    for (rgba, expected) in [
        ([0.2, 0.4, 0.6, 1.0], [51, 102, 153, 255]),
        ([1.0, 0.0, 0.8, 0.0], [255, 0, 204, 0]),
    ] {
        let mut e = gpu.encoder();
        e.begin_pass(PassDesc {
            colors: vec![
                Attachment::clear(color, rgba),
                Attachment::clear(hdr, [0.5, -2.0, 1024.0, 0.25]),
            ],
            depth: Some(Attachment::clear(depth, [0.25, 0.0, 0.0, 0.0])),
        });
        e.end_pass();
        e.copy_texture_to_buffer(color, 0, 0, out, 0);
        e.copy_texture_to_buffer(hdr, 0, 0, out, pixels * 4);
        e.copy_texture_to_buffer(depth, 0, 0, out, pixels * 12);
        gpu.submit(e).unwrap();
        let data = gpu.read_buffer(out).unwrap();
        let (rgba8, rest) = data.split_at((pixels * 4) as usize);
        let (half, float) = rest.split_at((pixels * 8) as usize);
        assert!(
            rgba8.chunks_exact(4).all(|p| p == expected),
            "{:?}",
            &rgba8[..4]
        );
        // f16 0.5, -2, 1024, 0.25
        let hdr_texel = [0x00, 0x38, 0x00, 0xC0, 0x00, 0x64, 0x00, 0x34];
        assert!(
            half.chunks_exact(8).all(|p| p == hdr_texel),
            "{:?}",
            &half[..8]
        );
        assert!(float.chunks_exact(4).all(|p| p == 0.25f32.to_le_bytes()));
    }
    finish(gpu);
}

#[test]
#[ignore = "requires a Vulkan device; run cargo test --test gpu -- --ignored"]
fn draws_vertex_buffers_with_push_constants_depth_and_culling() {
    let mut gpu = device();
    let (w, h) = (64, 64);
    let vs = mesh_vs();
    let fs = color_fs();
    let pipeline = gpu
        .create_render_pipeline(&RenderPipelineDesc {
            vertex: &vs,
            fragment: Some(&fs),
            vertex_layout: Some(mesh_layout()),
            topology: Topology::TriangleList,
            cull: Cull::Back,
            front_ccw: true,
            depth: Some(depth_state(Compare::Less)),
            colors: vec![color_target(Format::Rgba8Unorm)],
        })
        .unwrap();
    // In Vulkan framebuffer space (y down), (top-left, bottom, top-right) is counter-clockwise.
    let geometry = vertices(&[
        // Far triangle, magenta tinted to red: vertices 0..3.
        ([-0.8, -0.8, 0.6], [255, 0, 255, 255]),
        ([0.0, 0.8, 0.6], [255, 0, 255, 255]),
        ([0.8, -0.8, 0.6], [255, 0, 255, 255]),
        // Near triangle (drawn indexed, scaled by 0.5): vertices 3..6.
        ([-0.8, -0.8, 0.2], GREEN),
        ([0.0, 0.8, 0.2], GREEN),
        ([0.8, -0.8, 0.2], GREEN),
        // Clockwise, hence back-facing and culled: top-left corner.
        ([-1.0, -1.0, 0.1], BLUE),
        ([-0.6, -1.0, 0.1], BLUE),
        ([-1.0, -0.6, 0.1], BLUE),
    ]);
    let vb = gpu
        .create_buffer(&BufferDesc {
            size: geometry.len() as u64,
            usage: BufferUsage::VERTEX | BufferUsage::COPY_DST,
            memory: Memory::Device,
        })
        .unwrap();
    gpu.write_buffer(vb, 0, &geometry).unwrap();
    let ib = gpu
        .create_buffer(&BufferDesc {
            size: 12,
            usage: BufferUsage::INDEX,
            memory: Memory::Upload,
        })
        .unwrap();
    gpu.write_buffer(ib, 0, &u32_bytes(&[0, 1, 2])).unwrap();
    let color = target(&mut gpu, w, h, TextureUsage::empty());
    let depth = gpu
        .create_texture(&TextureDesc::new(
            w,
            h,
            Format::Depth32Float,
            TextureUsage::DEPTH_TARGET,
        ))
        .unwrap();
    let out = readback(&mut gpu, u64::from(w * h * 4));
    let mut frames = Vec::new();
    for near_first in [false, true] {
        let mut e = gpu.encoder();
        e.begin_pass(PassDesc {
            colors: vec![Attachment::clear(color, [0.0, 0.0, 0.0, 1.0])],
            depth: Some(Attachment {
                store: false,
                ..Attachment::clear(depth, [1.0; 4])
            }),
        });
        e.set_pipeline(pipeline);
        e.set_vertex_buffer(vb, 0);
        e.set_index_buffer(ib, 0);
        let far = |e: &mut Encoder| {
            e.push_constants(&f32_bytes(&[0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0]));
            e.draw(3, 1, 0, 0);
        };
        let near = |e: &mut Encoder| {
            e.push_constants(&f32_bytes(&[0.0, 0.0, 0.5, 0.0, 1.0, 1.0, 1.0, 1.0]));
            e.draw_indexed(3, 1, 0, 3, 0);
        };
        if near_first {
            near(&mut e);
            far(&mut e);
        } else {
            far(&mut e);
            near(&mut e);
        }
        e.push_constants(&f32_bytes(&[0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0]));
        e.draw(3, 1, 6, 0);
        e.end_pass();
        e.copy_texture_to_buffer(color, 0, 0, out, 0);
        gpu.submit(e).unwrap();
        frames.push(gpu.read_buffer(out).unwrap());
    }
    assert_eq!(frames[0], frames[1], "depth, not order, decides visibility");
    let image = &frames[0];
    assert_eq!(pixel(image, w, 32, 32), GREEN);
    assert_eq!(pixel(image, w, 32, 10), RED);
    assert_eq!(
        pixel(image, w, 2, 2),
        BLACK,
        "the clockwise triangle is culled"
    );
    assert_eq!(pixel(image, w, 63, 63), BLACK);
    finish(gpu);
}

#[test]
#[ignore = "requires a Vulkan device; run cargo test --test gpu -- --ignored"]
fn samples_uploaded_and_generated_mips() {
    let mut gpu = device();
    let size = 16;
    let usage = TextureUsage::SAMPLED | TextureUsage::COPY_DST | TextureUsage::COPY_SRC;
    let desc = TextureDesc {
        mips: 5,
        ..TextureDesc::new(size, size, Format::Rgba8Unorm, usage)
    };
    let uploaded = gpu.create_texture(&desc).unwrap();
    let colors = [RED, GREEN, BLUE, WHITE, [255, 255, 0, 255]];
    for (mip, rgba) in colors.iter().enumerate() {
        let (w, h) = desc.mip_size(mip as u32);
        gpu.write_texture(uploaded, mip as u32, 0, &solid(w, h, *rgba))
            .unwrap();
    }
    let generated = gpu.create_texture(&desc).unwrap();
    let checker = quadrants(size, [BLACK, WHITE, WHITE, BLACK]);
    gpu.write_texture(generated, 0, 0, &checker).unwrap();
    gpu.generate_mips(generated).unwrap();
    assert!(gpu.write_texture(generated, 1, 0, &checker).is_err());

    let fs = sample_fs();
    let pipeline = fullscreen_pipeline(&mut gpu, &fs);
    let sampler = nearest(&mut gpu, None);
    let groups = [uploaded, generated].map(|t| {
        gpu.create_bind_group(
            pipeline,
            0,
            &[(0, Resource::Texture(t)), (1, Resource::Sampler(sampler))],
        )
        .unwrap()
    });
    let view = target(&mut gpu, size, size, TextureUsage::empty());
    let bytes = u64::from(size * size * 4);
    let draws = [
        (0, 0.0),
        (0, 1.0),
        (0, 2.0),
        (0, 3.0),
        (0, 4.0),
        (1, 0.0),
        (1, 3.0),
        (1, 4.0),
    ];
    let out = readback(&mut gpu, bytes * draws.len() as u64);
    let mut e = gpu.encoder();
    for (i, (group, lod)) in draws.iter().enumerate() {
        e.begin_pass(PassDesc {
            colors: vec![Attachment {
                load: Load::DontCare,
                ..Attachment::clear(view, [0.0; 4])
            }],
            depth: None,
        });
        e.set_pipeline(pipeline);
        e.set_bind_group(0, groups[*group]);
        e.push_constants(&f32_bytes(&[*lod]));
        e.draw(3, 1, 0, 0);
        e.end_pass();
        e.copy_texture_to_buffer(view, 0, 0, out, bytes * i as u64);
    }
    gpu.submit(e).unwrap();
    let data = gpu.read_buffer(out).unwrap();
    let frames: Vec<&[u8]> = data.chunks_exact(bytes as usize).collect();
    for (mip, rgba) in colors.iter().enumerate() {
        assert!(
            frames[mip].chunks_exact(4).all(|p| p == rgba),
            "uploaded mip {mip}: {:?}",
            pixel(frames[mip], size, 0, 0)
        );
    }
    assert_eq!(frames[5], checker.as_slice(), "mip 0 survives generation");
    for (x, y, rgba) in [
        (0, 0, BLACK),
        (15, 0, WHITE),
        (0, 15, WHITE),
        (15, 15, BLACK),
    ] {
        assert_eq!(
            pixel(frames[6], size, x, y),
            rgba,
            "generated 2x2 mip at {x},{y}"
        );
    }
    for p in frames[7].chunks_exact(4) {
        assert!(
            (126..=129).contains(&p[0]) && p[0] == p[1] && p[1] == p[2] && p[3] == 255,
            "generated 1x1 mip should be mid grey: {p:?}"
        );
    }
    finish(gpu);
}

#[test]
#[ignore = "requires a Vulkan device; run cargo test --test gpu -- --ignored"]
fn renders_to_a_texture_then_samples_it_in_later_passes() {
    let mut gpu = device();
    let size = 32;
    let rt = target(&mut gpu, size, size, TextureUsage::SAMPLED);
    let outs = [0, 1].map(|_| target(&mut gpu, size, size, TextureUsage::empty()));
    let fs = sample_fs();
    let sample = fullscreen_pipeline(&mut gpu, &fs);
    let sampler = nearest(&mut gpu, None);
    let group = gpu
        .create_bind_group(
            sample,
            0,
            &[(0, Resource::Texture(rt)), (1, Resource::Sampler(sampler))],
        )
        .unwrap();
    let (vs, cs) = (mesh_vs(), color_fs());
    let mesh = gpu
        .create_render_pipeline(&RenderPipelineDesc {
            vertex: &vs,
            fragment: Some(&cs),
            vertex_layout: Some(mesh_layout()),
            topology: Topology::TriangleList,
            cull: Cull::None,
            front_ccw: true,
            depth: None,
            colors: vec![color_target(Format::Rgba8Unorm)],
        })
        .unwrap();
    let triangle = vertices(&[
        ([-0.5, -0.5, 0.5], BLUE),
        ([0.5, -0.5, 0.5], BLUE),
        ([0.0, 0.5, 0.5], BLUE),
    ]);
    let vb = gpu
        .create_buffer(&BufferDesc {
            size: triangle.len() as u64,
            usage: BufferUsage::VERTEX,
            memory: Memory::Upload,
        })
        .unwrap();
    gpu.write_buffer(vb, 0, &triangle).unwrap();
    let bytes = u64::from(size * size * 4);
    let out = readback(&mut gpu, bytes * 2);
    let sample_into = |e: &mut Encoder, view: TextureId| {
        e.begin_pass(PassDesc {
            colors: vec![Attachment::clear(view, [0.0; 4])],
            depth: None,
        });
        e.set_pipeline(sample);
        e.set_bind_group(0, group);
        e.push_constants(&f32_bytes(&[0.0]));
        e.draw(3, 1, 0, 0);
        e.end_pass();
    };
    let mut e = gpu.encoder();
    // Pass 1 writes the texture, pass 2 reads it, pass 3 overwrites it (write after read),
    // pass 4 reads the new contents.
    e.begin_pass(PassDesc {
        colors: vec![Attachment::clear(rt, [1.0, 0.0, 0.0, 1.0])],
        depth: None,
    });
    e.end_pass();
    sample_into(&mut e, outs[0]);
    e.begin_pass(PassDesc {
        colors: vec![Attachment::clear(rt, [0.0, 1.0, 0.0, 1.0])],
        depth: None,
    });
    e.set_pipeline(mesh);
    e.set_vertex_buffer(vb, 0);
    e.push_constants(&f32_bytes(&[0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0]));
    e.draw(3, 1, 0, 0);
    e.end_pass();
    sample_into(&mut e, outs[1]);
    e.copy_texture_to_buffer(outs[0], 0, 0, out, 0);
    e.copy_texture_to_buffer(outs[1], 0, 0, out, bytes);
    gpu.submit(e).unwrap();
    let data = gpu.read_buffer(out).unwrap();
    let (first, second) = data.split_at(bytes as usize);
    assert!(first.chunks_exact(4).all(|p| p == RED));
    assert_eq!(pixel(second, size, 16, 16), BLUE);
    assert_eq!(pixel(second, size, 1, 1), GREEN);
    assert_eq!(pixel(second, size, 30, 30), GREEN);

    // Rendering to a texture while sampling it in the same pass is rejected, and the device
    // keeps working afterwards.
    let mut e = gpu.encoder();
    e.begin_pass(PassDesc {
        colors: vec![Attachment::clear(rt, [0.0; 4])],
        depth: None,
    });
    e.set_pipeline(sample);
    e.set_bind_group(0, group);
    e.end_pass();
    let err = gpu.submit(e).unwrap_err();
    assert!(
        err.contains("colour attachment") && err.contains("sampled"),
        "{err}"
    );
    let mut e = gpu.encoder();
    sample_into(&mut e, outs[0]);
    e.copy_texture_to_buffer(outs[0], 0, 0, out, 0);
    gpu.submit(e).unwrap();
    assert_eq!(pixel(&gpu.read_buffer(out).unwrap(), size, 16, 16), BLUE);
    finish(gpu);
}

#[test]
#[ignore = "requires a Vulkan device; run cargo test --test gpu -- --ignored"]
fn depth_array_layer_is_rendered_then_compared_through_layer_and_array_views() {
    let mut gpu = device();
    let size = 32;
    let usage = TextureUsage::DEPTH_TARGET | TextureUsage::SAMPLED;
    let shadow = gpu
        .create_texture(&TextureDesc {
            layers: 2,
            ..TextureDesc::new(size, size, Format::Depth32Float, usage)
        })
        .unwrap();
    let vs = quad_vs();
    let depth_only = gpu
        .create_render_pipeline(&RenderPipelineDesc {
            vertex: &vs,
            fragment: None,
            vertex_layout: None,
            topology: Topology::TriangleList,
            cull: Cull::None,
            front_ccw: true,
            depth: Some(depth_state(Compare::LessEqual)),
            colors: vec![],
        })
        .unwrap();
    let (single_fs, array_fs) = (shadow_fs(false), shadow_fs(true));
    let single = fullscreen_pipeline(&mut gpu, &single_fs);
    let array = fullscreen_pipeline(&mut gpu, &array_fs);
    let compare = nearest(&mut gpu, Some(Compare::LessEqual));
    let layer_group = gpu
        .create_bind_group(
            single,
            0,
            &[
                (0, Resource::TextureLayer(shadow, 1)),
                (1, Resource::Sampler(compare)),
            ],
        )
        .unwrap();
    let array_group = gpu
        .create_bind_group(
            array,
            0,
            &[
                (0, Resource::Texture(shadow)),
                (1, Resource::Sampler(compare)),
            ],
        )
        .unwrap();
    // A plain sampler or a whole-array resource does not fit these bindings.
    let plain = nearest(&mut gpu, None);
    let err = gpu
        .create_bind_group(
            single,
            0,
            &[
                (0, Resource::Texture(shadow)),
                (1, Resource::Sampler(compare)),
            ],
        )
        .unwrap_err();
    assert!(err.contains("TextureLayer"), "{err}");
    let err = gpu
        .create_bind_group(
            single,
            0,
            &[
                (0, Resource::TextureLayer(shadow, 1)),
                (1, Resource::Sampler(plain)),
            ],
        )
        .unwrap_err();
    assert!(err.contains("sampler_comparison"), "{err}");

    let outs = [0, 1, 2].map(|_| target(&mut gpu, size, size, TextureUsage::empty()));
    let bytes = u64::from(size * size * 4);
    let out = readback(&mut gpu, bytes * 3);
    let depth_pass = |e: &mut Encoder, layer: u32, clear: f32| {
        e.begin_pass(PassDesc {
            colors: vec![],
            depth: Some(Attachment {
                layer,
                ..Attachment::clear(shadow, [clear, 0.0, 0.0, 0.0])
            }),
        });
    };
    let mut e = gpu.encoder();
    depth_pass(&mut e, 0, 0.0);
    e.end_pass();
    depth_pass(&mut e, 1, 1.0);
    e.set_pipeline(depth_only);
    // Left half at depth 0.5.
    e.push_constants(&f32_bytes(&[-1.0, -1.0, 0.0, 1.0, 0.5]));
    e.draw(6, 1, 0, 0);
    e.end_pass();
    let compare_into = |e: &mut Encoder, view, pipeline, group, push: &[f32]| {
        e.begin_pass(PassDesc {
            colors: vec![Attachment::clear(view, [0.5; 4])],
            depth: None,
        });
        e.set_pipeline(pipeline);
        e.set_bind_group(0, group);
        e.push_constants(&f32_bytes(push));
        e.draw(3, 1, 0, 0);
        e.end_pass();
    };
    // Lit where 0.75 <= stored depth.
    compare_into(&mut e, outs[0], single, layer_group, &[0.75]);
    compare_into(&mut e, outs[1], array, array_group, &[0.75, 1.0]);
    compare_into(&mut e, outs[2], array, array_group, &[0.75, 0.0]);
    for (i, view) in outs.iter().enumerate() {
        e.copy_texture_to_buffer(*view, 0, 0, out, bytes * i as u64);
    }
    gpu.submit(e).unwrap();
    let data = gpu.read_buffer(out).unwrap();
    let frames: Vec<&[u8]> = data.chunks_exact(bytes as usize).collect();
    for frame in &frames[..2] {
        for y in 0..size {
            for x in 0..size {
                let expected = if x < size / 2 { BLACK } else { WHITE };
                assert_eq!(pixel(frame, size, x, y), expected, "at {x},{y}");
            }
        }
    }
    assert!(
        frames[2].chunks_exact(4).all(|p| p == BLACK),
        "layer 0 holds depth 0"
    );
    finish(gpu);
}

#[test]
#[ignore = "requires a Vulkan device; run cargo test --test gpu -- --ignored"]
fn compute_writes_storage_buffers_and_textures() {
    let mut gpu = device();
    // Two parameter blocks in one uniform buffer, the second at the minimum offset alignment.
    let align = gpu.limits().uniform_offset_alignment.max(16);
    let params = gpu
        .create_buffer(&BufferDesc {
            size: align + 16,
            usage: BufferUsage::UNIFORM,
            memory: Memory::Upload,
        })
        .unwrap();
    gpu.write_buffer(params, 0, &u32_bytes(&[1000, 16, 0, 0]))
        .unwrap();
    gpu.write_buffer(params, align, &u32_bytes(&[5000, 16, 0, 0]))
        .unwrap();
    let storage = |gpu: &mut Device<'_>| {
        gpu.create_buffer(&BufferDesc {
            size: 256 * 4,
            usage: BufferUsage::STORAGE | BufferUsage::COPY_SRC,
            memory: Memory::Device,
        })
        .unwrap()
    };
    let (values, scaled) = (storage(&mut gpu), storage(&mut gpu));
    let usage = TextureUsage::STORAGE | TextureUsage::COPY_SRC;
    let image = gpu
        .create_texture(&TextureDesc::new(16, 16, Format::Rgba8Unorm, usage))
        .unwrap();
    let fill = gpu.create_compute_pipeline(&fill_cs()).unwrap();
    let scale = gpu.create_compute_pipeline(&scale_cs()).unwrap();
    let whole = |id| Resource::Buffer {
        id,
        offset: 0,
        size: 0,
    };
    let fill_groups = [0, align].map(|offset| {
        let block = Resource::Buffer {
            id: params,
            offset,
            size: 16,
        };
        gpu.create_bind_group(
            fill,
            0,
            &[
                (0, block),
                (1, whole(values)),
                (2, Resource::Texture(image)),
            ],
        )
        .unwrap()
    });
    let misaligned = Resource::Buffer {
        id: params,
        offset: 4,
        size: 8,
    };
    let err = gpu
        .create_bind_group(
            fill,
            0,
            &[
                (0, misaligned),
                (1, whole(values)),
                (2, Resource::Texture(image)),
            ],
        )
        .unwrap_err();
    assert!(err.contains("multiple of"), "{err}");
    let scale_group = gpu
        .create_bind_group(scale, 0, &[(0, whole(values)), (1, whole(scaled))])
        .unwrap();
    let out = readback(&mut gpu, 256 * 4 * 3);
    for round in 0..2u32 {
        let base = [1000, 5000][round as usize];
        let mut e = gpu.encoder();
        e.dispatch(fill, &[(0, fill_groups[round as usize])], &[], 2, 2, 1);
        e.dispatch(
            scale,
            &[(0, scale_group)],
            &u32_bytes(&[256, 7 + round]),
            4,
            1,
            1,
        );
        e.copy_buffer_to_buffer(values, 0, out, 0, 1024);
        e.copy_buffer_to_buffer(scaled, 0, out, 1024, 1024);
        e.copy_texture_to_buffer(image, 0, 0, out, 2048);
        gpu.submit(e).unwrap();
        let data = gpu.read_buffer(out).unwrap();
        let words: Vec<u32> = data[..2048]
            .chunks_exact(4)
            .map(|w| u32::from_le_bytes(w.try_into().unwrap()))
            .collect();
        for i in 0..256 {
            assert_eq!(words[i], base + 3 * i as u32);
            assert_eq!(words[256 + i], (base + 3 * i as u32) * 2 + 7 + round);
        }
        for y in 0..16 {
            for x in 0..16 {
                let expected = [x as u8, y as u8, (x + y) as u8, 255];
                assert_eq!(pixel(&data[2048..], 16, x, y), expected);
            }
        }
    }
    finish(gpu);
}

#[test]
#[ignore = "requires a Vulkan device; run cargo test --test gpu -- --ignored"]
fn blits_scale_between_textures_and_mips() {
    let mut gpu = device();
    let usage = TextureUsage::COPY_SRC | TextureUsage::COPY_DST;
    let colors = [RED, GREEN, BLUE, WHITE];
    let source = gpu
        .create_texture(&TextureDesc::new(32, 32, Format::Rgba8Unorm, usage))
        .unwrap();
    gpu.write_texture(source, 0, 0, &quadrants(32, colors))
        .unwrap();
    let linear = gpu
        .create_texture(&TextureDesc::new(8, 8, Format::Rgba8Unorm, usage))
        .unwrap();
    let nearest = gpu
        .create_texture(&TextureDesc::new(4, 4, Format::Rgba8Unorm, usage))
        .unwrap();
    let chain = gpu
        .create_texture(&TextureDesc {
            mips: 2,
            ..TextureDesc::new(16, 16, Format::Rgba8Unorm, usage)
        })
        .unwrap();
    gpu.write_texture(chain, 0, 0, &quadrants(16, colors))
        .unwrap();
    let out = readback(&mut gpu, (64 + 16 + 64) * 4);
    let mut e = gpu.encoder();
    e.blit((source, 0, 0), (linear, 0, 0), Filter::Linear);
    e.blit((source, 0, 0), (nearest, 0, 0), Filter::Nearest);
    e.blit((chain, 0, 0), (chain, 1, 0), Filter::Linear);
    e.copy_texture_to_buffer(linear, 0, 0, out, 0);
    e.copy_texture_to_buffer(nearest, 0, 0, out, 256);
    e.copy_texture_to_buffer(chain, 1, 0, out, 320);
    gpu.submit(e).unwrap();
    let data = gpu.read_buffer(out).unwrap();
    for (offset, size) in [(0, 8), (256, 4), (320, 8)] {
        let expected = quadrants(size, colors);
        assert_eq!(&data[offset..offset + expected.len()], expected.as_slice());
    }
    let mut e = gpu.encoder();
    e.blit((chain, 1, 0), (chain, 1, 0), Filter::Nearest);
    assert!(gpu.submit(e).unwrap_err().contains("same subresource"));
    finish(gpu);
}

#[test]
#[ignore = "requires a Vulkan device; run cargo test --test gpu -- --ignored"]
fn reports_misuse_and_survives_deferred_destruction() {
    let mut gpu = device();
    let fs = sample_fs();
    let pipeline = fullscreen_pipeline(&mut gpu, &fs);
    let sampler = nearest(&mut gpu, None);
    let texture = gpu
        .create_texture(&TextureDesc::new(
            4,
            4,
            Format::Rgba8Unorm,
            TextureUsage::SAMPLED | TextureUsage::COPY_DST,
        ))
        .unwrap();
    let err = gpu
        .create_bind_group(pipeline, 0, &[(0, Resource::Texture(texture))])
        .unwrap_err();
    assert!(err.contains("missing"), "{err}");
    let err = gpu
        .create_bind_group(
            pipeline,
            0,
            &[
                (0, Resource::Sampler(sampler)),
                (1, Resource::Texture(texture)),
            ],
        )
        .unwrap_err();
    assert!(err.contains("expects"), "{err}");
    // Bindings and groups the shaders do not use are ignored; an unused group is empty and
    // cannot stand in for a used one.
    let unused = gpu.create_bind_group(pipeline, 1, &[]).unwrap();
    gpu.create_bind_group(
        pipeline,
        0,
        &[
            (0, Resource::Texture(texture)),
            (1, Resource::Sampler(sampler)),
            (7, Resource::Sampler(sampler)),
        ],
    )
    .unwrap();
    assert!(gpu.create_bind_group(pipeline, 8, &[]).is_err());
    assert!(gpu.write_texture(texture, 0, 0, &[0; 15]).is_err());
    // A vertex shader input without a matching attribute.
    let (vs, cs) = (mesh_vs(), color_fs());
    let err = gpu
        .create_render_pipeline(&RenderPipelineDesc {
            vertex: &vs,
            fragment: Some(&cs),
            vertex_layout: Some(VertexLayout {
                stride: 12,
                attributes: vec![VertexAttribute {
                    location: 0,
                    format: VertexFormat::F32x3,
                    offset: 0,
                }],
            }),
            topology: Topology::TriangleList,
            cull: Cull::None,
            front_ccw: true,
            depth: None,
            colors: vec![color_target(Format::Rgba8Unorm)],
        })
        .unwrap_err();
    assert!(err.contains("@location(1)"), "{err}");
    // A pipeline whose formats do not match the pass, and a draw without bind group.
    let view = target(&mut gpu, 4, 4, TextureUsage::empty());
    let hdr = gpu
        .create_texture(&TextureDesc::new(
            4,
            4,
            Format::Rgba16Float,
            TextureUsage::COLOR_TARGET,
        ))
        .unwrap();
    let mut e = gpu.encoder();
    e.begin_pass(PassDesc {
        colors: vec![Attachment::clear(hdr, [0.0; 4])],
        depth: None,
    });
    e.set_pipeline(pipeline);
    e.end_pass();
    assert!(gpu.submit(e).unwrap_err().contains("does not match"));
    let mut e = gpu.encoder();
    e.begin_pass(PassDesc {
        colors: vec![Attachment::clear(view, [0.0; 4])],
        depth: None,
    });
    e.set_pipeline(pipeline);
    e.push_constants(&f32_bytes(&[0.0]));
    e.draw(3, 1, 0, 0);
    e.end_pass();
    assert!(gpu.submit(e).unwrap_err().contains("@group(0)"));
    let mut e = gpu.encoder();
    e.begin_pass(PassDesc {
        colors: vec![Attachment::clear(view, [0.0; 4])],
        depth: None,
    });
    e.set_pipeline(pipeline);
    e.set_bind_group(0, unused);
    e.push_constants(&f32_bytes(&[0.0]));
    e.draw(3, 1, 0, 0);
    e.end_pass();
    assert!(gpu.submit(e).unwrap_err().contains("different layout"));
    gpu.destroy_bind_group(unused);

    // Destroyed objects are reported, and their ids are never reused as-is.
    let group = gpu
        .create_bind_group(
            pipeline,
            0,
            &[
                (0, Resource::Texture(texture)),
                (1, Resource::Sampler(sampler)),
            ],
        )
        .unwrap();
    gpu.write_texture(texture, 0, 0, &[7; 64]).unwrap();
    let out = readback(&mut gpu, 64);
    let mut e = gpu.encoder();
    e.begin_pass(PassDesc {
        colors: vec![Attachment::clear(view, [0.0; 4])],
        depth: None,
    });
    e.set_pipeline(pipeline);
    e.set_bind_group(0, group);
    e.push_constants(&f32_bytes(&[0.0]));
    e.draw(3, 1, 0, 0);
    e.end_pass();
    e.copy_texture_to_buffer(view, 0, 0, out, 0);
    gpu.submit(e).unwrap();
    assert!(gpu.read_buffer(out).unwrap().iter().all(|b| *b == 7));
    gpu.destroy_texture(texture);
    let replacement = gpu
        .create_texture(&TextureDesc::new(
            4,
            4,
            Format::Rgba8Unorm,
            TextureUsage::SAMPLED,
        ))
        .unwrap();
    assert_ne!(replacement, texture);
    assert!(gpu.texture_desc(texture).is_err());
    let mut e = gpu.encoder();
    e.begin_pass(PassDesc {
        colors: vec![Attachment::clear(view, [0.0; 4])],
        depth: None,
    });
    e.set_pipeline(pipeline);
    e.set_bind_group(0, group);
    e.end_pass();
    assert!(gpu.submit(e).unwrap_err().contains("destroyed"));
    gpu.destroy_bind_group(group);
    gpu.destroy_buffer(out);
    assert!(gpu.read_buffer(out).is_err());
    finish(gpu);
}

#[test]
#[ignore = "requires a Vulkan device; run cargo test --test gpu -- --ignored"]
fn renders_cube_faces_and_samples_them_as_a_cube() {
    let mut gpu = device();
    let usage = TextureUsage::COLOR_TARGET | TextureUsage::SAMPLED;
    let cube = gpu
        .create_texture(&TextureDesc {
            layers: 6,
            ..TextureDesc::new(8, 8, Format::Rgba8Unorm, usage)
        })
        .unwrap();
    let faces: [[u8; 4]; 6] = [
        RED,
        GREEN,
        BLUE,
        WHITE,
        [255, 255, 0, 255],
        [0, 255, 255, 255],
    ];
    let directions = [
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
    ];
    let fs = cube_fs();
    let pipeline = fullscreen_pipeline(&mut gpu, &fs);
    let sampler = nearest(&mut gpu, None);
    let group = gpu
        .create_bind_group(
            pipeline,
            0,
            &[
                (0, Resource::Texture(cube)),
                (1, Resource::Sampler(sampler)),
            ],
        )
        .unwrap();
    let view = target(&mut gpu, 4, 4, TextureUsage::empty());
    let out = readback(&mut gpu, 6 * 64);
    let mut e = gpu.encoder();
    for (layer, rgba) in faces.iter().enumerate() {
        let clear = rgba.map(|c| f32::from(c) / 255.0);
        e.begin_pass(PassDesc {
            colors: vec![Attachment {
                layer: layer as u32,
                ..Attachment::clear(cube, clear)
            }],
            depth: None,
        });
        e.end_pass();
    }
    for (i, d) in directions.iter().enumerate() {
        e.begin_pass(PassDesc {
            colors: vec![Attachment::clear(view, [0.0; 4])],
            depth: None,
        });
        e.set_pipeline(pipeline);
        e.set_bind_group(0, group);
        e.push_constants(&f32_bytes(&[d[0], d[1], d[2], 0.0]));
        e.draw(3, 1, 0, 0);
        e.end_pass();
        e.copy_texture_to_buffer(view, 0, 0, out, 64 * i as u64);
    }
    gpu.submit(e).unwrap();
    let data = gpu.read_buffer(out).unwrap();
    for (i, face) in data.chunks_exact(64).enumerate() {
        assert!(face.chunks_exact(4).all(|p| p == faces[i]), "face {i}");
    }
    finish(gpu);
}

#[test]
#[ignore = "requires a Vulkan device; run cargo test --test gpu -- --ignored"]
fn renders_each_mip_from_the_previous_one() {
    // A bloom-style chain: each pass renders one mip while sampling the mip above it.
    let mut gpu = device();
    let usage = TextureUsage::COLOR_TARGET
        | TextureUsage::SAMPLED
        | TextureUsage::COPY_SRC
        | TextureUsage::COPY_DST;
    let desc = TextureDesc {
        mips: 3,
        ..TextureDesc::new(16, 16, Format::Rgba8Unorm, usage)
    };
    let chain = gpu.create_texture(&desc).unwrap();
    let colors = [RED, GREEN, BLUE, WHITE];
    gpu.write_texture(chain, 0, 0, &quadrants(16, colors))
        .unwrap();
    let fs = sample_fs();
    let pipeline = fullscreen_pipeline(&mut gpu, &fs);
    let sampler = nearest(&mut gpu, None);
    let sources = [0, 1].map(|mip| {
        gpu.create_bind_group(
            pipeline,
            0,
            &[
                (0, Resource::TextureMip(chain, mip)),
                (1, Resource::Sampler(sampler)),
            ],
        )
        .unwrap()
    });
    let out = readback(&mut gpu, 256 + 64);
    let mut e = gpu.encoder();
    for mip in 1..3 {
        e.begin_pass(PassDesc {
            colors: vec![Attachment {
                mip,
                load: Load::DontCare,
                ..Attachment::clear(chain, [0.0; 4])
            }],
            depth: None,
        });
        e.set_pipeline(pipeline);
        e.set_bind_group(0, sources[mip as usize - 1]);
        e.push_constants(&f32_bytes(&[0.0]));
        e.draw(3, 1, 0, 0);
        e.end_pass();
    }
    e.copy_texture_to_buffer(chain, 1, 0, out, 0);
    e.copy_texture_to_buffer(chain, 2, 0, out, 256);
    gpu.submit(e).unwrap();
    let data = gpu.read_buffer(out).unwrap();
    assert_eq!(&data[..256], quadrants(8, colors).as_slice());
    assert_eq!(&data[256..], quadrants(4, colors).as_slice());
    finish(gpu);
}

#[test]
#[ignore = "requires a Vulkan device; run cargo test --test gpu -- --ignored"]
fn blends_within_scissor_and_viewport() {
    let mut gpu = device();
    let usage = TextureUsage::COLOR_TARGET | TextureUsage::COPY_SRC;
    let hdr = gpu
        .create_texture(&TextureDesc::new(8, 8, Format::Rgba16Float, usage))
        .unwrap();
    let (vs, fs) = (mesh_vs(), color_fs());
    let pipeline = |gpu: &mut Device<'_>, blend| {
        gpu.create_render_pipeline(&RenderPipelineDesc {
            vertex: &vs,
            fragment: Some(&fs),
            vertex_layout: Some(mesh_layout()),
            topology: Topology::TriangleList,
            cull: Cull::None,
            front_ccw: true,
            depth: None,
            colors: vec![ColorTarget {
                format: Format::Rgba16Float,
                blend,
                write_mask: 0xF,
            }],
        })
        .unwrap()
    };
    let additive = pipeline(&mut gpu, Blend::Additive);
    let alpha = pipeline(&mut gpu, Blend::Alpha);
    // One triangle covering the whole target; the push-constant tint is the colour.
    let cover = vertices(&[
        ([-1.0, -1.0, 0.5], WHITE),
        ([3.0, -1.0, 0.5], WHITE),
        ([-1.0, 3.0, 0.5], WHITE),
    ]);
    let vb = gpu
        .create_buffer(&BufferDesc {
            size: cover.len() as u64,
            usage: BufferUsage::VERTEX,
            memory: Memory::Upload,
        })
        .unwrap();
    gpu.write_buffer(vb, 0, &cover).unwrap();
    let out = readback(&mut gpu, 8 * 8 * 8);
    let tint =
        |rgba: [f32; 4]| f32_bytes(&[0.0, 0.0, 1.0, 0.0, rgba[0], rgba[1], rgba[2], rgba[3]]);
    let mut e = gpu.encoder();
    e.begin_pass(PassDesc {
        colors: vec![Attachment::clear(hdr, [0.25, 0.5, 0.0, 1.0])],
        depth: None,
    });
    e.set_vertex_buffer(vb, 0);
    e.set_pipeline(additive);
    e.push_constants(&tint([0.5, 0.25, 0.125, 0.0]));
    e.draw(3, 1, 0, 0);
    // Left half: alpha blend half-transparent red.
    e.set_scissor(0, 0, 4, 8);
    e.set_pipeline(alpha);
    e.push_constants(&tint([1.0, 0.0, 0.0, 0.5]));
    e.draw(3, 1, 0, 0);
    // Bottom-right quadrant through the viewport: add blue.
    e.set_scissor(0, 0, 8, 8);
    e.set_viewport(4.0, 4.0, 4.0, 4.0);
    e.set_pipeline(additive);
    e.push_constants(&tint([0.0, 0.0, 0.5, 0.0]));
    e.draw(3, 1, 0, 0);
    e.end_pass();
    e.copy_texture_to_buffer(hdr, 0, 0, out, 0);
    gpu.submit(e).unwrap();
    let data = gpu.read_buffer(out).unwrap();
    let texel = |x: u32, y: u32| -> [u16; 4] {
        let i = ((y * 8 + x) * 8) as usize;
        core::array::from_fn(|c| u16::from_le_bytes([data[i + 2 * c], data[i + 2 * c + 1]]))
    };
    // f16: 1.0 = 3C00, 0.875 = 3B00, 0.75 = 3A00, 0.625 = 3900, 0.375 = 3600, 0.125 = 3000,
    // 0.0625 = 2C00.
    assert_eq!(
        texel(6, 1),
        [0x3A00, 0x3A00, 0x3000, 0x3C00],
        "additive only"
    );
    assert_eq!(
        texel(1, 1),
        [0x3B00, 0x3600, 0x2C00, 0x3C00],
        "alpha over additive"
    );
    assert_eq!(
        texel(6, 6),
        [0x3A00, 0x3A00, 0x3900, 0x3C00],
        "viewport quadrant"
    );
    assert_eq!(
        texel(1, 6),
        [0x3B00, 0x3600, 0x2C00, 0x3C00],
        "outside the viewport"
    );
    finish(gpu);
}

#[test]
#[ignore = "requires a Vulkan device; run cargo test --test gpu -- --ignored"]
fn creates_uses_and_recycles_many_bind_groups() {
    let mut gpu = device();
    let fs = sample_fs();
    let pipeline = fullscreen_pipeline(&mut gpu, &fs);
    let sampler = nearest(&mut gpu, None);
    let usage = TextureUsage::SAMPLED | TextureUsage::COPY_DST;
    let texture = gpu
        .create_texture(&TextureDesc::new(1, 1, Format::Rgba8Unorm, usage))
        .unwrap();
    gpu.write_texture(texture, 0, 0, &GREEN).unwrap();
    let view = target(&mut gpu, 2, 2, TextureUsage::empty());
    let out = readback(&mut gpu, 16);
    let entries = [
        (0, Resource::Texture(texture)),
        (1, Resource::Sampler(sampler)),
    ];
    for round in 0..3 {
        // More sets than one descriptor pool holds.
        let groups: Vec<BindGroupId> = (0..700)
            .map(|_| gpu.create_bind_group(pipeline, 0, &entries).unwrap())
            .collect();
        let mut e = gpu.encoder();
        e.begin_pass(PassDesc {
            colors: vec![Attachment::clear(view, [0.0; 4])],
            depth: None,
        });
        e.set_pipeline(pipeline);
        e.push_constants(&f32_bytes(&[0.0]));
        for g in groups.iter().step_by(97) {
            e.set_bind_group(0, *g);
            e.draw(3, 1, 0, 0);
        }
        e.end_pass();
        e.copy_texture_to_buffer(view, 0, 0, out, 0);
        gpu.submit(e).unwrap();
        assert_eq!(
            gpu.read_buffer(out).unwrap(),
            GREEN.repeat(4),
            "round {round}"
        );
        for g in groups {
            gpu.destroy_bind_group(g);
        }
    }
    finish(gpu);
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
#[test]
#[ignore = "requires a display and a Vulkan presentation driver"]
fn presents_frames_across_a_resize() {
    use abe_engine::window::Window;
    use std::time::{Duration, Instant};
    #[cfg(target_os = "linux")]
    if std::env::var_os("DISPLAY").is_none() {
        eprintln!("skipping: DISPLAY is not set");
        return;
    }
    let window = match Window::new("ABE gpu presentation test", 320, 200) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("skipping: {e}");
            return;
        }
    };
    let pump_until = |condition: &dyn Fn() -> bool| {
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            while window.poll().is_some() {}
            if condition() {
                return;
            }
            assert!(
                Instant::now() < end,
                "window did not reach the requested state"
            );
            std::thread::sleep(Duration::from_millis(3));
        }
    };
    pump_until(&|| window.visible());
    let mut gpu = Device::for_window(&window).unwrap();
    let format = gpu.surface_format();
    eprintln!("device: {} | surface {format:?}", gpu.name());
    assert!(matches!(
        format,
        Format::Bgra8Srgb | Format::Bgra8Unorm | Format::Rgba8Srgb | Format::Rgba8Unorm
    ));
    let (vs, fs) = (mesh_vs(), color_fs());
    let pipeline = gpu
        .create_render_pipeline(&RenderPipelineDesc {
            vertex: &vs,
            fragment: Some(&fs),
            vertex_layout: Some(mesh_layout()),
            topology: Topology::TriangleList,
            cull: Cull::None,
            front_ccw: true,
            depth: Some(depth_state(Compare::Less)),
            colors: vec![color_target(format)],
        })
        .unwrap();
    let triangle = vertices(&[
        ([-0.5, -0.5, 0.5], GREEN),
        ([0.5, -0.5, 0.5], GREEN),
        ([0.0, 0.5, 0.5], GREEN),
    ]);
    // One vertex buffer per frame slot, rewritten every frame.
    let slots = [0, 1].map(|_| {
        gpu.create_buffer(&BufferDesc {
            size: triangle.len() as u64,
            usage: BufferUsage::VERTEX,
            memory: Memory::Upload,
        })
        .unwrap()
    });
    let usage = TextureUsage::COPY_SRC | TextureUsage::COPY_DST;
    let tile = gpu
        .create_texture(&TextureDesc::new(2, 2, Format::Rgba8Unorm, usage))
        .unwrap();
    gpu.write_texture(tile, 0, 0, &solid(2, 2, RED)).unwrap();
    let mut depth: Option<(TextureId, (u32, u32))> = None;
    let mut shot = None;
    let mut sizes = Vec::new();
    let (mut presented, mut swapchains_seen) = (0, Vec::new());
    let end = Instant::now() + Duration::from_secs(30);
    while presented < 30 {
        assert!(Instant::now() < end, "timed out presenting");
        while window.poll().is_some() {}
        if presented == 15 && window.size() != (400, 240) {
            window.resize(400, 240);
            pump_until(&|| window.size() == (400, 240));
        }
        let Some(frame) = gpu.acquire().unwrap() else {
            std::thread::sleep(Duration::from_millis(2));
            continue;
        };
        assert_eq!(gpu.acquire().unwrap(), Some(frame), "acquire is idempotent");
        if !swapchains_seen.contains(&frame) {
            swapchains_seen.push(frame);
        }
        let size = gpu.surface_size();
        let desc = gpu.texture_desc(frame).unwrap();
        assert_eq!((desc.width, desc.height), size);
        if depth.is_none_or(|(_, s)| s != size) {
            // Destroying a texture an in-flight frame still uses is deferred, not a hazard.
            if let Some((old, _)) = depth.take() {
                gpu.destroy_texture(old);
            }
            let d = TextureDesc::new(
                size.0,
                size.1,
                Format::Depth32Float,
                TextureUsage::DEPTH_TARGET,
            );
            depth = Some((gpu.create_texture(&d).unwrap(), size));
            sizes.push(size);
        }
        let vb = slots[gpu.frame_slot()];
        gpu.write_buffer(vb, 0, &triangle).unwrap();
        let mut e = gpu.encoder();
        let blit = presented % 3 == 0 && desc.usage.contains(TextureUsage::COPY_DST);
        if blit {
            e.blit((tile, 0, 0), (frame, 0, 0), Filter::Nearest);
        }
        e.begin_pass(PassDesc {
            colors: vec![Attachment {
                load: if blit {
                    Load::Load
                } else {
                    Load::Clear([0.0, 0.0, 1.0, 1.0])
                },
                ..Attachment::clear(frame, [0.0; 4])
            }],
            depth: Some(Attachment {
                store: false,
                ..Attachment::clear(depth.unwrap().0, [1.0; 4])
            }),
        });
        e.set_pipeline(pipeline);
        e.set_vertex_buffer(vb, 0);
        e.push_constants(&f32_bytes(&[0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0]));
        e.draw(3, 1, 0, 0);
        e.end_pass();
        if presented == 25 && desc.usage.contains(TextureUsage::COPY_SRC) {
            let buffer = readback(&mut gpu, u64::from(size.0 * size.1 * 4));
            e.copy_texture_to_buffer(frame, 0, 0, buffer, 0);
            shot = Some((buffer, size, blit));
        }
        gpu.present(e).unwrap();
        presented += 1;
    }
    assert_eq!(gpu.surface_size(), (400, 240));
    assert_eq!(sizes.first(), Some(&(320, 200)));
    assert_eq!(sizes.last(), Some(&(400, 240)));
    assert!(swapchains_seen.len() >= 2);
    if let Some((buffer, (w, h), blit)) = shot {
        let data = gpu.read_buffer(buffer).unwrap();
        let memory_order = |[r, g, b, a]: [u8; 4]| match format {
            Format::Bgra8Srgb | Format::Bgra8Unorm => [b, g, r, a],
            _ => [r, g, b, a],
        };
        assert_eq!(pixel(&data, w, w / 2, h / 2), memory_order(GREEN));
        let background = if blit { RED } else { BLUE };
        assert_eq!(pixel(&data, w, 1, 1), memory_order(background));
    } else {
        eprintln!("the surface cannot be copied from; skipped the pixel check");
    }

    // Hidden windows have nothing to acquire; presentation resumes when shown again.
    window.set_visible(false);
    pump_until(&|| !window.visible());
    assert_eq!(gpu.acquire().unwrap(), None);
    window.set_visible(true);
    pump_until(&|| window.visible());
    let mut resumed = 0;
    while resumed < 5 {
        assert!(Instant::now() < end, "timed out presenting after restore");
        while window.poll().is_some() {}
        let Some(frame) = gpu.acquire().unwrap() else {
            std::thread::sleep(Duration::from_millis(2));
            continue;
        };
        let mut e = gpu.encoder();
        e.begin_pass(PassDesc {
            colors: vec![Attachment::clear(frame, [0.2, 0.2, 0.2, 1.0])],
            depth: None,
        });
        e.end_pass();
        gpu.present(e).unwrap();
        resumed += 1;
    }
    // Frames remain in flight here; dropping the device waits for them.
    finish(gpu);
}
