//! A self-test of the parts of the GPU layer the renderer does not use: a compute dispatch with
//! push constants writing a storage texture and a storage buffer, texture-to-buffer and
//! buffer-to-buffer copies, mip generation and a blit into the canvas texture.
//!
//! The canvas then shows mip 1 of a 64x64 gradient: red grows to the right, green downwards,
//! blue is 0.5 everywhere. `web/test/run.cjs --selftest` checks those pixels and that WebGPU
//! reported no error.
use abe_engine::gpu::{
    BufferDesc, BufferUsage, Device, Filter, Format, Memory, Resource, TextureDesc, TextureUsage,
    compile_shader, f32_bytes, shader::Stage,
};

const SHADER: &str = r#"
struct Push { scale: vec4f };
var<push_constant> push: Push;
@group(0) @binding(0) var image: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(1) var<storage, read_write> marks: array<u32>;

@compute @workgroup_size(8, 8, 1)
fn fill(@builtin(global_invocation_id) id: vec3u) {
  let uv = vec2f(id.xy) / 63.0;
  textureStore(image, vec2i(id.xy), vec4f(uv * push.scale.xy, push.scale.z, 1.0));
  if (all(id.xy == vec2u(0u))) {
    marks[0] = 0xabe0u;
  }
}
"#;

pub fn run() -> Result<String, String> {
    let mut d = Device::for_canvas()?;
    let shader = compile_shader(SHADER, Stage::Compute, "fill")?;
    let pipeline = d.create_compute_pipeline(&shader)?;
    let image = d.create_texture(&TextureDesc {
        width: 64,
        height: 64,
        layers: 1,
        mips: 7,
        format: Format::Rgba8Unorm,
        usage: TextureUsage::STORAGE
            | TextureUsage::SAMPLED
            | TextureUsage::COPY_SRC
            | TextureUsage::COPY_DST,
    })?;
    let marks = d.create_buffer(&BufferDesc {
        size: 256,
        usage: BufferUsage::STORAGE | BufferUsage::COPY_SRC,
        memory: Memory::Device,
    })?;
    let copy = d.create_buffer(&BufferDesc {
        size: 256,
        usage: BufferUsage::COPY_DST,
        memory: Memory::Device,
    })?;
    let readback = d.create_buffer(&BufferDesc {
        size: 64 * 64 * 4,
        usage: BufferUsage::COPY_DST,
        memory: Memory::Readback,
    })?;
    let group = d.create_bind_group(
        pipeline,
        0,
        &[
            (0, Resource::TextureMip(image, 0)),
            (
                1,
                Resource::Buffer {
                    id: marks,
                    offset: 0,
                    size: 0,
                },
            ),
        ],
    )?;
    let mut e = d.encoder();
    e.dispatch(
        pipeline,
        &[(0, group)],
        &f32_bytes(&[1.0, 1.0, 0.5, 0.0]),
        8,
        8,
        1,
    );
    e.copy_texture_to_buffer(image, 0, 0, readback, 0);
    e.copy_buffer_to_buffer(marks, 0, copy, 0, 256);
    d.submit(e)?;
    if d.read_buffer(readback).is_ok() {
        return Err("read_buffer should report that it is unavailable".into());
    }
    d.generate_mips(image)?;
    let canvas = d.acquire()?.ok_or("the canvas has no area")?;
    let mut e = d.encoder();
    e.blit((image, 1, 0), (canvas, 0, 0), Filter::Linear);
    d.present(e)?;
    // Misuse is reported at submission, as on Vulkan.
    let mut bad = d.encoder();
    bad.blit((image, 9, 0), (image, 0, 0), Filter::Nearest);
    let misuse = d
        .submit(bad)
        .err()
        .ok_or("an out-of-range blit was accepted")?;
    for b in [marks, copy, readback] {
        d.destroy_buffer(b);
    }
    d.destroy_texture(image);
    Ok(format!(
        "self-test on {}: compute, copies, mips and a canvas blit submitted; misuse reported: {misuse}",
        d.name()
    ))
}
