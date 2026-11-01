//! Shader libraries, render and compute pipelines, depth-stencil states, bind groups and the
//! internal pipeline used for scaled blits.
use super::ffi;
use super::resources::{compare_function, pixel_format};
use super::*;
use crate::apple::objc::nsstring;
use crate::gpu::{
    Blend, BufferUsage, Cull, RenderPipelineDesc, Resource, TextureUsage, Topology, VertexFormat,
};
use abe_shader::{Compiled, IoType, SampleType, Stage, TexelFormat, TextureView};

const MAX_BIND_GROUPS: usize = 8;

fn stage_bit(s: Stage) -> u32 {
    match s {
        Stage::Vertex => 1,
        Stage::Fragment => 2,
        Stage::Compute => 4,
    }
}

fn texel_format(f: TexelFormat) -> Format {
    match f {
        TexelFormat::Rgba8Unorm => Format::Rgba8Unorm,
        TexelFormat::Rgba16Float => Format::Rgba16Float,
        TexelFormat::Rgba32Float => Format::Rgba32Float,
        TexelFormat::R32Float => Format::R32Float,
        TexelFormat::Rg16Float => Format::Rg16Float,
        TexelFormat::R16Float => Format::R16Float,
    }
}

fn is_float(ty: IoType) -> bool {
    matches!(
        ty,
        IoType::F32 | IoType::Vec2F | IoType::Vec3F | IoType::Vec4F
    )
}
fn is_uint(ty: IoType) -> bool {
    matches!(
        ty,
        IoType::U32 | IoType::Vec2U | IoType::Vec3U | IoType::Vec4U
    )
}

fn merge_kind(a: BindingKind, b: BindingKind) -> Option<BindingKind> {
    match (a, b) {
        (BindingKind::UniformBuffer { size: x }, BindingKind::UniformBuffer { size: y }) => {
            Some(BindingKind::UniformBuffer { size: x.max(y) })
        }
        (
            BindingKind::StorageBuffer { read_only: x },
            BindingKind::StorageBuffer { read_only: y },
        ) => Some(BindingKind::StorageBuffer { read_only: x && y }),
        (a, b) => (a == b).then_some(a),
    }
}

/// The reflected bindings of every stage merged into one sorted entry list per group.
fn merge_bindings(stages: &[&Compiled]) -> Result<Vec<Vec<Entry>>> {
    let mut groups: Vec<Vec<Entry>> = Vec::new();
    for shader in stages {
        let bit = stage_bit(shader.stage);
        for b in &shader.bindings {
            if b.group as usize >= MAX_BIND_GROUPS {
                return Err(format!("@group({}) is above the supported 0..=7", b.group));
            }
            let g = b.group as usize;
            if g >= groups.len() {
                groups.resize(g + 1, Vec::new());
            }
            match groups[g].iter_mut().find(|e| e.binding == b.binding) {
                Some(e) => {
                    e.kind = merge_kind(e.kind, b.kind).ok_or_else(|| {
                        format!(
                            "@group({}) @binding({}) is declared as {:?} and {:?}",
                            b.group, b.binding, e.kind, b.kind
                        )
                    })?;
                    e.stages |= bit;
                }
                None => groups[g].push(Entry {
                    binding: b.binding,
                    kind: b.kind,
                    stages: bit,
                }),
            }
        }
    }
    for g in &mut groups {
        g.sort_by_key(|e| e.binding);
    }
    Ok(groups)
}

/// Compiles a shader's MSL and returns its entry function (+1) and its binding map.
fn function(device: Id, shader: &Compiled, invariant: &mut bool) -> Result<(Obj, StageMap)> {
    let msl = shader.msl.as_ref().ok_or(
        "the shader has no Metal Shading Language; compile it with abe_shader::compile on an \
         Apple target or with abe_shader::compile_msl",
    )?;
    *invariant |= msl.invariant;
    let source = nsstring(&msl.source);
    let name = nsstring(&msl.entry_point);
    // SAFETY: MTLCompileOptions and the library are owned; the error is autoreleased.
    unsafe {
        let options = Gpu::new_object(class!("MTLCompileOptions"), "MTLCompileOptions")?;
        if msl.invariant
            && send!(options.id(), "respondsToSelector:", crate::apple::objc::sel!("setPreserveInvariance:") => crate::apple::objc::Sel; i8)
                != 0
        {
            send!(options.id(), "setPreserveInvariance:", true => bool; ());
        }
        let mut error: Id = core::ptr::null_mut();
        let library = Obj::owned(send!(
            device,
            "newLibraryWithSource:options:error:",
            source.id() => Id,
            options.id() => Id,
            &mut error as *mut Id => *mut Id;
            Id
        ))
        .ok_or_else(|| {
            ns_error(
                &format!(
                    "Metal rejected the {:?} shader {}",
                    shader.stage, msl.entry_point
                ),
                error,
            )
        })?;
        let f = Obj::owned(send!(library.id(), "newFunctionWithName:", name.id() => Id; Id))
            .ok_or_else(|| format!("{} is missing from its Metal library", msl.entry_point))?;
        let kind = match shader.stage {
            Stage::Vertex => StageKind::Vertex,
            Stage::Fragment => StageKind::Fragment,
            Stage::Compute => StageKind::Compute,
        };
        Ok((
            f,
            StageMap {
                kind,
                slots: msl.slots.clone(),
                push: msl.push_constant_buffer.is_some(),
                sized: msl.sized_bindings.clone(),
            },
        ))
    }
}

/// `objectAtIndexedSubscript:` of an array property (colour attachments, vertex attributes).
unsafe fn at(array: Id, i: usize) -> Id {
    // SAFETY: the caller passes a Metal descriptor array, which creates entries on demand.
    unsafe { send!(array, "objectAtIndexedSubscript:", i => NSUInteger; Id) }
}

impl<'w> Gpu<'w> {
    fn depth_state(&mut self, compare: u64, write: bool) -> Result<Obj> {
        if let Some(s) = self.depth_states.get(&(compare, write)) {
            return Ok(s.clone());
        }
        let desc = Self::new_object(
            class!("MTLDepthStencilDescriptor"),
            "MTLDepthStencilDescriptor",
        )?;
        // SAFETY: setters on a depth-stencil descriptor, then an owned state or nil.
        let state = unsafe {
            send!(desc.id(), "setDepthCompareFunction:", compare as NSUInteger => NSUInteger; ());
            send!(desc.id(), "setDepthWriteEnabled:", write => bool; ());
            Obj::owned(send!(
                self.device.id(),
                "newDepthStencilStateWithDescriptor:",
                desc.id() => Id;
                Id
            ))
        }
        .ok_or("newDepthStencilStateWithDescriptor returned nil")?;
        self.depth_states.insert((compare, write), state.clone());
        Ok(state)
    }

    pub fn create_render_pipeline(&mut self, d: &RenderPipelineDesc) -> Result<PipelineId> {
        if d.vertex.stage != Stage::Vertex {
            return Err("create_render_pipeline: `vertex` is not a vertex shader".into());
        }
        if d.fragment.is_some_and(|f| f.stage != Stage::Fragment) {
            return Err("create_render_pipeline: `fragment` is not a fragment shader".into());
        }
        if d.colors.is_empty() && d.depth.is_none() {
            return Err("create_render_pipeline: no colour target and no depth".into());
        }
        if d.colors.len() > 8 {
            return Err(format!(
                "{} colour targets exceed the device limit",
                d.colors.len()
            ));
        }
        for (i, c) in d.colors.iter().enumerate() {
            if c.format.is_depth() {
                return Err(format!(
                    "colour target {i}: {:?} is not renderable",
                    c.format
                ));
            }
            if c.blend != Blend::Replace
                && matches!(c.format, Format::R32Float | Format::Rgba32Float)
                && !cfg!(target_os = "macos")
            {
                return Err(format!("colour target {i}: {:?} cannot blend", c.format));
            }
        }
        if d.depth.is_some_and(|z| !z.format.is_depth()) {
            return Err("create_render_pipeline: the depth format is not a depth format".into());
        }
        match d.fragment {
            None if !d.colors.is_empty() => {
                return Err("colour targets need a fragment shader".into());
            }
            Some(f) => {
                if let Some(o) = f.outputs.iter().find(|o| !is_float(o.ty)) {
                    return Err(format!(
                        "fragment @location({}) is {:?}; colour targets are float formats",
                        o.location, o.ty
                    ));
                }
                for (i, c) in d.colors.iter().enumerate() {
                    if c.write_mask & 0xF != 0 && !f.outputs.iter().any(|o| o.location == i as u32)
                    {
                        return Err(format!("colour target {i} has no @location({i}) output"));
                    }
                }
            }
            None => {}
        }
        self.check_vertex_input(d)?;
        let mut stages = vec![d.vertex];
        stages.extend(d.fragment);
        let groups = merge_bindings(&stages)?;
        let push_size = stages
            .iter()
            .map(|s| s.push_constant_size)
            .max()
            .unwrap_or(0)
            .next_multiple_of(4);
        if push_size > 4096 {
            return Err(format!(
                "{push_size} bytes of push constants exceed Metal's 4096-byte set*Bytes limit"
            ));
        }
        autoreleasepool(|| {
            let device = self.device.id();
            let mut invariant = false;
            let (vs, vmap) = function(device, d.vertex, &mut invariant)?;
            let fs = d
                .fragment
                .map(|f| function(device, f, &mut invariant))
                .transpose()?;
            let desc = Self::new_object(
                class!("MTLRenderPipelineDescriptor"),
                "MTLRenderPipelineDescriptor",
            )?;
            // SAFETY: setters on a render pipeline descriptor and its sub-descriptors; the
            // pipeline state is owned and the error autoreleased.
            let state = unsafe {
                let p = desc.id();
                send!(p, "setVertexFunction:", vs.id() => Id; ());
                if let Some((f, _)) = &fs {
                    send!(p, "setFragmentFunction:", f.id() => Id; ());
                }
                let colors = send!(p, "colorAttachments"; Id);
                for (i, c) in d.colors.iter().enumerate() {
                    let a = at(colors, i);
                    send!(a, "setPixelFormat:", pixel_format(c.format) as NSUInteger => NSUInteger; ());
                    let mut mask = 0;
                    for (bit, m) in [
                        (1, ffi::write_mask::RED),
                        (2, ffi::write_mask::GREEN),
                        (4, ffi::write_mask::BLUE),
                        (8, ffi::write_mask::ALPHA),
                    ] {
                        if c.write_mask & bit != 0 {
                            mask |= m;
                        }
                    }
                    send!(a, "setWriteMask:", mask as NSUInteger => NSUInteger; ());
                    use ffi::blend::*;
                    // (src colour, dst colour, src alpha, dst alpha); the operation is add.
                    let factors = match c.blend {
                        Blend::Replace => None,
                        Blend::Alpha => Some((
                            SOURCE_ALPHA,
                            ONE_MINUS_SOURCE_ALPHA,
                            ONE,
                            ONE_MINUS_SOURCE_ALPHA,
                        )),
                        Blend::Premultiplied => {
                            Some((ONE, ONE_MINUS_SOURCE_ALPHA, ONE, ONE_MINUS_SOURCE_ALPHA))
                        }
                        Blend::Additive => Some((ONE, ONE, ONE, ONE)),
                    };
                    if let Some((sc, dc, sa, da)) = factors {
                        send!(a, "setBlendingEnabled:", true => bool; ());
                        send!(a, "setSourceRGBBlendFactor:", sc as NSUInteger => NSUInteger; ());
                        send!(a, "setDestinationRGBBlendFactor:", dc as NSUInteger => NSUInteger; ());
                        send!(a, "setRgbBlendOperation:", ADD as NSUInteger => NSUInteger; ());
                        send!(a, "setSourceAlphaBlendFactor:", sa as NSUInteger => NSUInteger; ());
                        send!(a, "setDestinationAlphaBlendFactor:", da as NSUInteger => NSUInteger; ());
                        send!(a, "setAlphaBlendOperation:", ADD as NSUInteger => NSUInteger; ());
                    }
                }
                if let Some(z) = &d.depth {
                    send!(p, "setDepthAttachmentPixelFormat:", pixel_format(z.format) as NSUInteger => NSUInteger; ());
                }
                if let Some(layout) = &d.vertex_layout {
                    let vd = send!(class!("MTLVertexDescriptor"), "vertexDescriptor"; Id);
                    let attributes = send!(vd, "attributes"; Id);
                    for a in &layout.attributes {
                        let format = match a.format {
                            VertexFormat::F32 => ffi::vertex_format::FLOAT,
                            VertexFormat::F32x2 => ffi::vertex_format::FLOAT2,
                            VertexFormat::F32x3 => ffi::vertex_format::FLOAT3,
                            VertexFormat::F32x4 => ffi::vertex_format::FLOAT4,
                            VertexFormat::U32 => ffi::vertex_format::UINT,
                            VertexFormat::Unorm8x4 => ffi::vertex_format::UCHAR4_NORMALIZED,
                        };
                        let e = at(attributes, a.location as usize);
                        send!(e, "setFormat:", format as NSUInteger => NSUInteger; ());
                        send!(e, "setOffset:", a.offset as NSUInteger => NSUInteger; ());
                        send!(e, "setBufferIndex:", abe_shader::VERTEX_BUFFER as NSUInteger => NSUInteger; ());
                    }
                    let l = at(send!(vd, "layouts"; Id), abe_shader::VERTEX_BUFFER as usize);
                    send!(l, "setStride:", layout.stride as NSUInteger => NSUInteger; ());
                    send!(l, "setStepFunction:", ffi::STEP_PER_VERTEX as NSUInteger => NSUInteger; ());
                    send!(l, "setStepRate:", 1usize => NSUInteger; ());
                    send!(p, "setVertexDescriptor:", vd => Id; ());
                }
                let label = nsstring(&format!(
                    "{}{}",
                    d.vertex.msl.as_ref().map_or("", |m| m.entry_point.as_str()),
                    d.fragment
                        .and_then(|f| f.msl.as_ref())
                        .map_or(String::new(), |m| format!(" + {}", m.entry_point))
                ));
                send!(p, "setLabel:", label.id() => Id; ());
                let mut error: Id = core::ptr::null_mut();
                Obj::owned(send!(
                    self.device.id(),
                    "newRenderPipelineStateWithDescriptor:error:",
                    p => Id,
                    &mut error as *mut Id => *mut Id;
                    Id
                ))
                .ok_or_else(|| ns_error("Metal rejected the render pipeline", error))?
            };
            let _ = invariant;
            let depth_state = match &d.depth {
                Some(z) => self.depth_state(compare_function(z.compare), z.write)?,
                None => self.depth_state(ffi::compare::ALWAYS, false)?,
            };
            let raster = Raster {
                primitive: match d.topology {
                    Topology::TriangleList => ffi::primitive::TRIANGLE,
                    Topology::TriangleStrip => ffi::primitive::TRIANGLE_STRIP,
                    Topology::LineList => ffi::primitive::LINE,
                },
                cull: match d.cull {
                    Cull::None => ffi::cull::NONE,
                    Cull::Front => ffi::cull::FRONT,
                    Cull::Back => ffi::cull::BACK,
                },
                // The MSL vertex stage flips Y, which keeps Vulkan's visual winding.
                winding: if d.front_ccw {
                    ffi::winding::COUNTER_CLOCKWISE
                } else {
                    ffi::winding::CLOCKWISE
                },
                depth_bias: d
                    .depth
                    .map_or((0.0, 0.0), |z| (z.bias_constant, z.bias_slope)),
                clip: if d.depth.is_some_and(|z| z.clamp) && self.depth_clamp {
                    ffi::depth_clip::CLAMP
                } else {
                    ffi::depth_clip::CLIP
                },
                depth_state,
            };
            let mut maps = vec![vmap];
            maps.extend(fs.map(|(_, m)| m));
            let raw = self.pipelines.insert(Pipeline {
                state,
                compute: false,
                groups,
                stages: maps,
                push_size,
                colors: d.colors.iter().map(|c| c.format).collect(),
                depth: d.depth.map(|z| z.format),
                vertex_input: d.vertex_layout.is_some(),
                raster: Some(raster),
                threads: [1, 1, 1],
            })?;
            Ok(PipelineId::from_raw(raw))
        })
    }

    fn check_vertex_input(&self, d: &RenderPipelineDesc) -> Result<()> {
        let shader = d.vertex;
        let Some(layout) = &d.vertex_layout else {
            if let Some(input) = shader.inputs.first() {
                return Err(format!(
                    "the vertex shader reads @location({}) but the pipeline has no vertex layout",
                    input.location
                ));
            }
            return Ok(());
        };
        if layout.stride == 0 || layout.stride % 4 != 0 {
            return Err(format!(
                "invalid vertex stride {} (Metal needs a positive multiple of 4)",
                layout.stride
            ));
        }
        for (i, a) in layout.attributes.iter().enumerate() {
            if layout.attributes[..i]
                .iter()
                .any(|b| b.location == a.location)
            {
                return Err(format!("vertex @location({}) is listed twice", a.location));
            }
            if a.location >= 31 || a.offset + a.format.size() > layout.stride {
                return Err(format!("vertex attribute {a:?} exceeds device limits"));
            }
        }
        for input in &shader.inputs {
            let a = layout
                .attributes
                .iter()
                .find(|a| a.location == input.location)
                .ok_or_else(|| {
                    format!(
                        "the vertex shader reads @location({}) but the layout has no such attribute",
                        input.location
                    )
                })?;
            let ok = if a.format == VertexFormat::U32 {
                is_uint(input.ty)
            } else {
                is_float(input.ty)
            };
            if !ok {
                return Err(format!(
                    "vertex @location({}) is {:?} in the shader but {:?} in the layout",
                    input.location, input.ty, a.format
                ));
            }
        }
        Ok(())
    }

    pub fn create_compute_pipeline(&mut self, shader: &Compiled) -> Result<PipelineId> {
        if shader.stage != Stage::Compute {
            return Err("create_compute_pipeline: not a compute shader".into());
        }
        let groups = merge_bindings(&[shader])?;
        let push_size = shader.push_constant_size.next_multiple_of(4);
        autoreleasepool(|| {
            let mut invariant = false;
            let (f, map) = function(self.device.id(), shader, &mut invariant)?;
            // SAFETY: an owned pipeline state; the error is autoreleased.
            let state = unsafe {
                let mut error: Id = core::ptr::null_mut();
                Obj::owned(send!(
                    self.device.id(),
                    "newComputePipelineStateWithFunction:error:",
                    f.id() => Id,
                    &mut error as *mut Id => *mut Id;
                    Id
                ))
                .ok_or_else(|| ns_error("Metal rejected the compute pipeline", error))?
            };
            // SAFETY: a property of a live pipeline state.
            let max = unsafe { send!(state.id(), "maxTotalThreadsPerThreadgroup"; NSUInteger) };
            let [x, y, z] = shader.workgroup_size;
            if (x as usize) * (y as usize) * (z as usize) > max {
                return Err(format!(
                    "@workgroup_size({x}, {y}, {z}) exceeds the {max} threads this pipeline allows"
                ));
            }
            let raw = self.pipelines.insert(Pipeline {
                state,
                compute: true,
                groups,
                stages: vec![map],
                push_size,
                colors: Vec::new(),
                depth: None,
                vertex_input: false,
                raster: None,
                threads: shader.workgroup_size,
            })?;
            Ok(PipelineId::from_raw(raw))
        })
    }

    pub fn create_bind_group(
        &mut self,
        pipeline: PipelineId,
        group: u32,
        entries: &[(u32, Resource)],
    ) -> Result<crate::gpu::BindGroupId> {
        for (i, (binding, _)) in entries.iter().enumerate() {
            if entries[..i].iter().any(|(b, _)| b == binding) {
                return Err(format!("@binding({binding}) is given twice"));
            }
        }
        if group as usize >= MAX_BIND_GROUPS {
            return Err(format!("@group({group}) is above the supported 0..=7"));
        }
        let layout = self
            .pipeline(pipeline)?
            .groups
            .get(group as usize)
            .cloned()
            .unwrap_or_default();
        let limits = self.limits();
        let mut bound = Vec::with_capacity(layout.len());
        for e in &layout {
            let (_, res) = *entries
                .iter()
                .find(|(b, _)| *b == e.binding)
                .ok_or_else(|| format!("@group({group}) @binding({}) is missing", e.binding))?;
            let at = format!("@group({group}) @binding({})", e.binding);
            let b = match (e.kind, res) {
                (
                    BindingKind::UniformBuffer { .. } | BindingKind::StorageBuffer { .. },
                    Resource::Buffer { id, offset, size },
                ) => {
                    let b = self.buffer(id)?;
                    let uniform = matches!(e.kind, BindingKind::UniformBuffer { .. });
                    let (usage, align) = if uniform {
                        (BufferUsage::UNIFORM, limits.uniform_offset_alignment)
                    } else {
                        (BufferUsage::STORAGE, limits.storage_offset_alignment)
                    };
                    if !b.desc.usage.contains(usage) {
                        return Err(format!("{at}: {id:?} lacks {usage:?} usage"));
                    }
                    if offset >= b.desc.size || offset % align != 0 {
                        return Err(format!(
                            "{at}: offset {offset} must be inside the buffer and a multiple of {align}"
                        ));
                    }
                    let range = if size == 0 {
                        b.desc.size - offset
                    } else {
                        size
                    };
                    if offset + range > b.desc.size {
                        return Err(format!("{at}: range {range} at {offset} is out of bounds"));
                    }
                    if let BindingKind::UniformBuffer { size: need } = e.kind
                        && range < u64::from(need)
                    {
                        return Err(format!(
                            "{at}: the shader reads {need} bytes but only {range} are bound"
                        ));
                    }
                    Bound::Buffer {
                        id,
                        offset,
                        size: range,
                    }
                }
                (
                    BindingKind::Texture { view, .. } | BindingKind::StorageTexture { view, .. },
                    Resource::Texture(id)
                    | Resource::TextureLayer(id, _)
                    | Resource::TextureMip(id, _),
                ) => {
                    let desc = self.texture(id)?.desc;
                    let storage = matches!(e.kind, BindingKind::StorageTexture { .. });
                    match e.kind {
                        BindingKind::Texture { sample, .. } => {
                            if !desc.usage.contains(TextureUsage::SAMPLED) {
                                return Err(format!("{at}: {id:?} lacks SAMPLED usage"));
                            }
                            let ok = match sample {
                                SampleType::Depth => desc.format.is_depth(),
                                // Metal binds depth textures only to depth texture types.
                                SampleType::Float => !desc.format.is_depth(),
                                SampleType::Sint | SampleType::Uint => false,
                            };
                            if !ok {
                                return Err(format!(
                                    "{at}: a {sample:?} texture binding cannot sample {:?}",
                                    desc.format
                                ));
                            }
                        }
                        BindingKind::StorageTexture { format, .. } => {
                            if !desc.usage.contains(TextureUsage::STORAGE) {
                                return Err(format!("{at}: {id:?} lacks STORAGE usage"));
                            }
                            if texel_format(format) != desc.format {
                                return Err(format!(
                                    "{at}: the shader writes {format:?} but {id:?} is {:?}",
                                    desc.format
                                ));
                            }
                        }
                        _ => unreachable!("matched texture kinds above"),
                    }
                    let (key, mips, layers) =
                        view_for(view, res, &desc, storage).map_err(|m| format!("{at}: {m}"))?;
                    let obj = self.view(id, key)?;
                    Bound::Texture {
                        id,
                        obj,
                        mips,
                        layers,
                    }
                }
                (BindingKind::Sampler { comparison }, Resource::Sampler(id)) => {
                    let s = self
                        .samplers
                        .get(id.raw())
                        .ok_or_else(|| format!("{at}: {id:?} does not belong to this device"))?;
                    if s.comparison != comparison {
                        return Err(format!(
                            "{at}: the shader declares a {} but {id:?} is {}",
                            if comparison {
                                "sampler_comparison"
                            } else {
                                "sampler"
                            },
                            if s.comparison {
                                "a comparison sampler"
                            } else {
                                "not a comparison sampler"
                            }
                        ));
                    }
                    Bound::Sampler(id)
                }
                (kind, res) => return Err(format!("{at} expects {kind:?} but got {res:?}")),
            };
            bound.push((e.binding, b));
        }
        let raw = self.groups.insert(BindGroup { layout, bound })?;
        Ok(crate::gpu::BindGroupId::from_raw(raw))
    }
}

/// The view a binding of dimension `dim` needs for `res`, with the mip and layer ranges it
/// covers.
fn view_for(
    dim: TextureView,
    res: Resource,
    d: &TextureDesc,
    storage: bool,
) -> Result<(ViewKey, (u32, u32), (u32, u32))> {
    let (mips, layers) = match res {
        Resource::TextureLayer(_, l) if l < d.layers => ((0, d.mips), (l, l + 1)),
        Resource::TextureMip(_, m) if m < d.mips => ((m, m + 1), (0, d.layers)),
        Resource::Texture(_) => ((0, d.mips), (0, d.layers)),
        _ => return Err(format!("{res:?} is outside the texture")),
    };
    let layer_count = layers.1 - layers.0;
    let ty = match dim {
        TextureView::D2 if layer_count == 1 => ffi::texture_type::D2,
        TextureView::D2 => {
            return Err(format!(
                "a 2D binding needs one layer but the texture has {layer_count}; bind \
                 Resource::TextureLayer"
            ));
        }
        TextureView::D2Array => ffi::texture_type::D2_ARRAY,
        TextureView::Cube if layer_count == 6 && d.width == d.height => ffi::texture_type::CUBE,
        TextureView::Cube => return Err("a cube binding needs 6 square layers".into()),
    };
    if storage && mips.1 - mips.0 != 1 {
        return Err("storage bindings need a single mip; bind Resource::TextureMip".into());
    }
    Ok((
        ViewKey {
            ty,
            base_mip: mips.0,
            mips: mips.1 - mips.0,
            base_layer: layers.0,
            layers: layer_count,
        },
        mips,
        layers,
    ))
}

/// Full-screen pipelines (one per destination format) for blits that scale or convert.
pub(super) struct Blitter {
    vertex: Compiled,
    fragment: Compiled,
    pipelines: HashMap<Format, PipelineId>,
    samplers: [SamplerId; 2],
}

const BLIT_WGSL: &str = "
@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var smp: sampler;
struct Out {
  @builtin(position) pos: vec4f,
  @location(0) uv: vec2f,
};
@vertex fn vs_blit(@builtin(vertex_index) i: u32) -> Out {
  let uv = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
  var o: Out;
  o.pos = vec4f(uv * 2.0 - 1.0, 0.0, 1.0);
  o.uv = uv;
  return o;
}
@fragment fn fs_blit(v: Out) -> @location(0) vec4f {
  return textureSampleLevel(src, smp, v.uv, 0.0);
}
";

impl<'w> Gpu<'w> {
    /// The blit pipeline rendering into `format` and a sampler for `filter`.
    pub(super) fn blit_pipeline(
        &mut self,
        format: Format,
        filter: crate::gpu::Filter,
    ) -> Result<(PipelineId, SamplerId)> {
        if self.blitter.is_none() {
            let compile = |stage, entry| {
                abe_shader::compile_msl(BLIT_WGSL, stage, entry)
                    .map_err(|e| format!("blit shader {entry}: {e}"))
            };
            let vertex = compile(Stage::Vertex, "vs_blit")?;
            let fragment = compile(Stage::Fragment, "fs_blit")?;
            let sampler = |filter| crate::gpu::SamplerDesc {
                filter,
                mip_filter: crate::gpu::Filter::Nearest,
                address: crate::gpu::Address::Clamp,
                ..crate::gpu::SamplerDesc::default()
            };
            let samplers = [
                self.create_sampler(&sampler(crate::gpu::Filter::Nearest))?,
                self.create_sampler(&sampler(crate::gpu::Filter::Linear))?,
            ];
            self.blitter = Some(Blitter {
                vertex,
                fragment,
                pipelines: HashMap::new(),
                samplers,
            });
        }
        let b = self.blitter.as_ref().expect("created above");
        let sampler = b.samplers[usize::from(filter == crate::gpu::Filter::Linear)];
        if let Some(&p) = b.pipelines.get(&format) {
            return Ok((p, sampler));
        }
        let (vertex, fragment) = (b.vertex.clone(), b.fragment.clone());
        let p = self.create_render_pipeline(&RenderPipelineDesc {
            vertex: &vertex,
            fragment: Some(&fragment),
            vertex_layout: None,
            topology: Topology::TriangleList,
            cull: Cull::None,
            front_ccw: true,
            depth: None,
            colors: vec![crate::gpu::ColorTarget {
                format,
                blend: Blend::Replace,
                write_mask: 0xF,
            }],
        })?;
        self.blitter
            .as_mut()
            .expect("created above")
            .pipelines
            .insert(format, p);
        Ok((p, sampler))
    }
}
