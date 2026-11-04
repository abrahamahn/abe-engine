//! Shader modules, layouts derived from reflection, pipelines, render passes, framebuffers and
//! bind groups.
use super::resources::{FEATURE_BLEND, FEATURE_COLOR, vk_compare, vk_format};
use super::track::{access, stage};
use super::*;
use crate::gpu::{Blend, BufferUsage, Cull, RenderPipelineDesc, Resource, Topology, VertexFormat};
use crate::gpu::{TextureUsage, VertexLayout};
use ganbate_shader::{Compiled, IoType, SampleType, Stage, TexelFormat, TextureView};

const VERTEX_BIT: u32 = 0x1;
const FRAGMENT_BIT: u32 = 0x10;
const COMPUTE_BIT: u32 = 0x20;
/// Descriptor pool capacity per kind, in `DESCRIPTOR_TYPES` order.
const POOL_SETS: u32 = 256;
const POOL_COUNTS: [u32; 5] = [256, 512, 128, 256, 256];

fn descriptor_slot(kind: &BindingKind) -> usize {
    match kind {
        BindingKind::Sampler { .. } => 0,
        BindingKind::Texture { .. } => 1,
        BindingKind::StorageTexture { .. } => 2,
        BindingKind::UniformBuffer { .. } => 3,
        BindingKind::StorageBuffer { .. } => 4,
    }
}
/// Pipeline stages that run the shader stages in `stages` (VkShaderStageFlags).
fn shader_pipeline_stages(stages: u32) -> u32 {
    let mut out = 0;
    if stages & VERTEX_BIT != 0 {
        out |= stage::VERTEX_SHADER;
    }
    if stages & FRAGMENT_BIT != 0 {
        out |= stage::FRAGMENT_SHADER;
    }
    if stages & COMPUTE_BIT != 0 {
        out |= stage::COMPUTE;
    }
    out
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
fn vertex_format(f: VertexFormat) -> i32 {
    match f {
        VertexFormat::F32 => 100,
        VertexFormat::F32x2 => 103,
        VertexFormat::F32x3 => 106,
        VertexFormat::F32x4 => 109,
        VertexFormat::U32 => 98,
        VertexFormat::Unorm8x4 => 37,
    }
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
/// Merges the reflected bindings of every stage: one entry list per group, sorted by binding.
fn merge_bindings(stages: &[(&Compiled, u32)]) -> Result<Vec<Vec<Entry>>> {
    let mut groups: Vec<Vec<Entry>> = Vec::new();
    for (shader, bit) in stages {
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
                    stages: *bit,
                }),
            }
        }
    }
    for g in &mut groups {
        g.sort_by_key(|e| e.binding);
    }
    Ok(groups)
}

impl<'w> Gpu<'w> {
    fn shader_module(&self, shader: &Compiled) -> Result<u64> {
        if shader.words.len() < 5 || shader.words[0] != 0x0723_0203 {
            return Err(format!("{:?} shader is not a SPIR-V module", shader.stage));
        }
        let mut ci = VkShaderModuleCreateInfo::new();
        ci.codeSize = shader.words.len() * 4;
        ci.pCode = shader.words.as_ptr();
        // SAFETY: the words outlive the call; the caller destroys the module.
        create("vkCreateShaderModule", |o| unsafe {
            vkCreateShaderModule(self.device, &ci, null(), o)
        })
    }

    fn set_layout(&mut self, entries: &[Entry]) -> Result<u64> {
        let key: Vec<(u32, i32, u32)> = entries
            .iter()
            .map(|e| {
                (
                    e.binding,
                    DESCRIPTOR_TYPES[descriptor_slot(&e.kind)],
                    e.stages,
                )
            })
            .collect();
        if let Some(&layout) = self.set_layouts.get(&key) {
            return Ok(layout);
        }
        let bindings: Vec<VkDescriptorSetLayoutBinding> = key
            .iter()
            .map(|&(binding, ty, stages)| VkDescriptorSetLayoutBinding {
                binding,
                descriptorType: ty,
                descriptorCount: 1,
                stageFlags: stages,
                pImmutableSamplers: null(),
            })
            .collect();
        let mut ci = VkDescriptorSetLayoutCreateInfo::new();
        ci.bindingCount = bindings.len() as u32;
        ci.pBindings = bindings.as_ptr();
        // SAFETY: the bindings outlive the call; layouts are cached and destroyed in Drop.
        let layout = create("vkCreateDescriptorSetLayout", |o| unsafe {
            vkCreateDescriptorSetLayout(self.device, &ci, null(), o)
        })?;
        self.set_layouts.insert(key, layout);
        Ok(layout)
    }

    fn intern_layout_id(&mut self, entries: &[Entry]) -> u32 {
        if entries.is_empty() {
            return 0;
        }
        if let Some(&id) = self.layout_ids.get(entries) {
            return id;
        }
        let id = (self.layout_ids.len() as u32)
            .checked_add(1)
            .expect("too many unique bind group layouts");
        self.layout_ids.insert(entries.to_vec(), id);
        id
    }

    /// Group layouts (with empty layouts for gaps) and the pipeline layout.
    fn layouts(
        &mut self,
        stages: &[(&Compiled, u32)],
    ) -> Result<(Vec<GroupLayout>, u64, u32, u32)> {
        let groups = merge_bindings(stages)?;
        for e in groups.iter().flatten() {
            let writes = matches!(
                e.kind,
                BindingKind::StorageBuffer { read_only: false }
                    | BindingKind::StorageTexture { .. }
            );
            let unsupported = if e.stages & VERTEX_BIT != 0 && !self.features.vertex_stores {
                "vertex"
            } else if e.stages & FRAGMENT_BIT != 0 && !self.features.fragment_stores {
                "fragment"
            } else {
                ""
            };
            if writes && !unsupported.is_empty() {
                return Err(format!(
                    "binding {} writes storage in a {unsupported} shader, which {} cannot do",
                    e.binding, self.name
                ));
            }
            if let BindingKind::StorageTexture {
                format: TexelFormat::Rg16Float | TexelFormat::R16Float,
                ..
            } = e.kind
            {
                if !self.features.storage_formats {
                    return Err(format!(
                        "{:?} storage textures are unsupported on {}",
                        e.kind, self.name
                    ));
                }
            }
        }
        if groups.len() > self.limits.maxBoundDescriptorSets as usize {
            return Err(format!(
                "the shaders use {} bind groups; the device binds at most {}",
                groups.len(),
                self.limits.maxBoundDescriptorSets
            ));
        }
        let mut layouts = Vec::with_capacity(groups.len());
        for entries in groups {
            let id = self.intern_layout_id(&entries);
            let set_layout = self.set_layout(&entries)?;
            layouts.push(GroupLayout {
                id,
                set_layout,
                entries,
            });
        }
        let push = stages
            .iter()
            .map(|(s, _)| s.push_constant_size)
            .max()
            .unwrap_or(0)
            .next_multiple_of(4);
        if push > self.limits.maxPushConstantsSize {
            return Err(format!(
                "{push} bytes of push constants exceed the device limit of {}",
                self.limits.maxPushConstantsSize
            ));
        }
        let push_stages = if push == 0 {
            0
        } else {
            stages.iter().fold(0, |acc, (_, bit)| acc | bit)
        };
        let sets: Vec<u64> = layouts.iter().map(|g| g.set_layout).collect();
        let key = (sets, push, push_stages);
        if let Some(&layout) = self.pipeline_layouts.get(&key) {
            return Ok((layouts, layout, push, push_stages));
        }
        let range = VkPushConstantRange {
            stageFlags: push_stages,
            offset: 0,
            size: push,
        };
        let mut ci = VkPipelineLayoutCreateInfo::new();
        ci.setLayoutCount = key.0.len() as u32;
        ci.pSetLayouts = key.0.as_ptr();
        if push > 0 {
            ci.pushConstantRangeCount = 1;
            ci.pPushConstantRanges = &range;
        }
        // SAFETY: the arrays outlive the call; layouts are cached and destroyed in Drop.
        let layout = create("vkCreatePipelineLayout", |o| unsafe {
            vkCreatePipelineLayout(self.device, &ci, null(), o)
        })?;
        self.pipeline_layouts.insert(key, layout);
        Ok((layouts, layout, push, push_stages))
    }

    /// A render pass whose attachments stay in their attachment layouts: every layout
    /// transition happens in explicit barriers recorded between passes.
    pub(super) fn render_pass(&mut self, key: &PassKey) -> Result<u64> {
        if let Some(&pass) = self.render_passes.get(key) {
            return Ok(pass);
        }
        let describe =
            |(format, load, store): (i32, i32, i32), layout: i32| VkAttachmentDescription {
                flags: 0,
                format,
                samples: 1,
                loadOp: load,
                storeOp: store,
                stencilLoadOp: 2,
                stencilStoreOp: 1,
                initialLayout: layout,
                finalLayout: layout,
            };
        let key_colors = key.colors();
        // SAFETY: zeroed attachment structs are overwritten up to `att_len` / `key_colors.len()`.
        let mut attachments =
            [unsafe { core::mem::zeroed::<VkAttachmentDescription>() }; MAX_PASS_COLORS + 1];
        let mut colors = [unsafe { core::mem::zeroed::<VkAttachmentReference>() }; MAX_PASS_COLORS];
        for (i, &c) in key_colors.iter().enumerate() {
            attachments[i] = describe(c, track::COLOR_ATTACHMENT);
            colors[i] = VkAttachmentReference {
                attachment: i as u32,
                layout: track::COLOR_ATTACHMENT,
            };
        }
        let mut att_len = key_colors.len();
        let depth = VkAttachmentReference {
            attachment: att_len as u32,
            layout: track::DEPTH_ATTACHMENT,
        };
        if let Some(d) = key.depth {
            attachments[att_len] = describe(d, track::DEPTH_ATTACHMENT);
            att_len += 1;
        }
        // SAFETY: zero is a valid VkSubpassDescription (no attachments, graphics bind point).
        let mut subpass: VkSubpassDescription = unsafe { core::mem::zeroed() };
        subpass.colorAttachmentCount = key_colors.len() as u32;
        subpass.pColorAttachments = colors.as_ptr();
        if key.depth.is_some() {
            subpass.pDepthStencilAttachment = &depth;
        }
        let mut ci = VkRenderPassCreateInfo::new();
        ci.attachmentCount = att_len as u32;
        ci.pAttachments = attachments.as_ptr();
        ci.subpassCount = 1;
        ci.pSubpasses = &subpass;
        // SAFETY: the arrays outlive the call; passes are cached and destroyed in Drop.
        let pass = create("vkCreateRenderPass", |o| unsafe {
            vkCreateRenderPass(self.device, &ci, null(), o)
        })?;
        self.render_passes.insert(*key, pass);
        Ok(pass)
    }

    pub(super) fn framebuffer(&mut self, pass: u64, views: &[u64], w: u32, h: u32) -> Result<u64> {
        let mut view_arr = [0u64; MAX_PASS_COLORS + 1];
        view_arr[..views.len()].copy_from_slice(views);
        let key = FramebufferKey {
            pass,
            views: view_arr,
            len: views.len() as u8,
            w,
            h,
        };
        if let Some(&f) = self.framebuffers.get(&key) {
            return Ok(f);
        }
        let mut ci = VkFramebufferCreateInfo::new();
        ci.renderPass = pass;
        ci.attachmentCount = views.len() as u32;
        ci.pAttachments = views.as_ptr();
        ci.width = w;
        ci.height = h;
        ci.layers = 1;
        // SAFETY: the views are alive; framebuffers referencing a view are retired with it.
        let f = create("vkCreateFramebuffer", |o| unsafe {
            vkCreateFramebuffer(self.device, &ci, null(), o)
        })?;
        self.framebuffers.insert(key, f);
        Ok(f)
    }

    fn vertex_input(
        &self,
        layout: Option<&VertexLayout>,
        shader: &Compiled,
    ) -> Result<Vec<VkVertexInputAttributeDescription>> {
        let Some(layout) = layout else {
            if let Some(input) = shader.inputs.first() {
                return Err(format!(
                    "the vertex shader reads @location({}) but the pipeline has no vertex layout",
                    input.location
                ));
            }
            return Ok(Vec::new());
        };
        if layout.stride == 0 || layout.stride > self.limits.maxVertexInputBindingStride {
            return Err(format!("invalid vertex stride {}", layout.stride));
        }
        for (i, a) in layout.attributes.iter().enumerate() {
            if layout.attributes[..i]
                .iter()
                .any(|b| b.location == a.location)
            {
                return Err(format!("vertex @location({}) is listed twice", a.location));
            }
            if a.offset > self.limits.maxVertexInputAttributeOffset
                || a.location >= self.limits.maxVertexInputAttributes
            {
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
        Ok(layout
            .attributes
            .iter()
            .map(|a| VkVertexInputAttributeDescription {
                location: a.location,
                binding: 0,
                format: vertex_format(a.format),
                offset: a.offset,
            })
            .collect())
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
        if d.colors.len() > (self.limits.maxColorAttachments as usize).min(MAX_PASS_COLORS) {
            return Err(format!(
                "{} colour targets exceed the device limit",
                d.colors.len()
            ));
        }
        for (i, c) in d.colors.iter().enumerate() {
            let features = self.format_features(vk_format(c.format));
            if c.format.is_depth() || features & FEATURE_COLOR == 0 {
                return Err(format!(
                    "colour target {i}: {:?} is not renderable",
                    c.format
                ));
            }
            if c.blend != Blend::Replace && features & FEATURE_BLEND == 0 {
                return Err(format!("colour target {i}: {:?} cannot blend", c.format));
            }
            let first = d.colors[0];
            if !self.features.independent_blend
                && (c.blend != first.blend || c.write_mask & 0xF != first.write_mask & 0xF)
            {
                return Err(format!(
                    "{} needs the same blend and write mask on every colour target",
                    self.name
                ));
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
        let attributes = self.vertex_input(d.vertex_layout.as_ref(), d.vertex)?;
        let mut stages = vec![(d.vertex, VERTEX_BIT)];
        if let Some(f) = d.fragment {
            stages.push((f, FRAGMENT_BIT));
        }
        let (groups, layout, push_size, push_stages) = self.layouts(&stages)?;
        let mut key = PassKey::empty();
        key.color_len = d.colors.len() as u8;
        for (i, c) in d.colors.iter().enumerate() {
            key.colors[i] = (vk_format(c.format), 2, 0);
        }
        key.depth = d.depth.map(|z| (vk_format(z.format), 2, 0));
        let pass = self.render_pass(&key)?;

        let binding = VkVertexInputBindingDescription {
            binding: 0,
            stride: d.vertex_layout.as_ref().map_or(0, |l| l.stride),
            inputRate: 0,
        };
        let mut vi = VkPipelineVertexInputStateCreateInfo::new();
        if d.vertex_layout.is_some() {
            vi.vertexBindingDescriptionCount = 1;
            vi.pVertexBindingDescriptions = &binding;
            vi.vertexAttributeDescriptionCount = attributes.len() as u32;
            vi.pVertexAttributeDescriptions = attributes.as_ptr();
        }
        let mut ia = VkPipelineInputAssemblyStateCreateInfo::new();
        ia.topology = match d.topology {
            Topology::TriangleList => 3,
            Topology::TriangleStrip => 4,
            Topology::LineList => 1,
        };
        let mut vp = VkPipelineViewportStateCreateInfo::new();
        vp.viewportCount = 1;
        vp.scissorCount = 1;
        let mut rs = VkPipelineRasterizationStateCreateInfo::new();
        rs.cullMode = match d.cull {
            Cull::None => 0,
            Cull::Front => 1,
            Cull::Back => 2,
        };
        rs.frontFace = if d.front_ccw { 0 } else { 1 };
        rs.lineWidth = 1.0;
        if let Some(z) = d.depth {
            rs.depthClampEnable = (z.clamp && self.features.depth_clamp) as u32;
            if z.bias_constant != 0.0 || z.bias_slope != 0.0 {
                rs.depthBiasEnable = 1;
                rs.depthBiasConstantFactor = z.bias_constant;
                rs.depthBiasSlopeFactor = z.bias_slope;
            }
        }
        let mut ms = VkPipelineMultisampleStateCreateInfo::new();
        ms.rasterizationSamples = 1;
        let mut ds = VkPipelineDepthStencilStateCreateInfo::new();
        if let Some(z) = d.depth {
            ds.depthTestEnable = 1;
            ds.depthWriteEnable = z.write as u32;
            ds.depthCompareOp = vk_compare(z.compare);
        }
        let blends: Vec<VkPipelineColorBlendAttachmentState> = d
            .colors
            .iter()
            .map(|c| {
                // (enable, src colour, dst colour, src alpha, dst alpha); op is always ADD.
                let (on, sc, dc, sa, da) = match c.blend {
                    Blend::Replace => (0, 1, 0, 1, 0),
                    Blend::Alpha => (1, 6, 7, 1, 7),
                    Blend::Premultiplied => (1, 1, 7, 1, 7),
                    Blend::Additive => (1, 1, 1, 1, 1),
                };
                VkPipelineColorBlendAttachmentState {
                    blendEnable: on,
                    srcColorBlendFactor: sc,
                    dstColorBlendFactor: dc,
                    colorBlendOp: 0,
                    srcAlphaBlendFactor: sa,
                    dstAlphaBlendFactor: da,
                    alphaBlendOp: 0,
                    colorWriteMask: u32::from(c.write_mask & 0xF),
                }
            })
            .collect();
        let mut cb = VkPipelineColorBlendStateCreateInfo::new();
        cb.attachmentCount = blends.len() as u32;
        cb.pAttachments = blends.as_ptr();
        let dynamic = [0, 1]; // VIEWPORT, SCISSOR
        let mut dy = VkPipelineDynamicStateCreateInfo::new();
        dy.dynamicStateCount = 2;
        dy.pDynamicStates = dynamic.as_ptr();

        let vs = self.shader_module(d.vertex)?;
        let fs = match d.fragment.map(|f| self.shader_module(f)).transpose() {
            Ok(fs) => fs,
            Err(e) => {
                // SAFETY: the module was created above and is not referenced elsewhere.
                unsafe { vkDestroyShaderModule(self.device, vs, null()) };
                return Err(e);
            }
        };
        let mut shader_stages = vec![shader_stage(VERTEX_BIT as i32, vs)];
        if let Some(fs) = fs {
            shader_stages.push(shader_stage(FRAGMENT_BIT as i32, fs));
        }
        let mut pi = VkGraphicsPipelineCreateInfo::new();
        pi.stageCount = shader_stages.len() as u32;
        pi.pStages = shader_stages.as_ptr();
        pi.pVertexInputState = &vi;
        pi.pInputAssemblyState = &ia;
        pi.pViewportState = &vp;
        pi.pRasterizationState = &rs;
        pi.pMultisampleState = &ms;
        if d.depth.is_some() {
            pi.pDepthStencilState = &ds;
        }
        if !blends.is_empty() {
            pi.pColorBlendState = &cb;
        }
        pi.pDynamicState = (&dy as *const VkPipelineDynamicStateCreateInfo).cast();
        pi.layout = layout;
        pi.renderPass = pass;
        // SAFETY: every state struct and array outlives the call. Shader modules may be
        // destroyed once the pipeline exists.
        let handle = unsafe {
            let result = create("vkCreateGraphicsPipelines", |o| {
                vkCreateGraphicsPipelines(self.device, self.pipeline_cache, 1, &pi, null(), o)
            });
            vkDestroyShaderModule(self.device, vs, null());
            if let Some(fs) = fs {
                vkDestroyShaderModule(self.device, fs, null());
            }
            result?
        };
        self.insert_pipeline(Pipeline {
            handle,
            layout,
            compute: false,
            groups,
            push_size,
            push_stages,
            colors: d.colors.iter().map(|c| c.format).collect(),
            depth: d.depth.map(|z| z.format),
            vertex_input: d.vertex_layout.is_some(),
        })
    }

    pub fn create_compute_pipeline(&mut self, shader: &Compiled) -> Result<PipelineId> {
        if shader.stage != Stage::Compute {
            return Err("create_compute_pipeline: not a compute shader".into());
        }
        let (groups, layout, push_size, push_stages) = self.layouts(&[(shader, COMPUTE_BIT)])?;
        let module = self.shader_module(shader)?;
        let mut ci = VkComputePipelineCreateInfo::new();
        ci.stage = shader_stage(COMPUTE_BIT as i32, module);
        ci.layout = layout;
        // SAFETY: the create info outlives the call; the module may be destroyed afterwards.
        let handle = unsafe {
            let result = create("vkCreateComputePipelines", |o| {
                vkCreateComputePipelines(self.device, self.pipeline_cache, 1, &ci, null(), o)
            });
            vkDestroyShaderModule(self.device, module, null());
            result?
        };
        self.insert_pipeline(Pipeline {
            handle,
            layout,
            compute: true,
            groups,
            push_size,
            push_stages,
            colors: Vec::new(),
            depth: None,
            vertex_input: false,
        })
    }

    fn insert_pipeline(&mut self, p: Pipeline) -> Result<PipelineId> {
        let handle = p.handle;
        match self.pipelines.insert(p) {
            Ok(raw) => Ok(PipelineId::from_raw(raw)),
            Err(e) => {
                // SAFETY: the pipeline was never handed out.
                unsafe { vkDestroyPipeline(self.device, handle, null()) };
                Err(e)
            }
        }
    }

    pub fn create_bind_group(
        &mut self,
        pipeline: PipelineId,
        group: u32,
        entries: &[(u32, Resource)],
    ) -> Result<BindGroupId> {
        for (i, (binding, _)) in entries.iter().enumerate() {
            if entries[..i].iter().any(|(b, _)| b == binding) {
                return Err(format!("@binding({binding}) is given twice"));
            }
        }
        // Reflection lists only what the entry points statically use, so entries for other
        // bindings are ignored, and a group the pipeline does not use gets an empty group.
        let mut inline_entries = [Entry {
            binding: 0,
            kind: BindingKind::Sampler { comparison: false },
            stages: 0,
        }; 32];
        let mut overflow_entries = Vec::new();
        let (layout_id, set_layout, entry_count) = {
            let p = self.pipeline(pipeline)?;
            match p
                .groups
                .get(group as usize)
                .filter(|g| !g.entries.is_empty())
            {
                Some(g) => {
                    if g.entries.len() <= inline_entries.len() {
                        inline_entries[..g.entries.len()].copy_from_slice(&g.entries);
                    } else {
                        overflow_entries.extend_from_slice(&g.entries);
                    }
                    (g.id, g.set_layout, g.entries.len())
                }
                None => (0, 0, 0),
            }
        };
        if entry_count == 0 {
            if group as usize >= MAX_BIND_GROUPS {
                return Err(format!("@group({group}) is above the supported 0..=7"));
            }
            let empty = BindGroup {
                set: 0,
                pool: usize::MAX,
                layout_id: 0,
                bound: Vec::new(),
                counts: [0; 5],
            };
            return Ok(BindGroupId::from_raw(self.groups.insert(empty)?));
        }
        let layout_entries = if entry_count <= inline_entries.len() {
            &inline_entries[..entry_count]
        } else {
            overflow_entries.as_slice()
        };
        let mut images = Vec::with_capacity(entry_count);
        let mut buffers = Vec::with_capacity(entry_count);
        let mut bound = Vec::with_capacity(entry_count);
        let mut counts = [0u32; 5];
        for e in layout_entries {
            let (_, res) = *entries
                .iter()
                .find(|(b, _)| *b == e.binding)
                .ok_or_else(|| format!("@group({group}) @binding({}) is missing", e.binding))?;
            let at = || format!("@group({group}) @binding({})", e.binding);
            counts[descriptor_slot(&e.kind)] += 1;
            let stages = shader_pipeline_stages(e.stages);
            match (e.kind, res) {
                (
                    BindingKind::UniformBuffer { .. } | BindingKind::StorageBuffer { .. },
                    Resource::Buffer { id, offset, size },
                ) => {
                    let b = self.buffer(id)?;
                    let uniform = matches!(e.kind, BindingKind::UniformBuffer { .. });
                    let (usage, align, max) = if uniform {
                        (
                            BufferUsage::UNIFORM,
                            self.limits.minUniformBufferOffsetAlignment,
                            self.limits.maxUniformBufferRange,
                        )
                    } else {
                        (
                            BufferUsage::STORAGE,
                            self.limits.minStorageBufferOffsetAlignment,
                            self.limits.maxStorageBufferRange,
                        )
                    };
                    if !b.desc.usage.contains(usage) {
                        return Err(format!("{}: {id:?} lacks {usage:?} usage", at()));
                    }
                    if offset >= b.desc.size || offset % align.max(1) != 0 {
                        return Err(format!(
                            "{}: offset {offset} must be inside the buffer and a multiple of {align}",
                            at()
                        ));
                    }
                    let range = if size == 0 {
                        b.desc.size - offset
                    } else {
                        size
                    };
                    if offset + range > b.desc.size || range > u64::from(max) {
                        return Err(format!(
                            "{}: range {range} at {offset} is out of bounds",
                            at()
                        ));
                    }
                    if let BindingKind::UniformBuffer { size: need } = e.kind {
                        if range < u64::from(need) {
                            return Err(format!(
                                "{}: the shader reads {need} bytes but only {range} are bound",
                                at()
                            ));
                        }
                    }
                    let access = match e.kind {
                        BindingKind::UniformBuffer { .. } => access::UNIFORM_READ,
                        BindingKind::StorageBuffer { read_only: true } => access::SHADER_READ,
                        _ => access::SHADER_READ | access::SHADER_WRITE,
                    };
                    buffers.push((
                        e.binding,
                        DESCRIPTOR_TYPES[descriptor_slot(&e.kind)],
                        VkDescriptorBufferInfo {
                            buffer: b.handle,
                            offset,
                            range,
                        },
                    ));
                    bound.push(Bound::Buffer {
                        id,
                        usage: Use::buffer(stages, access),
                    });
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
                                return Err(format!("{}: {id:?} lacks SAMPLED usage", at()));
                            }
                            let ok = match sample {
                                SampleType::Depth => desc.format.is_depth(),
                                SampleType::Float => true,
                                SampleType::Sint | SampleType::Uint => false,
                            };
                            if !ok {
                                return Err(format!(
                                    "{}: a {sample:?} texture binding cannot sample {:?}",
                                    at(),
                                    desc.format
                                ));
                            }
                        }
                        BindingKind::StorageTexture { format, .. } => {
                            if !desc.usage.contains(TextureUsage::STORAGE) {
                                return Err(format!("{}: {id:?} lacks STORAGE usage", at()));
                            }
                            if texel_format(format) != desc.format {
                                return Err(format!(
                                    "{}: the shader writes {format:?} but {id:?} is {:?}",
                                    at(),
                                    desc.format
                                ));
                            }
                        }
                        _ => unreachable!("matched texture kinds above"),
                    }
                    let (key, mips, layers) = view_for(view, res, &desc, storage)
                        .map_err(|m| format!("{}: {m}", at()))?;
                    let image_view = self.view(id, key)?;
                    let (layout, access) = if storage {
                        (track::GENERAL, access::SHADER_WRITE)
                    } else {
                        (track::SHADER_READ_ONLY, access::SHADER_READ)
                    };
                    images.push((
                        e.binding,
                        DESCRIPTOR_TYPES[descriptor_slot(&e.kind)],
                        VkDescriptorImageInfo {
                            sampler: 0,
                            imageView: image_view,
                            imageLayout: layout,
                        },
                    ));
                    bound.push(Bound::Texture {
                        id,
                        mips,
                        layers,
                        usage: Use::new(layout, stages, access),
                    });
                }
                (BindingKind::Sampler { comparison }, Resource::Sampler(id)) => {
                    let s = self.samplers.get(id.raw()).ok_or_else(|| {
                        format!("{}: {id:?} does not belong to this device", at())
                    })?;
                    if s.comparison != comparison {
                        return Err(format!(
                            "{}: the shader declares a {} but {id:?} is {}",
                            at(),
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
                    images.push((
                        e.binding,
                        0,
                        VkDescriptorImageInfo {
                            sampler: s.handle,
                            imageView: 0,
                            imageLayout: 0,
                        },
                    ));
                    bound.push(Bound::Sampler(id));
                }
                (kind, res) => return Err(format!("{} expects {kind:?} but got {res:?}", at())),
            }
        }
        let (pool, set) = self.allocate_set(set_layout, counts)?;
        let mut writes = Vec::with_capacity(images.len() + buffers.len());
        for (binding, ty, info) in &images {
            let mut w = VkWriteDescriptorSet::new();
            w.dstSet = set;
            w.dstBinding = *binding;
            w.descriptorCount = 1;
            w.descriptorType = *ty;
            w.pImageInfo = info;
            writes.push(w);
        }
        for (binding, ty, info) in &buffers {
            let mut w = VkWriteDescriptorSet::new();
            w.dstSet = set;
            w.dstBinding = *binding;
            w.descriptorCount = 1;
            w.descriptorType = *ty;
            w.pBufferInfo = info;
            writes.push(w);
        }
        // SAFETY: the set is new and unused by the GPU; every info outlives the call.
        unsafe {
            vkUpdateDescriptorSets(self.device, writes.len() as u32, writes.as_ptr(), 0, null());
        }
        let group = BindGroup {
            set,
            pool,
            layout_id,
            bound,
            counts,
        };
        match self.groups.insert(group) {
            Ok(raw) => Ok(BindGroupId::from_raw(raw)),
            Err(e) => {
                self.free(Garbage::Set(pool, set, counts));
                Err(e)
            }
        }
    }

    /// Allocates a set from a pool with enough remaining capacity, creating pools as needed.
    /// Capacity is tracked explicitly because Vulkan 1.0 does not report pool exhaustion.
    fn allocate_set(&mut self, layout: u64, counts: [u32; 5]) -> Result<(usize, u64)> {
        let fits = |p: &DescriptorPool| {
            p.sets > 0 && p.counts.iter().zip(counts).all(|(free, n)| *free >= n)
        };
        let mut index = self.descriptor_pools.iter().position(fits);
        for attempt in 0..2 {
            let i = match index {
                Some(i) => i,
                None => self.create_descriptor_pool(counts)?,
            };
            let mut ai = VkDescriptorSetAllocateInfo::new();
            ai.descriptorPool = self.descriptor_pools[i].handle;
            ai.descriptorSetCount = 1;
            ai.pSetLayouts = &layout;
            let mut set = 0;
            // SAFETY: the pool and layout are alive; the allocate info outlives the call.
            let rc = unsafe { vkAllocateDescriptorSets(self.device, &ai, &mut set) };
            if rc == 0 {
                let p = &mut self.descriptor_pools[i];
                p.sets -= 1;
                for (free, n) in p.counts.iter_mut().zip(counts) {
                    *free -= n;
                }
                return Ok((i, set));
            }
            // A fragmented pool: retry once in a fresh pool.
            if attempt == 1 || (rc != -12 && rc != -1_000_069_000) {
                return Err(format!("vkAllocateDescriptorSets: VkResult {rc}"));
            }
            index = None;
        }
        unreachable!("the loop returns on its second attempt")
    }

    fn create_descriptor_pool(&mut self, counts: [u32; 5]) -> Result<usize> {
        let mut capacity = POOL_COUNTS;
        for (c, n) in capacity.iter_mut().zip(counts) {
            *c = (*c).max(n);
        }
        let sizes: Vec<VkDescriptorPoolSize> = DESCRIPTOR_TYPES
            .iter()
            .zip(capacity)
            .map(|(&ty, count)| VkDescriptorPoolSize {
                r#type: ty,
                descriptorCount: count,
            })
            .collect();
        let mut ci = VkDescriptorPoolCreateInfo::new();
        ci.flags = 1; // FREE_DESCRIPTOR_SET
        ci.maxSets = POOL_SETS;
        ci.poolSizeCount = sizes.len() as u32;
        ci.pPoolSizes = sizes.as_ptr();
        // SAFETY: the sizes outlive the call; pools are destroyed in Drop.
        let handle = create("vkCreateDescriptorPool", |o| unsafe {
            vkCreateDescriptorPool(self.device, &ci, null(), o)
        })?;
        self.descriptor_pools.push(DescriptorPool {
            handle,
            sets: POOL_SETS,
            counts: capacity,
        });
        Ok(self.descriptor_pools.len() - 1)
    }
}

fn shader_stage(stage: i32, module: u64) -> VkPipelineShaderStageCreateInfo {
    let mut s = VkPipelineShaderStageCreateInfo::new();
    s.stage = stage;
    s.module = module;
    s.pName = c"main".as_ptr();
    s
}

/// A half-open range of mips or layers.
type Span = (u32, u32);

/// The view a binding of dimension `dim` needs for `res`, with the mip and layer ranges it
/// covers.
fn view_for(
    dim: TextureView,
    res: Resource,
    d: &TextureDesc,
    storage: bool,
) -> Result<(ViewKey, Span, Span)> {
    let (mips, layers) = match res {
        Resource::TextureLayer(_, l) if l < d.layers => ((0, d.mips), (l, l + 1)),
        Resource::TextureMip(_, m) if m < d.mips => ((m, m + 1), (0, d.layers)),
        Resource::Texture(_) => ((0, d.mips), (0, d.layers)),
        _ => return Err(format!("{res:?} is outside the texture")),
    };
    let layer_count = layers.1 - layers.0;
    let ty = match dim {
        TextureView::D2 if layer_count == 1 => 1,
        TextureView::D2 => {
            return Err(format!(
                "a 2D binding needs one layer but the texture has {layer_count}; bind \
                 Resource::TextureLayer"
            ));
        }
        TextureView::D2Array => 5,
        TextureView::Cube if layer_count == 6 && d.width == d.height => 3,
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
