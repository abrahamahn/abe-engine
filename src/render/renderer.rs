//! The stylized renderer's frame: two shadow cascades, a top-down ground map (what is highest
//! around the view and whether grass grows there: rain cover, grass placement, water depth), a
//! mirrored reflection pass for water, the main forward pass (world, grass and flowers, sky,
//! weather particles) into HDR colour plus normals, then ambient occlusion, air (outlines,
//! aerial perspective, mist, light shafts), bloom, tone mapping and grading, the optional
//! painterly filter and the final output with sharpening (a swapchain image or a readback
//! target).

use crate::gpu::{
    Address, Attachment, BindGroupId, Blend, BufferDesc, BufferId, BufferUsage, ColorTarget,
    Compare, Cull, DepthState, Device, Encoder, Filter, Format, Load, Memory, PassDesc, PipelineId,
    RenderPipelineDesc, Resource, Result, SamplerDesc, SamplerId, TextureDesc, TextureId,
    TextureUsage, Topology, VertexAttribute, VertexFormat, VertexLayout, f32_bytes,
};
use abe_shader::{Compiled, Stage};

use super::look::{DetailPreset, Look, View};
use super::math::{
    self, M4, V3, add, cross, invert, look_at, mul, normalize, ortho, perspective, reflect_y,
    scale, sub, transform,
};
use super::scene::GpuScene;
use super::sky::Environment;

const COMMON: &str = include_str!("shaders/common.wgsl");
const SCENE: &str = include_str!("shaders/scene.wgsl");
const WORLD_FX: &str = include_str!("shaders/world_fx.wgsl");
const POST: &str = include_str!("shaders/post.wgsl");
const PROP_SHADER_INPUT: &str = include_str!("shaders/prop_shader_input.wgsl");
const PROP_SHADER: &str = include_str!("shaders/prop_shader.wgsl");
const HDR: Format = Format::Rgba16Float;
const MAX_LIGHTS_PER_TILE: usize = 96;
/// The near clip distance of `view`, kept positive.
fn near(view: &View) -> f32 {
    if view.near > 1e-4 { view.near } else { 1e-4 }
}

/// `view.up`, or a perpendicular fallback when it is (nearly) along the line of sight.
fn camera_up(view: &View) -> V3 {
    let fwd = normalize(sub(view.target, view.eye));
    if math::length(cross(fwd, view.up)) > 1e-4 {
        view.up
    } else if fwd[1].abs() > 0.9 {
        [0.0, 0.0, -1.0]
    } else {
        [0.0, 1.0, 0.0]
    }
}
/// The ground map covers this far (metres) around a point just ahead of the camera.
const GROUND_REACH: f32 = 48.0;
/// The world map covers the land's bounds plus this margin (metres) on every side, so shallows
/// and foam reach out past the outermost shore.
const WORLD_MARGIN: f32 = 16.0;
/// ...but reaches at most this far (metres) from where the land's detail is densest, so distant
/// scenery does not spread its texels thin.
const WORLD_MAX_REACH: f32 = 560.0;
/// The grass instance grid is at most GRASS_SIDE x GRASS_SIDE cells of 13 cm around the camera
/// (see scene.wgsl); `Quality::grass` scales its area.
const GRASS_SIDE: u32 = 640;
const RAIN_BOX: f32 = 16.0;
const PETAL_BOX: f32 = 14.0;
const GROUND_FORMAT: Format = Format::Rgba8Unorm;
/// Size of the per-view uniform buffer holding the `Frame` block.
const FRAME_BYTES: u64 = 1536;

/// How much work each frame does. [`Quality::still`] is for offline images,
/// [`Quality::realtime`] for the interactive player.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quality {
    pub shadow_size: u32,
    pub reflection_scale: f32,
    pub ao_scale: f32,
    pub ao_samples: u32,
    pub tile_size: u32,
    pub bloom_levels: u32,
    pub painterly: bool,
    pub fxaa: bool,
    pub particles: bool,
    /// Texels per side of the top-down ground map.
    pub ground_size: u32,
    /// Texels per side of the world map (shallows and foam beyond the ground map).
    pub world_size: u32,
    /// Fraction of the full grass area drawn around the camera (0 = no grass).
    pub grass: f32,
}
impl Quality {
    pub fn still() -> Self {
        Self {
            shadow_size: 4096,
            reflection_scale: 1.0,
            ao_scale: 1.0,
            ao_samples: 16,
            tile_size: 16,
            bloom_levels: 6,
            painterly: true,
            fxaa: false,
            particles: true,
            ground_size: 1024,
            world_size: 2048,
            grass: 1.0,
        }
    }
    pub fn realtime() -> Self {
        Self {
            shadow_size: 2048,
            reflection_scale: 0.5,
            ao_scale: 0.5,
            ao_samples: 8,
            tile_size: 32,
            bloom_levels: 5,
            painterly: true,
            fxaa: true,
            particles: true,
            ground_size: 1024,
            world_size: 1024,
            grass: 0.6,
        }
    }
}

/// Where the final pass writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    /// An offscreen RGBA16F target read back as linear, display-referred colour.
    Still,
    /// A presentation surface of this format (the final pass encodes sRGB when the format
    /// does not).
    Surface(Format),
}

pub(super) struct Pipelines {
    pub(super) scene: PipelineId,
    /// The scene shading for props (model-space vertices placed per instance).
    pub(super) scene_prop: PipelineId,
    /// The same, blended over the scene without writing depth: alpha, additive.
    pub(super) scene_prop_blend: [PipelineId; 2],
    /// Props over the finished image: [opaque, alpha, additive][without, with depth test].
    pub(super) overlay: [[PipelineId; 2]; 3],
    /// What a game's prop shader is built against: the output format, the prop vertex layout,
    /// and the resources the built-in overlay fragment reads (a prop shader may read no others,
    /// so it takes the same bind groups).
    pub(super) output_format: Format,
    pub(super) prop_layout: VertexLayout,
    pub(super) overlay_bindings: Vec<abe_shader::Binding>,
    shadow: PipelineId,
    shadow_masked: PipelineId,
    pub(super) shadow_prop: PipelineId,
    pub(super) shadow_prop_masked: PipelineId,
    ground: PipelineId,
    grass: PipelineId,
    flower: PipelineId,
    sky: PipelineId,
    particle: PipelineId,
    ssao: PipelineId,
    air: PipelineId,
    bloom_down: PipelineId,
    bloom_up: PipelineId,
    tonemap: PipelineId,
    paint: PipelineId,
    output: PipelineId,
}

pub(super) struct Targets {
    width: u32,
    height: u32,
    hdr: TextureId,
    aux: TextureId,
    depth: TextureId,
    refl: (u32, u32),
    pub(super) refl_hdr: TextureId,
    refl_aux: TextureId,
    refl_depth: TextureId,
    ao: (u32, u32),
    ao_tex: TextureId,
    lit: TextureId,
    bloom: TextureId,
    bloom_size: (u32, u32),
    bloom_levels: u32,
    ldr: TextureId,
    paint: TextureId,
    still: Option<TextureId>,
    readback: Option<BufferId>,
    tiles: [(u32, u32); 2],
}

/// Per-frame-slot uniform buffers, light tiles and the bind groups that reference them.
pub(super) struct Slot {
    pub(super) frame: [BufferId; 2],
    pub(super) tiles: [BufferId; 2],
    pub(super) index: [BufferId; 2],
    scene: [BindGroupId; 2],
    sky: [BindGroupId; 2],
    grass: BindGroupId,
    flower: BindGroupId,
    particle: BindGroupId,
    ssao: BindGroupId,
    air: BindGroupId,
    down: Vec<BindGroupId>,
    up: Vec<BindGroupId>,
    tonemap: BindGroupId,
    paint: BindGroupId,
    output: BindGroupId,
}

pub struct Renderer {
    pub(super) scene: GpuScene,
    pub(super) pipelines: Pipelines,
    pub(super) quality: Quality,
    pub(super) output: Output,
    pub(super) look: Look,
    pub(super) targets: Targets,
    pub(super) slots: Vec<Slot>,
    pub(super) materials: Vec<BindGroupId>,
    /// Bind groups for alpha-masked materials in the masked shadow pipeline.
    pub(super) shadow_materials: Vec<Option<BindGroupId>>,
    pub(super) shadow_map: TextureId,
    pub(super) ground_depth: TextureId,
    pub(super) ground_mask: TextureId,
    /// The world map: the whole land seen from above once, for shallows and foam on shores
    /// beyond the ground map. Drawn on the first frame and again when the Look changes.
    pub(super) world_depth: TextureId,
    pub(super) world_mask: TextureId,
    world: Ground,
    pub(super) world_drawn: bool,
    /// Per material: the procedural detail block pushed with each draw (see `Draw` in
    /// scene.wgsl), from the Look's detail rules.
    pub(super) details: Vec<[f32; 16]>,
    /// Per material: grows grass (the ground map's mask).
    pub(super) grassy: Vec<bool>,
    pub(super) dummy: TextureId,
    pub(super) linear_clamp: SamplerId,
    pub(super) shadow_sampler: SamplerId,
    /// Fallback textures and the material sampler, shared with prop materials.
    pub(super) white: TextureId,
    pub(super) flat: TextureId,
    pub(super) material_sampler: SamplerId,
    pub(super) props: super::props::Props,
    pub(super) models: super::models::Models,
}

fn compile(body: &str, stage: Stage, entry: &str) -> Result<Compiled> {
    let source = format!("{COMMON}\n{body}");
    crate::gpu::compile_shader(&source, stage, entry)
}

/// The scene shader with a game's prop shader (`shade`) and the overlay entry point that calls it.
pub(super) fn prop_shader_source(user: &str) -> String {
    format!(
        "{COMMON}\n{SCENE}\n{PROP_SHADER_INPUT}\n// --- prop shader\n{user}\n// --- prop shader \
         entry\n{PROP_SHADER}"
    )
}

/// Overlay pipelines for a game's prop shader: [opaque, alpha, additive][without, with depth].
pub(super) fn prop_shader_pipelines(
    device: &mut Device<'_>,
    pipelines: &Pipelines,
    user: &str,
) -> Result<[[PipelineId; 2]; 3]> {
    let source = prop_shader_source(user);
    let vs = crate::gpu::compile_shader(&source, Stage::Vertex, "vs_prop")?;
    let fs = crate::gpu::compile_shader(&source, Stage::Fragment, "fs_prop_shader")?;
    if fs.bindings != pipelines.overlay_bindings {
        return Err(
            "a prop shader reads only its material's base texture and the draw's parameters \
             (`ShadeInput`); pass anything else, the time among it, in `params`"
                .into(),
        );
    }
    let depth = |compare| DepthState {
        format: Format::Depth32Float,
        compare,
        write: false,
        bias_constant: 0.0,
        bias_slope: 0.0,
        clamp: false,
    };
    let mut out = pipelines.overlay;
    for (i, blend) in [Blend::Replace, Blend::Premultiplied, Blend::Additive]
        .into_iter()
        .enumerate()
    {
        for (j, compare) in [Compare::Always, Compare::Greater].into_iter().enumerate() {
            out[i][j] = device.create_render_pipeline(&RenderPipelineDesc {
                vertex: &vs,
                fragment: Some(&fs),
                vertex_layout: Some(pipelines.prop_layout.clone()),
                topology: Topology::TriangleList,
                cull: Cull::None,
                front_ccw: true,
                depth: Some(depth(compare)),
                colors: vec![color(pipelines.output_format, blend)],
            })?;
        }
    }
    Ok(out)
}

fn color(format: Format, blend: Blend) -> ColorTarget {
    ColorTarget {
        format,
        blend,
        write_mask: 0xF,
    }
}

fn fullscreen(
    device: &mut Device<'_>,
    vs: &Compiled,
    entry: &str,
    format: Format,
    blend: Blend,
) -> Result<PipelineId> {
    let fs = compile(POST, Stage::Fragment, entry)?;
    device.create_render_pipeline(&RenderPipelineDesc {
        vertex: vs,
        fragment: Some(&fs),
        vertex_layout: None,
        topology: Topology::TriangleList,
        cull: Cull::None,
        front_ccw: true,
        depth: None,
        colors: vec![color(format, blend)],
    })
}

impl Pipelines {
    fn new(device: &mut Device<'_>, output: Format) -> Result<Self> {
        let layout = VertexLayout {
            stride: 32,
            attributes: vec![
                VertexAttribute {
                    location: 0,
                    format: VertexFormat::F32x3,
                    offset: 0,
                },
                VertexAttribute {
                    location: 1,
                    format: VertexFormat::F32x3,
                    offset: 12,
                },
                VertexAttribute {
                    location: 2,
                    format: VertexFormat::F32x2,
                    offset: 24,
                },
            ],
        };
        let reverse_depth = |compare, write| DepthState {
            format: Format::Depth32Float,
            compare,
            write,
            bias_constant: 0.0,
            bias_slope: 0.0,
            clamp: false,
        };
        let mrt = || vec![color(HDR, Blend::Replace), color(HDR, Blend::Replace)];
        let vs = compile(SCENE, Stage::Vertex, "vs_main")?;
        let fs = compile(SCENE, Stage::Fragment, "fs_main")?;
        let scene = device.create_render_pipeline(&RenderPipelineDesc {
            vertex: &vs,
            fragment: Some(&fs),
            vertex_layout: Some(layout.clone()),
            topology: Topology::TriangleList,
            cull: Cull::None,
            front_ccw: true,
            depth: Some(reverse_depth(Compare::Greater, true)),
            colors: mrt(),
        })?;
        let vs = compile(SCENE, Stage::Vertex, "vs_prop")?;
        let scene_prop = device.create_render_pipeline(&RenderPipelineDesc {
            vertex: &vs,
            fragment: Some(&fs),
            vertex_layout: Some(layout.clone()),
            topology: Topology::TriangleList,
            cull: Cull::None,
            front_ccw: true,
            depth: Some(reverse_depth(Compare::Greater, true)),
            colors: mrt(),
        })?;
        // Blended props leave the aux target (normals for outlines and occlusion) alone.
        let mut scene_prop_blend = [scene_prop; 2];
        for (i, blend) in [Blend::Premultiplied, Blend::Additive]
            .into_iter()
            .enumerate()
        {
            scene_prop_blend[i] = device.create_render_pipeline(&RenderPipelineDesc {
                vertex: &vs,
                fragment: Some(&fs),
                vertex_layout: Some(layout.clone()),
                topology: Topology::TriangleList,
                cull: Cull::None,
                front_ccw: true,
                depth: Some(reverse_depth(Compare::Greater, false)),
                colors: vec![
                    color(HDR, blend),
                    ColorTarget {
                        format: HDR,
                        blend: Blend::Replace,
                        write_mask: 0,
                    },
                ],
            })?;
        }
        let overlay_fs = compile(SCENE, Stage::Fragment, "fs_overlay")?;
        let overlay_bindings = overlay_fs.bindings.clone();
        let mut overlay = [[scene_prop; 2]; 3];
        for (i, blend) in [Blend::Replace, Blend::Premultiplied, Blend::Additive]
            .into_iter()
            .enumerate()
        {
            for (j, compare) in [Compare::Always, Compare::Greater].into_iter().enumerate() {
                overlay[i][j] = device.create_render_pipeline(&RenderPipelineDesc {
                    vertex: &vs,
                    fragment: Some(&overlay_fs),
                    vertex_layout: Some(layout.clone()),
                    topology: Topology::TriangleList,
                    cull: Cull::None,
                    front_ccw: true,
                    depth: Some(reverse_depth(compare, false)),
                    colors: vec![color(output, blend)],
                })?;
            }
        }
        let shadow_depth = DepthState {
            format: Format::Depth32Float,
            compare: Compare::Less,
            write: true,
            bias_constant: 1.0,
            bias_slope: 1.75,
            clamp: true,
        };
        let vs = compile(WORLD_FX, Stage::Vertex, "vs_shadow_prop")?;
        let shadow_prop = device.create_render_pipeline(&RenderPipelineDesc {
            vertex: &vs,
            fragment: None,
            vertex_layout: Some(VertexLayout {
                stride: 32,
                attributes: vec![layout.attributes[0]],
            }),
            topology: Topology::TriangleList,
            cull: Cull::None,
            front_ccw: true,
            depth: Some(shadow_depth),
            colors: vec![],
        })?;
        let vs = compile(WORLD_FX, Stage::Vertex, "vs_shadow_prop_masked")?;
        let fs = compile(WORLD_FX, Stage::Fragment, "fs_shadow_prop_masked")?;
        let shadow_prop_masked = device.create_render_pipeline(&RenderPipelineDesc {
            vertex: &vs,
            fragment: Some(&fs),
            vertex_layout: Some(VertexLayout {
                stride: 32,
                attributes: vec![layout.attributes[0], layout.attributes[2]],
            }),
            topology: Topology::TriangleList,
            cull: Cull::None,
            front_ccw: true,
            depth: Some(shadow_depth),
            colors: vec![],
        })?;
        let vs = compile(WORLD_FX, Stage::Vertex, "vs_shadow")?;
        let shadow = device.create_render_pipeline(&RenderPipelineDesc {
            vertex: &vs,
            fragment: None,
            vertex_layout: Some(VertexLayout {
                stride: 32,
                attributes: vec![layout.attributes[0]],
            }),
            topology: Topology::TriangleList,
            cull: Cull::None,
            front_ccw: true,
            depth: Some(DepthState {
                format: Format::Depth32Float,
                compare: Compare::Less,
                write: true,
                bias_constant: 1.0,
                bias_slope: 1.75,
                clamp: true,
            }),
            colors: vec![],
        })?;
        let vs = compile(WORLD_FX, Stage::Vertex, "vs_shadow_masked")?;
        let fs = compile(WORLD_FX, Stage::Fragment, "fs_shadow_masked")?;
        let shadow_masked = device.create_render_pipeline(&RenderPipelineDesc {
            vertex: &vs,
            fragment: Some(&fs),
            vertex_layout: Some(VertexLayout {
                stride: 32,
                attributes: vec![layout.attributes[0], layout.attributes[2]],
            }),
            topology: Topology::TriangleList,
            cull: Cull::None,
            front_ccw: true,
            depth: Some(DepthState {
                format: Format::Depth32Float,
                compare: Compare::Less,
                write: true,
                bias_constant: 1.0,
                bias_slope: 1.75,
                clamp: true,
            }),
            colors: vec![],
        })?;
        let vs = compile(WORLD_FX, Stage::Vertex, "vs_ground")?;
        let fs = compile(WORLD_FX, Stage::Fragment, "fs_ground")?;
        let ground = device.create_render_pipeline(&RenderPipelineDesc {
            vertex: &vs,
            fragment: Some(&fs),
            vertex_layout: Some(VertexLayout {
                stride: 32,
                attributes: vec![layout.attributes[0], layout.attributes[1]],
            }),
            topology: Topology::TriangleList,
            cull: Cull::None,
            front_ccw: true,
            depth: Some(DepthState {
                format: Format::Depth32Float,
                compare: Compare::Less,
                write: true,
                bias_constant: 0.0,
                bias_slope: 0.0,
                clamp: true,
            }),
            colors: vec![color(GROUND_FORMAT, Blend::Replace)],
        })?;
        let blades = |device: &mut Device<'_>, vs: &str, fs: &str| -> Result<PipelineId> {
            let vs = compile(SCENE, Stage::Vertex, vs)?;
            let fs = compile(SCENE, Stage::Fragment, fs)?;
            device.create_render_pipeline(&RenderPipelineDesc {
                vertex: &vs,
                fragment: Some(&fs),
                vertex_layout: None,
                topology: Topology::TriangleList,
                cull: Cull::None,
                front_ccw: true,
                depth: Some(reverse_depth(Compare::Greater, true)),
                colors: mrt(),
            })
        };
        let grass = blades(device, "vs_grass", "fs_grass")?;
        let flower = blades(device, "vs_flower", "fs_flower")?;
        let vs = compile(WORLD_FX, Stage::Vertex, "vs_sky")?;
        let fs = compile(WORLD_FX, Stage::Fragment, "fs_sky")?;
        let sky = device.create_render_pipeline(&RenderPipelineDesc {
            vertex: &vs,
            fragment: Some(&fs),
            vertex_layout: None,
            topology: Topology::TriangleList,
            cull: Cull::None,
            front_ccw: true,
            depth: Some(reverse_depth(Compare::GreaterEqual, false)),
            colors: mrt(),
        })?;
        let vs = compile(WORLD_FX, Stage::Vertex, "vs_particle")?;
        let fs = compile(WORLD_FX, Stage::Fragment, "fs_particle")?;
        let particle = device.create_render_pipeline(&RenderPipelineDesc {
            vertex: &vs,
            fragment: Some(&fs),
            vertex_layout: None,
            topology: Topology::TriangleList,
            cull: Cull::None,
            front_ccw: true,
            depth: Some(reverse_depth(Compare::Greater, false)),
            colors: vec![
                color(HDR, Blend::Alpha),
                ColorTarget {
                    format: HDR,
                    blend: Blend::Replace,
                    write_mask: 0,
                },
            ],
        })?;
        let fsvs = compile(POST, Stage::Vertex, "vs_fullscreen")?;
        Ok(Self {
            scene,
            scene_prop,
            scene_prop_blend,
            overlay,
            output_format: output,
            prop_layout: layout,
            overlay_bindings,
            shadow,
            shadow_masked,
            shadow_prop,
            shadow_prop_masked,
            ground,
            grass,
            flower,
            sky,
            particle,
            ssao: fullscreen(device, &fsvs, "fs_ssao", Format::R16Float, Blend::Replace)?,
            air: fullscreen(device, &fsvs, "fs_air", HDR, Blend::Replace)?,
            bloom_down: fullscreen(device, &fsvs, "fs_bloom_down", HDR, Blend::Replace)?,
            bloom_up: fullscreen(device, &fsvs, "fs_bloom_up", HDR, Blend::Additive)?,
            tonemap: fullscreen(device, &fsvs, "fs_tonemap", HDR, Blend::Replace)?,
            paint: fullscreen(device, &fsvs, "fs_paint", HDR, Blend::Replace)?,
            output: fullscreen(device, &fsvs, "fs_final", output, Blend::Replace)?,
        })
    }
}

/// Builds the `Frame` uniform block in WGSL layout.
struct Block(Vec<f32>);
impl Block {
    fn m(&mut self, m: &M4) {
        self.0.extend_from_slice(m);
    }
    fn v(&mut self, v: V3, w: f32) {
        self.0.extend_from_slice(&[v[0], v[1], v[2], w]);
    }
    fn f(&mut self, a: f32, b: f32, c: f32, d: f32) {
        self.0.extend_from_slice(&[a, b, c, d]);
    }
}

/// The top-down ground map for one frame.
struct Ground {
    vp: M4,
    top: f32,
    range: f32,
    size: u32,
}

/// A camera for one frame: view-projection, its inverse and the eye.
#[derive(Clone, Copy)]
pub(super) struct Eye {
    pub(super) view_proj: M4,
    pub(super) inv: M4,
    pos: V3,
    near: f32,
}

struct Cascades {
    mats: [M4; 2],
    split: f32,
    offset: [f32; 2],
    radius: [f32; 2],
    bias: [f32; 2],
}

fn halton(i: u32, base: u32) -> f32 {
    let (mut f, mut r, mut i) = (1.0, 0.0, i);
    while i > 0 {
        f /= base as f32;
        r += f * (i % base) as f32;
        i /= base;
    }
    r
}

fn f16_to_f32(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = (h >> 10) & 0x1f;
    let frac = f32::from(h & 0x3ff);
    sign * match exp {
        0 => frac * 2f32.powi(-24),
        31 => f32::INFINITY,
        e => (1.0 + frac / 1024.0) * 2f32.powi(i32::from(e) - 15),
    }
}

/// The world map's top-down projection: the land's XZ bounds plus a margin, no further than
/// `WORLD_MAX_REACH` from the land's centre (square, so texels are square), depth 0 above the
/// highest point of the world and 1 below the lowest.
fn world_view(scene: &GpuScene, size: u32) -> Ground {
    let c = scene.land_centre;
    let span = |k: usize| {
        (
            (scene.land_min[k] - WORLD_MARGIN).max(c[k] - WORLD_MAX_REACH),
            (scene.land_max[k] + WORLD_MARGIN).min(c[k] + WORLD_MAX_REACH),
        )
    };
    let ((x0, x1), (z0, z1)) = (span(0), span(2));
    let (x, z) = (0.5 * (x0 + x1), 0.5 * (z0 + z1));
    let r = 0.5 * (x1 - x0).max(z1 - z0);
    let top = scene.max[1] + 1.0;
    let range = (top - scene.min[1] + 1.0).max(2.0);
    let v = look_at([x, top, z], [x, top - 1.0, z], [0.0, 0.0, -1.0]);
    Ground {
        vp: mul(&ortho(-r, r, -r, r, 0.0, range), &v),
        top,
        range,
        size,
    }
}

/// The detail block of a material named `name` (its first matching Look rule) and whether grass
/// grows on it.
pub(super) fn detail_block(look: &Look, name: &str) -> ([f32; 16], bool) {
    let rule = (!name.is_empty())
        .then(|| look.details.iter().find(|r| r.matches(name)))
        .flatten();
    let mut d = [0.0f32; 16];
    if let Some(r) = rule {
        let c = r.color.unwrap_or([0.0; 3]);
        // A lattice may leave out one direction of bars with a zero size.
        let floor = if r.preset == DetailPreset::Lattice {
            0.0
        } else {
            1e-3
        };
        d[..4].copy_from_slice(&[
            r.preset as u32 as f32,
            r.scale[0].max(floor),
            r.scale[1].max(floor),
            r.strength,
        ]);
        d[4..8].copy_from_slice(&[r.variation, c[0], c[1], c[2]]);
        d[8] = if r.color.is_some() { 1.0 } else { 0.0 };
        d[9] = r.gloss;
    }
    let grassy =
        rule.is_some_and(|r| matches!(r.preset, DetailPreset::Grass | DetailPreset::Terrain));
    (d, grassy)
}

/// The detail block of each world material and whether grass grows on it.
fn material_details(look: &Look, scene: &GpuScene) -> (Vec<[f32; 16]>, Vec<bool>) {
    scene.names.iter().map(|n| detail_block(look, n)).unzip()
}

impl Renderer {
    pub fn new(
        device: &mut Device<'_>,
        world: &crate::world::World,
        look: Look,
        quality: Quality,
        size: (u32, u32),
        output: Output,
    ) -> Result<Self> {
        let out_format = match output {
            Output::Still => HDR,
            Output::Surface(f) => f,
        };
        let pipelines = Pipelines::new(device, out_format)?;
        let scene = GpuScene::upload(device, world)?;
        let sampled = TextureUsage::SAMPLED | TextureUsage::COPY_DST;
        let solid = |device: &mut Device<'_>, format, texel: [u8; 4]| -> Result<TextureId> {
            let t = device.create_texture(&TextureDesc::new(1, 1, format, sampled))?;
            device.write_texture(t, 0, 0, &texel)?;
            Ok(t)
        };
        let white = solid(device, Format::Rgba8Srgb, [255; 4])?;
        let flat = solid(device, Format::Rgba8Unorm, [128, 128, 255, 255])?;
        let dummy = solid(device, Format::Rgba8Unorm, [0; 4])?;
        let material_sampler = device.create_sampler(&SamplerDesc {
            anisotropy: 8.0,
            ..SamplerDesc::default()
        })?;
        let linear_clamp = device.create_sampler(&SamplerDesc {
            address: Address::Clamp,
            ..SamplerDesc::default()
        })?;
        let shadow_sampler = device.create_sampler(&SamplerDesc {
            address: Address::Clamp,
            compare: Some(Compare::LessEqual),
            mip_filter: Filter::Nearest,
            ..SamplerDesc::default()
        })?;
        let mut materials = vec![];
        let mut shadow_materials = vec![];
        for m in &scene.materials {
            shadow_materials.push(if m.params[0] >= 0.0 {
                Some(device.create_bind_group(
                    pipelines.shadow_masked,
                    1,
                    &[
                        (
                            0,
                            Resource::Texture(m.base.map_or(white, |i| scene.textures[i])),
                        ),
                        (3, Resource::Sampler(material_sampler)),
                    ],
                )?)
            } else {
                None
            });
            let tex = |i: Option<usize>, fallback| i.map_or(fallback, |i| scene.textures[i]);
            materials.push(device.create_bind_group(
                pipelines.scene,
                1,
                &[
                    (0, Resource::Texture(tex(m.base, white))),
                    (1, Resource::Texture(tex(m.normal, flat))),
                    (2, Resource::Texture(tex(m.emission, white))),
                    (3, Resource::Sampler(material_sampler)),
                ],
            )?);
        }
        let shadow_map = device.create_texture(&TextureDesc {
            width: quality.shadow_size,
            height: quality.shadow_size,
            layers: 2,
            mips: 1,
            format: Format::Depth32Float,
            usage: TextureUsage::DEPTH_TARGET | TextureUsage::SAMPLED,
        })?;
        let ground_size = quality.ground_size.max(64);
        let ground_depth = device.create_texture(&TextureDesc {
            width: ground_size,
            height: ground_size,
            layers: 1,
            mips: 1,
            format: Format::Depth32Float,
            usage: TextureUsage::DEPTH_TARGET | TextureUsage::SAMPLED,
        })?;
        let ground_mask = device.create_texture(&TextureDesc {
            width: ground_size,
            height: ground_size,
            layers: 1,
            mips: 1,
            format: GROUND_FORMAT,
            usage: TextureUsage::COLOR_TARGET | TextureUsage::SAMPLED,
        })?;
        let world_size = quality.world_size.max(64);
        let world_depth = device.create_texture(&TextureDesc {
            width: world_size,
            height: world_size,
            layers: 1,
            mips: 1,
            format: Format::Depth32Float,
            usage: TextureUsage::DEPTH_TARGET | TextureUsage::SAMPLED,
        })?;
        let world_mask = device.create_texture(&TextureDesc {
            width: world_size,
            height: world_size,
            layers: 1,
            mips: 1,
            format: GROUND_FORMAT,
            usage: TextureUsage::COLOR_TARGET | TextureUsage::SAMPLED,
        })?;
        let world = world_view(&scene, world_size);
        let (details, grassy) = material_details(&look, &scene);
        let targets = Self::create_targets(device, &quality, output, size)?;
        let mut r = Self {
            scene,
            pipelines,
            quality,
            output,
            look,
            targets,
            slots: vec![],
            materials,
            shadow_materials,
            shadow_map,
            ground_depth,
            ground_mask,
            world_depth,
            world_mask,
            world,
            world_drawn: false,
            details,
            grassy,
            dummy,
            linear_clamp,
            shadow_sampler,
            white,
            flat,
            material_sampler,
            props: super::props::Props::default(),
            models: super::models::Models::default(),
        };
        let slots = match output {
            Output::Still => 1,
            Output::Surface(_) => 2,
        };
        for _ in 0..slots {
            let s = r.create_slot(device)?;
            r.slots.push(s);
        }
        Ok(r)
    }

    pub fn look(&self) -> &Look {
        &self.look
    }
    pub fn set_look(&mut self, look: Look) {
        (self.details, self.grassy) = material_details(&look, &self.scene);
        self.look = look;
        self.world_drawn = false;
        self.restyle_props();
    }
    pub fn size(&self) -> (u32, u32) {
        (self.targets.width, self.targets.height)
    }
    pub fn triangles(&self) -> usize {
        self.scene.triangles
    }
    pub fn lights(&self) -> usize {
        self.scene.lights.len()
    }
    pub fn water_level(&self) -> Option<f32> {
        self.scene.water_level
    }
    /// World bounds of everything drawn.
    pub fn bounds(&self) -> (V3, V3) {
        (self.scene.min, self.scene.max)
    }

    fn create_targets(
        device: &mut Device<'_>,
        q: &Quality,
        output: Output,
        (width, height): (u32, u32),
    ) -> Result<Targets> {
        let rt = TextureUsage::COLOR_TARGET | TextureUsage::SAMPLED;
        let tex = |d: &mut Device<'_>, w: u32, h: u32, f: Format, u| {
            d.create_texture(&TextureDesc::new(w.max(1), h.max(1), f, u))
        };
        let scaled = |s: f32| {
            (
                ((width as f32 * s).round() as u32).max(1),
                ((height as f32 * s).round() as u32).max(1),
            )
        };
        let refl = scaled(q.reflection_scale);
        let ao = scaled(q.ao_scale);
        let bloom_size = ((width / 2).max(1), (height / 2).max(1));
        let max_levels = 32 - bloom_size.0.min(bloom_size.1).leading_zeros();
        let bloom_levels = q.bloom_levels.clamp(1, max_levels.max(1));
        let depth_usage = TextureUsage::DEPTH_TARGET | TextureUsage::SAMPLED;
        let mut t = Targets {
            width,
            height,
            hdr: tex(device, width, height, HDR, rt)?,
            aux: tex(device, width, height, HDR, rt)?,
            depth: tex(device, width, height, Format::Depth32Float, depth_usage)?,
            refl,
            refl_hdr: tex(device, refl.0, refl.1, HDR, rt)?,
            refl_aux: tex(device, refl.0, refl.1, HDR, rt)?,
            refl_depth: tex(device, refl.0, refl.1, Format::Depth32Float, depth_usage)?,
            ao,
            ao_tex: tex(device, ao.0, ao.1, Format::R16Float, rt)?,
            lit: tex(device, width, height, HDR, rt)?,
            bloom: device.create_texture(&TextureDesc {
                width: bloom_size.0,
                height: bloom_size.1,
                layers: 1,
                mips: bloom_levels,
                format: HDR,
                usage: rt,
            })?,
            bloom_size,
            bloom_levels,
            ldr: tex(device, width, height, HDR, rt)?,
            paint: tex(device, width, height, HDR, rt)?,
            still: None,
            readback: None,
            tiles: [
                (width.div_ceil(q.tile_size), height.div_ceil(q.tile_size)),
                (refl.0.div_ceil(q.tile_size), refl.1.div_ceil(q.tile_size)),
            ],
        };
        if output == Output::Still {
            t.still = Some(tex(
                device,
                width,
                height,
                HDR,
                rt | TextureUsage::COPY_SRC,
            )?);
            t.readback = Some(device.create_buffer(&BufferDesc {
                size: u64::from(width) * u64::from(height) * 8,
                usage: BufferUsage::COPY_DST,
                memory: Memory::Readback,
            })?);
        }
        Ok(t)
    }

    fn create_slot(&self, device: &mut Device<'_>) -> Result<Slot> {
        let t = &self.targets;
        let p = &self.pipelines;
        let upload = |d: &mut Device<'_>, size: u64, usage| {
            d.create_buffer(&BufferDesc {
                size: size.max(16),
                usage,
                memory: Memory::Upload,
            })
        };
        let mut frame = vec![];
        let mut tiles = vec![];
        let mut index = vec![];
        for v in 0..2 {
            let (tx, ty) = t.tiles[v];
            let n = u64::from(tx) * u64::from(ty);
            frame.push(upload(device, FRAME_BYTES, BufferUsage::UNIFORM)?);
            tiles.push(upload(device, n * 8, BufferUsage::STORAGE)?);
            index.push(upload(
                device,
                n * MAX_LIGHTS_PER_TILE as u64 * 4,
                BufferUsage::STORAGE,
            )?);
        }
        let buf = |id| Resource::Buffer {
            id,
            offset: 0,
            size: 0,
        };
        let mut scene = vec![];
        let mut sky = vec![];
        let mut blades = vec![];
        for v in 0..2 {
            let reflection = if v == 0 { t.refl_hdr } else { self.dummy };
            let entries = [
                (0, buf(frame[v])),
                (1, buf(self.scene.light_buffer)),
                (2, buf(tiles[v])),
                (3, buf(index[v])),
                (4, Resource::Texture(self.shadow_map)),
                (5, Resource::Sampler(self.shadow_sampler)),
                (6, Resource::Texture(reflection)),
                (7, Resource::Sampler(self.linear_clamp)),
                (8, Resource::Texture(self.ground_depth)),
                (9, Resource::Texture(self.ground_mask)),
                (10, Resource::Texture(self.world_depth)),
            ];
            scene.push(device.create_bind_group(p.scene, 0, &entries)?);
            sky.push(device.create_bind_group(p.sky, 0, &entries[..1])?);
            if v == 0 {
                blades.push(device.create_bind_group(p.grass, 0, &entries)?);
                blades.push(device.create_bind_group(p.flower, 0, &entries)?);
                blades.push(device.create_bind_group(p.particle, 0, &entries)?);
            }
        }
        fn pair<T: Copy>(v: Vec<T>) -> [T; 2] {
            [v[0], v[1]]
        }
        let (frame, tiles, index) = (pair(frame), pair(tiles), pair(index));
        let (scene, sky) = (pair(scene), pair(sky));
        let (grass, flower, particle) = (blades[0], blades[1], blades[2]);
        let post = |d: &mut Device<'_>, pipe, src: Resource, extra: Resource| {
            d.create_bind_group(
                pipe,
                0,
                &[
                    (0, buf(frame[0])),
                    (1, Resource::Texture(t.depth)),
                    (2, Resource::Texture(t.aux)),
                    (3, src),
                    (4, extra),
                    (5, Resource::Sampler(self.linear_clamp)),
                ],
            )
        };
        let none = Resource::Texture(self.dummy);
        let ssao = post(device, p.ssao, none, none)?;
        let air = post(
            device,
            p.air,
            Resource::Texture(t.hdr),
            Resource::Texture(t.ao_tex),
        )?;
        let mut down = vec![];
        let mut up = vec![];
        for level in 0..t.bloom_levels {
            let src = if level == 0 {
                Resource::Texture(t.lit)
            } else {
                Resource::TextureMip(t.bloom, level - 1)
            };
            down.push(post(device, p.bloom_down, src, none)?);
            if level + 1 < t.bloom_levels {
                up.push(post(
                    device,
                    p.bloom_up,
                    Resource::TextureMip(t.bloom, level + 1),
                    none,
                )?);
            }
        }
        let tonemap = post(
            device,
            p.tonemap,
            Resource::Texture(t.lit),
            Resource::TextureMip(t.bloom, 0),
        )?;
        let paint = post(device, p.paint, Resource::Texture(t.ldr), none)?;
        let final_src = if self.quality.painterly {
            t.paint
        } else {
            t.ldr
        };
        let output = post(device, p.output, Resource::Texture(final_src), none)?;
        Ok(Slot {
            frame,
            tiles,
            index,
            scene,
            sky,
            grass,
            flower,
            particle,
            ssao,
            air,
            down,
            up,
            tonemap,
            paint,
            output,
        })
    }

    /// Recreate size-dependent targets (e.g. after a window resize).
    pub fn resize(&mut self, device: &mut Device<'_>, size: (u32, u32)) -> Result<()> {
        if size == (self.targets.width, self.targets.height) {
            return Ok(());
        }
        self.reset_prop_slots(device);
        let old = std::mem::replace(
            &mut self.targets,
            Self::create_targets(device, &self.quality, self.output, size)?,
        );
        for s in std::mem::take(&mut self.slots) {
            for g in s
                .scene
                .into_iter()
                .chain(s.sky)
                .chain([s.grass, s.flower, s.particle, s.ssao, s.air])
                .chain([s.tonemap, s.paint, s.output])
            {
                device.destroy_bind_group(g);
            }
            for g in s.down.into_iter().chain(s.up) {
                device.destroy_bind_group(g);
            }
            for b in s.frame.into_iter().chain(s.tiles).chain(s.index) {
                device.destroy_buffer(b);
            }
        }
        for t in [
            old.hdr,
            old.aux,
            old.depth,
            old.refl_hdr,
            old.refl_aux,
            old.refl_depth,
            old.ao_tex,
            old.lit,
            old.bloom,
            old.ldr,
            old.paint,
        ]
        .into_iter()
        .chain(old.still)
        {
            device.destroy_texture(t);
        }
        if let Some(b) = old.readback {
            device.destroy_buffer(b);
        }
        let slots = match self.output {
            Output::Still => 1,
            Output::Surface(_) => 2,
        };
        for _ in 0..slots {
            let s = self.create_slot(device)?;
            self.slots.push(s);
        }
        Ok(())
    }

    pub(super) fn eye(&self, view: &View, jitter: [f32; 2], (w, h): (u32, u32)) -> Eye {
        let v = look_at(view.eye, view.target, camera_up(view));
        let mut p = perspective(view.fov.to_radians(), w as f32 / h as f32, near(view));
        // Column 2 skews the frustum: x_ndc gains -p[8] and y_ndc (pointing down) -p[9].
        p[8] += 2.0 * jitter[0] / w as f32 - view.shift[0];
        p[9] += 2.0 * jitter[1] / h as f32 + view.shift[1];
        let view_proj = mul(&p, &v);
        Eye {
            view_proj,
            inv: invert(&view_proj).unwrap_or(math::IDENTITY),
            pos: view.eye,
            near: near(view),
        }
    }

    /// The ground map's top-down orthographic projection: centred on the view target (at most
    /// 40 m ahead of the eye, so the camera is always inside the map), depth 0 above the
    /// highest point of the world and 1 below the lowest. The centre snaps to whole texels so
    /// grass, rain cover and foam do not shimmer as the camera moves.
    fn ground_view(&self, view: &View) -> Ground {
        let to = sub(view.target, view.eye);
        let d = math::length(to);
        let centre = if d <= 40.0 {
            view.target
        } else {
            add(view.eye, scale(to, 40.0 / d))
        };
        let size = self.quality.ground_size.max(64);
        let texel = 2.0 * GROUND_REACH / size as f32;
        let (x, z) = (
            (centre[0] / texel).round() * texel,
            (centre[2] / texel).round() * texel,
        );
        let top = self.scene.max[1] + 1.0;
        let range = (top - self.scene.min[1] + 1.0).max(2.0);
        let v = look_at([x, top, z], [x, top - 1.0, z], [0.0, 0.0, -1.0]);
        let r = GROUND_REACH;
        Ground {
            vp: mul(&ortho(-r, r, -r, r, 0.0, range), &v),
            top,
            range,
            size,
        }
    }

    fn cascades(&self, view: &View, env: &Environment) -> Cascades {
        let (w, h) = (self.targets.width as f32, self.targets.height as f32);
        let focus = math::length(sub(view.target, view.eye)).max(1.0);
        let split = (focus * 1.5).clamp(18.0, 140.0);
        let far = (focus * 4.5).clamp(90.0, 420.0);
        let l = normalize(env.key_dir);
        let size = self.quality.shadow_size as f32;
        let fwd = normalize(sub(view.target, view.eye));
        let right = normalize(cross(fwd, camera_up(view)));
        let up = cross(right, fwd);
        let ty = (0.5 * view.fov.to_radians()).tan();
        let tx = ty * w / h;
        let [shift_x, shift_y] = view.shift;
        let mut out = Cascades {
            mats: [math::IDENTITY; 2],
            split,
            offset: [0.0; 2],
            radius: [0.0; 2],
            bias: [0.0; 2],
        };
        // Water casts no shadow, so a wide sea does not stretch the depth range.
        let (mut smin, mut smax) = (self.scene.land_min, self.scene.land_max);
        if let Some((lo, hi)) = self.props_bounds() {
            for k in 0..3 {
                smin[k] = smin[k].min(lo[k]);
                smax[k] = smax[k].max(hi[k]);
            }
        }
        for (c, (d0, d1)) in [(near(view), split), (split * 0.85, far)]
            .into_iter()
            .enumerate()
        {
            let mut pts = vec![];
            for d in [d0, d1] {
                for sx in [-1.0, 1.0] {
                    for sy in [-1.0, 1.0] {
                        pts.push(add(
                            add(view.eye, scale(fwd, d)),
                            add(
                                scale(right, (sx - shift_x) * d * tx),
                                scale(up, (sy - shift_y) * d * ty),
                            ),
                        ));
                    }
                }
            }
            let centre = scale(pts.iter().fold([0.0; 3], |a, p| add(a, *p)), 1.0 / 8.0);
            let radius = pts
                .iter()
                .map(|p| math::length(sub(*p, centre)))
                .fold(0.0f32, f32::max)
                .max(1.0);
            let up_hint = if l[1].abs() > 0.98 {
                [0.0, 0.0, 1.0]
            } else {
                [0.0, 1.0, 0.0]
            };
            let back = 1000.0;
            // The light's view is anchored at the world origin, so it only changes when the
            // light moves. The cascade centre is then snapped to whole texels in that frame,
            // and the radius to a fixed step, so shadows don't crawl as the camera moves.
            let lv = look_at(scale(l, back), [0.0; 3], up_hint);
            let radius = (radius * 2.0).ceil() * 0.5;
            let texel = 2.0 * radius / size;
            let lc = transform(&lv, [centre[0], centre[1], centre[2], 1.0]);
            let snap = |x: f32| (x / texel).round() * texel - x;
            let (dx, dy) = (snap(lc[0]), snap(lc[1]));
            // Depth range from the scene bounds along the light.
            let (mut zmin, mut zmax) = (f32::INFINITY, f32::NEG_INFINITY);
            for i in 0..8 {
                let p = [
                    if i & 1 == 0 { smin[0] } else { smax[0] },
                    if i & 2 == 0 { smin[1] } else { smax[1] },
                    if i & 4 == 0 { smin[2] } else { smax[2] },
                ];
                let z = -transform(&lv, [p[0], p[1], p[2], 1.0])[2];
                zmin = zmin.min(z);
                zmax = zmax.max(z);
            }
            let near = (zmin - 5.0).max(1.0);
            let farz = (zmax + 5.0).max(near + 1.0);
            let proj = ortho(
                lc[0] + dx - radius,
                lc[0] + dx + radius,
                lc[1] + dy - radius,
                lc[1] + dy + radius,
                near,
                farz,
            );
            out.mats[c] = mul(&proj, &lv);
            out.offset[c] = texel * 1.5;
            let filter_texels = if c == 0 { 1.6 } else { 1.1 };
            out.radius[c] = filter_texels * env.shadow_softness / size;
            out.bias[c] = (texel * 0.6) / (farz - near);
        }
        out
    }

    /// Light lists per screen tile for one camera. Lights are sorted nearest first so a full
    /// tile keeps the lights that matter most.
    fn tile_lights(
        &self,
        eye: &Eye,
        (w, h): (u32, u32),
        (tx, ty): (u32, u32),
        on: bool,
    ) -> (Vec<u32>, Vec<u32>) {
        let ts = self.quality.tile_size as f32;
        let mut lists: Vec<Vec<u32>> = vec![vec![]; (tx * ty) as usize];
        if on {
            let mut order: Vec<usize> = (0..self.scene.lights.len()).collect();
            let dist = |i: usize| {
                let l = &self.scene.lights[i];
                math::length(sub([l[0], l[1], l[2]], eye.pos))
            };
            order.sort_by(|a, b| dist(*a).total_cmp(&dist(*b)));
            for i in order {
                let l = &self.scene.lights[i];
                let r = l[3];
                let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                let mut behind = 0;
                let mut straddle = false;
                for k in 0..8 {
                    let p = [
                        l[0] + if k & 1 == 0 { -r } else { r },
                        l[1] + if k & 2 == 0 { -r } else { r },
                        l[2] + if k & 4 == 0 { -r } else { r },
                    ];
                    let c = transform(&eye.view_proj, [p[0], p[1], p[2], 1.0]);
                    if c[3] <= eye.near * 0.5 {
                        behind += 1;
                        straddle = true;
                        continue;
                    }
                    let (sx, sy) = (c[0] / c[3], c[1] / c[3]);
                    x0 = x0.min(sx);
                    x1 = x1.max(sx);
                    y0 = y0.min(sy);
                    y1 = y1.max(sy);
                }
                if behind == 8 {
                    continue;
                }
                if straddle {
                    (x0, y0, x1, y1) = (-1.0, -1.0, 1.0, 1.0);
                }
                if x1 < -1.0 || y1 < -1.0 || x0 > 1.0 || y0 > 1.0 {
                    continue;
                }
                let px = |v: f32, size: u32| ((v * 0.5 + 0.5) * size as f32 / ts).floor();
                let ax = px(x0.max(-1.0), w).max(0.0) as u32;
                let bx = (px(x1.min(1.0), w) as u32).min(tx - 1);
                let ay = px(y0.max(-1.0), h).max(0.0) as u32;
                let by = (px(y1.min(1.0), h) as u32).min(ty - 1);
                for y in ay..=by {
                    for x in ax..=bx {
                        let list = &mut lists[(y * tx + x) as usize];
                        if list.len() < MAX_LIGHTS_PER_TILE {
                            list.push(i as u32);
                        }
                    }
                }
            }
        }
        let mut tiles = Vec::with_capacity(lists.len() * 2);
        let mut index = vec![];
        for list in lists {
            tiles.push(index.len() as u32);
            tiles.push(list.len() as u32);
            index.extend(list);
        }
        if index.is_empty() {
            index.push(0);
        }
        (tiles, index)
    }

    #[allow(clippy::too_many_arguments)]
    fn frame_block(
        &self,
        eye: &Eye,
        env: &Environment,
        cas: &Cascades,
        ground: &Ground,
        (w, h): (u32, u32),
        tiles: (u32, u32),
        reflection: bool,
        seed: u32,
    ) -> Vec<u8> {
        let l = &self.look;
        let mut b = Block(Vec::with_capacity(240));
        b.m(&eye.view_proj);
        b.m(&eye.inv);
        b.m(&cas.mats[0]);
        b.m(&cas.mats[1]);
        b.m(&ground.vp);
        b.v(eye.pos, env.seconds);
        b.v(normalize(env.key_dir), env.key_intensity);
        b.v(env.key_color, 0.0);
        b.v(scale(env.ambient_sky, env.ambient), env.lamp_level);
        // Ground ambient plus the key light bounced up off sunlit ground.
        let bounce = scale(env.key_color, env.key_intensity * l.bounce);
        b.v(
            add(scale(env.ambient_ground, env.ambient), bounce),
            env.wetness,
        );
        b.v(l.shadow_tint, l.shadow_strength);
        b.f(
            l.ramp_center,
            l.ramp_softness.max(0.002),
            l.rim_strength,
            l.rim_power,
        );
        b.f(
            l.specular,
            l.specular_size,
            l.foliage_wrap,
            l.foliage_translucency,
        );
        b.f(
            l.texture_softness,
            l.normal_map_strength,
            l.lamp_light,
            l.lamp_glow,
        );
        b.f(
            self.scene.water_level.unwrap_or(-1e6),
            l.water_reflection,
            l.water_ripples,
            env.rain,
        );
        b.v(l.water_color, if reflection { 1.0 } else { 0.0 });
        b.v(env.zenith, env.stars);
        b.v(env.horizon, env.clouds);
        b.v(env.glow, env.sun_disc);
        b.v(normalize(env.glow_dir), env.moon_disc);
        b.v(normalize(env.sun_dir), l.cloud_scale);
        b.v(normalize(env.moon_dir), l.moon_size);
        b.v(env.cloud_lit, l.star_density);
        b.v(env.cloud_shadow, env.petals);
        b.v(env.fog_color, env.fog_density);
        let fog_base = l.fog_height + self.scene.water_level.unwrap_or(0.0);
        b.f(fog_base, l.fog_falloff, env.mist, l.fog);
        b.f(env.wind[0], env.wind[1], 1.0, seed as f32);
        b.f(w as f32, h as f32, 1.0 / w as f32, 1.0 / h as f32);
        b.f(cas.split, cas.offset[0], cas.offset[1], eye.near);
        b.f(cas.radius[0], cas.radius[1], cas.bias[0], cas.bias[1]);
        b.f(
            ground.top,
            ground.range,
            ground.size as f32,
            l.grass.density,
        );
        b.f(l.brush, l.brush_scale.max(0.01), l.sun_shafts, 0.0);
        b.v(l.water_shallow, l.foam);
        b.f(l.sparkle, self.grass_side() as f32, 0.0, 0.0);
        let g = &l.grass;
        b.f(g.height, g.width, g.flowers, g.sway);
        b.v(g.base, 0.0);
        b.v(g.tip, 0.0);
        for c in g.flower_colors {
            b.v(c, 0.0);
        }
        let w = &self.world;
        b.m(&w.vp);
        b.f(
            w.top,
            w.range,
            w.size as f32,
            if self.world_drawn { 1.0 } else { 0.0 },
        );
        let t = &l.terrain;
        b.f(t.beach, t.rock_slope[0], t.rock_slope[1], t.patch_size);
        b.v(t.sand, t.patches);
        b.v(t.wet_sand, 0.0);
        b.v(t.rock, 0.0);
        b.v(t.meadow, 0.0);
        b.f(l.lamp_foliage, 0.0, 0.0, 0.0);
        let tide = if self.scene.water_level.is_some() {
            l.tide
        } else {
            0.0
        };
        let [r, g, bl] = l.tide_color;
        b.f(tide, r, g, bl);
        let mut bytes = f32_bytes(&b.0);
        for u in [
            tiles.0,
            tiles.1,
            self.quality.tile_size,
            self.scene.lights.len() as u32,
        ] {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        bytes
    }

    /// Grass instances to draw: the instance grid scaled by quality and the Look's density,
    /// or 0 when no material grows grass.
    fn grass_instances(&self) -> u32 {
        let side = self.grass_side();
        side * side
    }

    /// Rows of the grass instance grid this frame (0 when nothing grows grass).
    fn grass_side(&self) -> u32 {
        let g = &self.look.grass;
        if g.density <= 0.0 || self.quality.grass <= 0.0 || !self.grassy.iter().any(|&x| x) {
            return 0;
        }
        ((GRASS_SIDE as f32 * self.quality.grass.min(1.0).sqrt()).round() as u32 / 2 * 2).max(2)
    }

    /// A ground (or world) map: the highest solid surfaces seen from above through `vp`, and
    /// which of them grow grass. Roofs and solid things count; foliage, alpha-masked cut-outs
    /// and water do not (rain falls through trees, grass grows under them). On terrain, beach
    /// sand and bare rock grow no grass (see `fs_ground`).
    fn draw_ground(&self, enc: &mut Encoder, depth: TextureId, mask: TextureId, vp: &M4) {
        enc.begin_pass(PassDesc {
            colors: vec![Attachment::clear(mask, [0.0; 4])],
            depth: Some(Attachment::clear(depth, [1.0; 4])),
        });
        enc.set_pipeline(self.pipelines.ground);
        enc.set_vertex_buffer(self.scene.vertices, 0);
        enc.set_index_buffer(self.scene.indices, 0);
        let t = &self.look.terrain;
        let mut gp = [0.0f32; 24];
        gp[..16].copy_from_slice(vp);
        gp[17] = 1.0;
        gp[19] = self.scene.water_level.unwrap_or(-1.0e6);
        gp[20..].copy_from_slice(&[t.beach, t.rock_slope[0], t.rock_slope[1], t.patch_size]);
        let terrain = DetailPreset::Terrain as u32 as f32;
        for d in self.scene.draws.iter() {
            let foliage = self.scene.materials[d.material].params[3] == 1.0;
            if d.water || foliage || self.shadow_materials[d.material].is_some() {
                continue;
            }
            gp[16] = if self.grassy[d.material] { 1.0 } else { 0.0 };
            gp[18] = if self.details[d.material][0] == terrain {
                1.0
            } else {
                0.0
            };
            enc.push_constants(&f32_bytes(&gp));
            enc.draw_indexed(d.count, 1, d.first_index, 0, 0);
        }
        enc.end_pass();
    }

    fn draw_world(&self, enc: &mut Encoder, model_push: &mut [f32; 32], water: bool) {
        enc.set_vertex_buffer(self.scene.vertices, 0);
        enc.set_index_buffer(self.scene.indices, 0);
        let mut bound = usize::MAX;
        for d in &self.scene.draws {
            if d.water && !water {
                continue;
            }
            if bound != d.material {
                enc.set_bind_group(1, self.materials[d.material]);
                bound = d.material;
            }
            let m = &self.scene.materials[d.material];
            let detail = &self.details[d.material];
            model_push[..16].copy_from_slice(detail);
            // A colour-only rule (preset `none`) recolours the material whatever its shading,
            // foliage included; patterned presets take the colour in their own way.
            let mut base = m.base_color;
            if detail[0] == 0.0 && detail[8] > 0.5 {
                base[..3].copy_from_slice(&detail[5..8]);
            }
            model_push[16..20].copy_from_slice(&base);
            model_push[20..24].copy_from_slice(&m.emissive);
            model_push[24..28].copy_from_slice(&m.params);
            model_push[28..32].copy_from_slice(&m.flags);
            enc.push_constants(&f32_bytes(model_push));
            enc.draw_indexed(d.count, 1, d.first_index, 0, 0);
        }
    }

    fn post_pass(
        enc: &mut Encoder,
        target: TextureId,
        mip: u32,
        load: Load,
        pipeline: PipelineId,
        group: BindGroupId,
        push: [f32; 16],
    ) {
        enc.begin_pass(PassDesc {
            colors: vec![Attachment {
                texture: target,
                mip,
                layer: 0,
                load,
                store: true,
            }],
            depth: None,
        });
        enc.set_pipeline(pipeline);
        enc.set_bind_group(0, group);
        enc.push_constants(&f32_bytes(&push));
        enc.draw(3, 1, 0, 0);
        enc.end_pass();
    }

    /// Record one frame. `target` is the swapchain texture for [`Output::Surface`] renderers
    /// and ignored for stills (which write their readback target). `jitter` is a sub-pixel
    /// offset in pixels; `seed` varies per accumulated sample.
    #[allow(clippy::too_many_arguments)]
    pub fn encode(
        &mut self,
        device: &mut Device<'_>,
        enc: &mut Encoder,
        view: &View,
        env: &Environment,
        jitter: [f32; 2],
        seed: u32,
        target: Option<TextureId>,
    ) -> Result<()> {
        let slot_index = match self.output {
            Output::Still => 0,
            Output::Surface(_) => device.frame_slot() % self.slots.len(),
        };
        // The world map is drawn in this frame's first passes, before anything reads it.
        let draw_world = !self.world_drawn;
        self.world_drawn = true;
        // Props first: their bounds widen the shadow cascades' depth range.
        self.prepare_props(device, slot_index, view.eye)?;
        let t = &self.targets;
        let size = (t.width, t.height);
        let eye = self.eye(view, jitter, size);
        let cas = self.cascades(view, env);
        let ground = self.ground_view(view);
        let lamps_on = env.lamp_level > 0.001 && !self.scene.lights.is_empty();
        let water = self.scene.water_level;
        // Mirror camera for the water reflection.
        let refl_eye = water.map(|h| {
            let r = reflect_y(h);
            let base = self.eye(view, [0.0, 0.0], t.refl);
            let view_proj = mul(&base.view_proj, &r);
            Eye {
                view_proj,
                inv: invert(&view_proj).unwrap_or(math::IDENTITY),
                pos: [view.eye[0], 2.0 * h - view.eye[1], view.eye[2]],
                near: base.near,
            }
        });
        {
            let slot = &self.slots[slot_index];
            let (tiles, index) = self.tile_lights(&eye, size, t.tiles[0], lamps_on);
            device.write_buffer(slot.tiles[0], 0, &crate::gpu::u32_bytes(&tiles))?;
            device.write_buffer(slot.index[0], 0, &crate::gpu::u32_bytes(&index))?;
            let block = self.frame_block(&eye, env, &cas, &ground, size, t.tiles[0], false, seed);
            device.write_buffer(slot.frame[0], 0, &block)?;
            if let Some(re) = &refl_eye {
                let (tiles, index) = self.tile_lights(re, t.refl, t.tiles[1], lamps_on);
                device.write_buffer(slot.tiles[1], 0, &crate::gpu::u32_bytes(&tiles))?;
                device.write_buffer(slot.index[1], 0, &crate::gpu::u32_bytes(&index))?;
                let block =
                    self.frame_block(re, env, &cas, &ground, t.refl, t.tiles[1], true, seed);
                device.write_buffer(slot.frame[1], 0, &block)?;
            }
        }
        let slot = &self.slots[slot_index];
        let t = &self.targets;
        let p = &self.pipelines;

        // Shadow cascades.
        if env.key_intensity > 0.0 {
            for (layer, m) in cas.mats.iter().enumerate() {
                enc.begin_pass(PassDesc {
                    colors: vec![],
                    depth: Some(Attachment {
                        texture: self.shadow_map,
                        mip: 0,
                        layer: layer as u32,
                        load: Load::Clear([1.0; 4]),
                        store: true,
                    }),
                });
                enc.set_pipeline(p.shadow);
                enc.set_vertex_buffer(self.scene.vertices, 0);
                enc.set_index_buffer(self.scene.indices, 0);
                enc.push_constants(&f32_bytes(m));
                let masked =
                    |d: &&super::scene::DrawItem| self.shadow_materials[d.material].is_some();
                for d in self.scene.draws.iter().filter(|d| !d.water && !masked(d)) {
                    enc.draw_indexed(d.count, 1, d.first_index, 0, 0);
                }
                let mut bound = false;
                for d in self.scene.draws.iter().filter(|d| !d.water && masked(d)) {
                    if !bound {
                        enc.set_pipeline(p.shadow_masked);
                        bound = true;
                    }
                    let mat = &self.scene.materials[d.material];
                    if let Some(g) = self.shadow_materials[d.material] {
                        enc.set_bind_group(1, g);
                    }
                    let mut push = [0.0f32; 20];
                    push[..16].copy_from_slice(m);
                    push[16..].copy_from_slice(&[
                        mat.params[0],
                        mat.base_color[3],
                        mat.flags[0],
                        0.0,
                    ]);
                    enc.push_constants(&f32_bytes(&push));
                    enc.draw_indexed(d.count, 1, d.first_index, 0, 0);
                }
                self.draw_prop_shadows(enc, slot_index, m);
                enc.end_pass();
            }
        } else {
            for layer in 0..2 {
                enc.begin_pass(PassDesc {
                    colors: vec![],
                    depth: Some(Attachment {
                        texture: self.shadow_map,
                        mip: 0,
                        layer,
                        load: Load::Clear([1.0; 4]),
                        store: true,
                    }),
                });
                enc.end_pass();
            }
        }

        // Rain and petals wrap around a box just ahead of the camera.
        let fwd = normalize(sub(view.target, view.eye));
        let boxes = [RAIN_BOX, PETAL_BOX].map(|b| add(view.eye, scale(fwd, b * 0.5 - 2.0)));
        // The ground map around the view, and on the first frame the world map.
        self.draw_ground(enc, self.ground_depth, self.ground_mask, &ground.vp);
        if draw_world {
            self.draw_ground(enc, self.world_depth, self.world_mask, &self.world.vp);
        }

        let mut push = [0.0f32; 32];
        let clear = [0.0, 0.0, 0.0, 0.0];
        // Water reflection.
        if refl_eye.is_some() {
            enc.begin_pass(PassDesc {
                colors: vec![
                    Attachment::clear(t.refl_hdr, clear),
                    Attachment::clear(t.refl_aux, clear),
                ],
                depth: Some(Attachment::clear(t.refl_depth, clear)),
            });
            enc.set_pipeline(p.scene);
            enc.set_bind_group(0, slot.scene[1]);
            self.draw_world(enc, &mut push, false);
            self.draw_props(enc, slot_index, 1);
            enc.set_pipeline(p.sky);
            enc.set_bind_group(0, slot.sky[1]);
            enc.draw(3, 1, 0, 0);
            enc.end_pass();
        }

        // Main pass: world, sky, weather.
        enc.begin_pass(PassDesc {
            colors: vec![
                Attachment::clear(t.hdr, clear),
                Attachment::clear(t.aux, clear),
            ],
            depth: Some(Attachment::clear(t.depth, clear)),
        });
        enc.set_pipeline(p.scene);
        enc.set_bind_group(0, slot.scene[0]);
        self.draw_world(enc, &mut push, true);
        self.draw_props(enc, slot_index, 0);
        let blades = self.grass_instances();
        if blades > 0 {
            enc.set_pipeline(p.grass);
            enc.set_bind_group(0, slot.grass);
            enc.draw(15, blades, 0, 0);
            if self.look.grass.flowers > 0.0 {
                enc.set_pipeline(p.flower);
                enc.set_bind_group(0, slot.flower);
                enc.draw(6, blades, 0, 0);
            }
        }
        enc.set_pipeline(p.sky);
        enc.set_bind_group(0, slot.sky[0]);
        enc.draw(3, 1, 0, 0);
        if self.quality.particles {
            enc.set_pipeline(p.particle);
            enc.set_bind_group(0, slot.particle);
            let ground = self.scene.water_level.unwrap_or(self.scene.min[1]);
            let particle_push = |params: [f32; 4], tint: [f32; 4], centre: V3| {
                let mut pp = [0.0f32; 12];
                pp[..4].copy_from_slice(&params);
                pp[4..8].copy_from_slice(&tint);
                pp[8..11].copy_from_slice(&centre);
                pp
            };
            let rain = (env.rain * 26000.0) as u32;
            if rain > 0 {
                let pp = particle_push(
                    [0.0, ground, RAIN_BOX, 1.0],
                    [0.7, 0.75, 0.85, 0.3],
                    boxes[0],
                );
                enc.push_constants(&f32_bytes(&pp));
                enc.draw(6, rain, 0, 0);
            }
            let petals = (env.petals * 3500.0) as u32;
            if petals > 0 {
                let pp = particle_push(
                    [1.0, ground, PETAL_BOX, 1.0],
                    [0.95, 0.5, 0.62, 0.9],
                    boxes[1],
                );
                enc.push_constants(&f32_bytes(&pp));
                enc.draw(6, petals, 0, 0);
            }
        }
        self.draw_blended_props(enc, slot_index);
        enc.end_pass();

        let l = &self.look;
        let px_scale = t.height as f32 / 1080.0;
        // Ambient occlusion.
        let mut pc = [0.0f32; 16];
        pc[..4].copy_from_slice(&[
            l.ao_radius,
            self.quality.ao_samples as f32,
            t.ao.0 as f32,
            t.ao.1 as f32,
        ]);
        Self::post_pass(enc, t.ao_tex, 0, Load::DontCare, p.ssao, slot.ssao, pc);
        // Air.
        let mut pc = [0.0f32; 16];
        pc[..4].copy_from_slice(&[
            l.ambient_occlusion,
            l.outline,
            (1.4 * px_scale).max(1.0),
            0.0,
        ]);
        pc[4..7].copy_from_slice(&l.ao_tint);
        pc[8..11].copy_from_slice(&l.outline_color);
        Self::post_pass(enc, t.lit, 0, Load::DontCare, p.air, slot.air, pc);
        // Bloom down and up.
        for level in 0..t.bloom_levels {
            let (sw, sh) = if level == 0 {
                (t.width, t.height)
            } else {
                (
                    (t.bloom_size.0 >> (level - 1)).max(1),
                    (t.bloom_size.1 >> (level - 1)).max(1),
                )
            };
            let mut pc = [0.0f32; 16];
            pc[..4].copy_from_slice(&[
                1.0 / sw as f32,
                1.0 / sh as f32,
                l.bloom_threshold,
                if level == 0 { 1.0 } else { 0.0 },
            ]);
            Self::post_pass(
                enc,
                t.bloom,
                level,
                Load::DontCare,
                p.bloom_down,
                slot.down[level as usize],
                pc,
            );
        }
        for level in (0..t.bloom_levels.saturating_sub(1)).rev() {
            let (sw, sh) = (
                (t.bloom_size.0 >> (level + 1)).max(1),
                (t.bloom_size.1 >> (level + 1)).max(1),
            );
            let mut pc = [0.0f32; 16];
            pc[..4].copy_from_slice(&[1.0 / sw as f32, 1.0 / sh as f32, 1.0, 0.0]);
            Self::post_pass(
                enc,
                t.bloom,
                level,
                Load::Load,
                p.bloom_up,
                slot.up[level as usize],
                pc,
            );
        }
        // Tone mapping and grading.
        let mut pc = [0.0f32; 16];
        pc[..4].copy_from_slice(&[l.exposure * env.exposure, l.bloom, l.saturation, l.contrast]);
        pc[4..7].copy_from_slice(&l.lift);
        pc[7] = l.vignette;
        pc[8..11].copy_from_slice(&l.gain);
        Self::post_pass(enc, t.ldr, 0, Load::DontCare, p.tonemap, slot.tonemap, pc);
        if self.quality.painterly {
            let mut pc = [0.0f32; 16];
            pc[..4].copy_from_slice(&[
                (l.painterly * px_scale * 0.6).clamp(0.0, 6.0),
                (l.painterly * l.painterly_far.max(0.0) * px_scale).clamp(0.0, 6.0),
                0.85,
                0.0,
            ]);
            Self::post_pass(enc, t.paint, 0, Load::DontCare, p.paint, slot.paint, pc);
        }
        let (dest, encode_srgb) = match self.output {
            Output::Still => (t.still.ok_or("still target missing")?, false),
            Output::Surface(f) => (target.ok_or("no surface texture")?, !f.is_srgb()),
        };
        let mut pc = [0.0f32; 16];
        pc[0] = if self.quality.fxaa { 1.0 } else { 0.0 };
        pc[1] = if encode_srgb { 1.0 } else { 0.0 };
        pc[2] = l.sharpen.max(0.0);
        Self::post_pass(enc, dest, 0, Load::DontCare, p.output, slot.output, pc);
        if self.has_overlays() {
            enc.begin_pass(PassDesc {
                colors: vec![Attachment {
                    texture: dest,
                    mip: 0,
                    layer: 0,
                    load: Load::Load,
                    store: true,
                }],
                depth: Some(Attachment {
                    texture: t.depth,
                    mip: 0,
                    layer: 0,
                    load: Load::Load,
                    store: false,
                }),
            });
            self.draw_overlays(enc, slot_index, encode_srgb);
            enc.end_pass();
        }
        if let (Output::Still, Some(still), Some(rb)) = (self.output, t.still, t.readback) {
            enc.copy_texture_to_buffer(still, 0, 0, rb, 0);
        }
        Ok(())
    }

    /// Render a still: `samples` jittered frames averaged in linear display space. Returns
    /// RGBA8 sRGB pixels, top row first.
    pub fn render_still(
        &mut self,
        device: &mut Device<'_>,
        view: &View,
        env: &Environment,
        samples: u32,
    ) -> Result<Vec<u8>> {
        if self.output != Output::Still {
            return Err("render_still needs a renderer created with Output::Still".into());
        }
        let (w, h) = self.size();
        let n = (w * h * 4) as usize;
        let mut acc = vec![0.0f32; n];
        let samples = samples.max(1);
        for s in 0..samples {
            let jitter = if samples == 1 {
                [0.0, 0.0]
            } else {
                [halton(s + 1, 2) - 0.5, halton(s + 1, 3) - 0.5]
            };
            let mut enc = device.encoder();
            self.encode(device, &mut enc, view, env, jitter, s, None)?;
            device.submit(enc)?;
            let bytes = device.read_buffer(self.targets.readback.ok_or("no readback")?)?;
            for (i, a) in acc.iter_mut().enumerate() {
                let h = u16::from_le_bytes([bytes[i * 2], bytes[i * 2 + 1]]);
                *a += f16_to_f32(h);
            }
        }
        let inv = 1.0 / samples as f32;
        let mut out = vec![0u8; n];
        for (i, (o, a)) in out.iter_mut().zip(&acc).enumerate() {
            let v = a * inv;
            if i % 4 == 3 {
                *o = 255;
            } else {
                // Ordered dither before quantizing to avoid banding in skies.
                let px = (i / 4) as u32;
                let d = (halton(px % 4096 + 1, 5) - 0.5) / 255.0;
                *o = ((math::linear_to_srgb(v) + d) * 255.0)
                    .round()
                    .clamp(0.0, 255.0) as u8;
            }
        }
        Ok(out)
    }
}
