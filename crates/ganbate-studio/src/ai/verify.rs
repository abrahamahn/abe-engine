//! `<100ms` Spatial, Screen-Box, Determinism & GPU-Budget Oracle (`M-AI.4`).

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use ganbate_core::Vec3;
use ganbate_core::digest::{hex, sha256};
use ganbate_core::json::{self, Value};
use ganbate_core::world::Transform;
use ganbate_pipeline::project::{Kind, Project};
use ganbate_scene::scene;
use ganbate_scene::schema::Registry;

use super::catalog::entity_local_aabb;
use super::{Aabb, SelectionContext, inspect_model_geometry, parse_vec3_f32};

#[derive(Clone, Debug)]
pub struct VerifyOptions {
    pub camera_ref: Option<String>,
    pub width: u32,
    pub height: u32,
    pub max_draws: u64,
    pub max_triangles: u64,
    pub max_vram_mb: u64,
    pub render_contact_sheet: bool,
}

impl Default for VerifyOptions {
    fn default() -> Self {
        Self {
            camera_ref: None,
            width: 320,
            height: 180,
            max_draws: 500,
            max_triangles: 500_000,
            max_vram_mb: 256,
            render_contact_sheet: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CameraView {
    pub name: String,
    pub source: String,
    pub eye: [f32; 3],
    pub target: [f32; 3],
    pub up: [f32; 3],
    pub fov_deg: f32,
    pub near: f32,
    pub aspect: f32,
    pub right: [f32; 3],
    pub cam_up: [f32; 3],
    pub forward: [f32; 3],
}

impl CameraView {
    #[allow(clippy::too_many_arguments)]
    pub fn from_look(
        name: String,
        source: String,
        eye: [f32; 3],
        target: [f32; 3],
        up_hint: [f32; 3],
        fov_deg: f32,
        near: f32,
        aspect: f32,
    ) -> Self {
        let mut forward = [target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]];
        let fl =
            (forward[0] * forward[0] + forward[1] * forward[1] + forward[2] * forward[2]).sqrt();
        if fl < 1e-6 {
            forward = [0.0, 0.0, -1.0];
        } else {
            forward = [forward[0] / fl, forward[1] / fl, forward[2] / fl];
        }
        let up_ref =
            if (forward[0] * up_hint[0] + forward[1] * up_hint[1] + forward[2] * up_hint[2]).abs()
                > 0.995
            {
                [0.0, 0.0, 1.0]
            } else {
                up_hint
            };
        let mut right = cross(forward, up_ref);
        right = norm(right);
        let cam_up = norm(cross(right, forward));
        Self {
            name,
            source,
            eye,
            target,
            up: cam_up,
            fov_deg: fov_deg.clamp(5.0, 160.0),
            near: near.max(0.01),
            aspect: aspect.max(0.1),
            right,
            cam_up,
            forward,
        }
    }

    /// Transforms world point into camera space (`+X` right, `+Y` up, `+Z` forward into screen so `z_forward > near` is visible).
    fn world_to_cam(&self, p: [f32; 3]) -> [f32; 3] {
        let d = [p[0] - self.eye[0], p[1] - self.eye[1], p[2] - self.eye[2]];
        [
            dot(d, self.right),
            dot(d, self.cam_up),
            dot(d, self.forward),
        ]
    }

    /// Projects a camera-space point (`z > 0`) to normalized screen `[u, v]` (`[0,0]` top-left, `[1,1]` bottom-right).
    fn cam_to_uv(&self, c: [f32; 3]) -> [f32; 2] {
        let tan_half_y = (0.5 * self.fov_deg.to_radians()).tan().max(1e-4);
        let tan_half_x = tan_half_y * self.aspect;
        let ndc_x = c[0] / (c[2] * tan_half_x);
        let ndc_y = c[1] / (c[2] * tan_half_y);
        [0.5 * (ndc_x + 1.0), 0.5 * (1.0 - ndc_y)]
    }
}

#[derive(Clone, Debug)]
pub struct ScreenBox {
    pub id: String,
    pub name: String,
    pub rect: [f32; 4],
    pub depth_m: f32,
    pub on_screen: bool,
    pub clipped: bool,
    pub occlusion_ratio: f32,
}

#[derive(Clone, Debug)]
pub struct SpatialLint {
    pub code: String,
    pub severity: String,
    pub entity: Option<String>,
    pub peer: Option<String>,
    pub metric: f32,
    pub message: String,
    pub fix_hint: Option<String>,
}

impl SpatialLint {
    pub fn to_json(&self) -> Value {
        let mut m = BTreeMap::from([
            ("code".into(), Value::String(self.code.clone())),
            ("severity".into(), Value::String(self.severity.clone())),
            (
                "metric".into(),
                Value::Number(((self.metric as f64) * 10_000.0).round() / 10_000.0),
            ),
            ("message".into(), Value::String(self.message.clone())),
        ]);
        if let Some(e) = &self.entity {
            m.insert("entity".into(), Value::String(e.clone()));
        }
        if let Some(p) = &self.peer {
            m.insert("peer".into(), Value::String(p.clone()));
        }
        if let Some(h) = &self.fix_hint {
            m.insert("fixHint".into(), Value::String(h.clone()));
        }
        Value::Object(m)
    }
}

#[derive(Clone, Debug)]
pub struct VerifyReport {
    pub ok: bool,
    pub elapsed_ms: f64,
    pub world_hash: String,
    pub schema_issues: Vec<(String, String)>,
    pub spatial_lints: Vec<SpatialLint>,
    pub screen_boxes: Vec<ScreenBox>,
    pub camera: CameraView,
    pub budget: BTreeMap<String, u64>,
    pub contact_sheet_png: Option<Vec<u8>>,
}

impl VerifyReport {
    pub fn to_json(&self, contact_sheet_path: Option<&str>) -> Value {
        let schema_arr = Value::Array(
            self.schema_issues
                .iter()
                .map(|(p, m)| {
                    Value::Object(BTreeMap::from([
                        ("path".into(), Value::String(p.clone())),
                        ("message".into(), Value::String(m.clone())),
                    ]))
                })
                .collect(),
        );
        let lints_arr = Value::Array(
            self.spatial_lints
                .iter()
                .map(SpatialLint::to_json)
                .collect(),
        );
        let boxes_arr = Value::Array(
            self.screen_boxes
                .iter()
                .map(|b| {
                    Value::Object(BTreeMap::from([
                        ("id".into(), Value::String(b.id.clone())),
                        ("name".into(), Value::String(b.name.clone())),
                        (
                            "rect".into(),
                            Value::Array(
                                b.rect
                                    .iter()
                                    .map(|x| {
                                        Value::Number(((*x as f64) * 10_000.0).round() / 10_000.0)
                                    })
                                    .collect(),
                            ),
                        ),
                        (
                            "depthM".into(),
                            Value::Number(((b.depth_m as f64) * 1_000.0).round() / 1_000.0),
                        ),
                        ("onScreen".into(), Value::Bool(b.on_screen)),
                        ("clipped".into(), Value::Bool(b.clipped)),
                        (
                            "occlusionRatio".into(),
                            Value::Number(((b.occlusion_ratio as f64) * 1_000.0).round() / 1_000.0),
                        ),
                    ]))
                })
                .collect(),
        );
        let cam_obj = Value::Object(BTreeMap::from([
            ("name".into(), Value::String(self.camera.name.clone())),
            ("source".into(), Value::String(self.camera.source.clone())),
            (
                "eye".into(),
                Value::Array(
                    self.camera
                        .eye
                        .iter()
                        .map(|v| Value::Number(*v as f64))
                        .collect(),
                ),
            ),
            (
                "target".into(),
                Value::Array(
                    self.camera
                        .target
                        .iter()
                        .map(|v| Value::Number(*v as f64))
                        .collect(),
                ),
            ),
            ("fov".into(), Value::Number(self.camera.fov_deg as f64)),
        ]));
        let budget_obj = Value::Object(
            self.budget
                .iter()
                .map(|(k, v)| (k.clone(), Value::Number(*v as f64)))
                .collect(),
        );

        let mut map = BTreeMap::from([
            ("passed".into(), Value::Bool(self.ok)),
            (
                "elapsedMs".into(),
                Value::Number((self.elapsed_ms * 100.0).round() / 100.0),
            ),
            ("worldHash".into(), Value::String(self.world_hash.clone())),
            ("schemaIssues".into(), schema_arr),
            ("spatialLints".into(), lints_arr),
            ("camera".into(), cam_obj),
            ("screenBoxes".into(), boxes_arr),
            ("budget".into(), budget_obj),
        ]);
        if let Some(p) = contact_sheet_path {
            map.insert("contactSheet".into(), Value::String(p.into()));
        }
        Value::Object(map)
    }
}

struct SolidItem {
    id: String,
    name: String,
    aabb: Aabb,
    xf: Transform,
    is_solid_prop: bool,
    ancestors: BTreeSet<String>,
    color: [f32; 3],
}

fn c_is_solid_collider(col: Option<&Value>) -> bool {
    col.is_some_and(|c| c.get("sensor") != Some(&Value::Bool(true)))
}

/// Runs the `<100ms` scene verification oracle.
pub fn verify_scene(
    project: &Project,
    registry: &Registry,
    root: &Value,
    selection: Option<&SelectionContext>,
    load_prefab: &dyn Fn(&str) -> Option<Value>,
    opts: &VerifyOptions,
) -> Result<VerifyReport, String> {
    let t0 = Instant::now();

    let schema_issues: Vec<(String, String)> =
        scene::check_with_prefabs(root, registry, load_prefab)
            .into_iter()
            .map(|i| (i.path, i.message))
            .collect();

    let resolved = scene::flatten(root, load_prefab)?;
    let xforms = scene::world_transforms(&resolved);

    // Deterministic world hash over resolved IDs, world transforms, and components.
    let mut hash_items = Vec::with_capacity(resolved.len());
    for (i, r) in resolved.iter().enumerate() {
        let m_arr: Vec<Value> = xforms[i]
            .0
            .iter()
            .map(|v| {
                let clean = if v.abs() < 1e-6 { 0.0 } else { *v as f64 };
                Value::Number((clean * 100_000.0).round() / 100_000.0)
            })
            .collect();
        hash_items.push(Value::Object(BTreeMap::from([
            ("id".into(), Value::String(r.id.clone())),
            ("matrix".into(), Value::Array(m_arr)),
            ("components".into(), Value::Object(r.components.clone())),
        ])));
    }
    let world_canonical = json::to_canonical(&Value::Array(hash_items))?;
    let world_hash = hex(&sha256(world_canonical.as_bytes()));

    // Parent map for ancestor lookup
    let mut parent_map: BTreeMap<String, String> = BTreeMap::new();
    for r in &resolved {
        if let Some(p) = &r.parent {
            parent_map.insert(r.id.clone(), p.clone());
        }
    }

    let mut model_cache: BTreeMap<String, Option<super::ModelGeometry>> = BTreeMap::new();
    let mut unique_models = BTreeSet::new();
    let mut unique_textures = BTreeSet::new();
    let mut draw_calls = 0u64;
    let mut triangles = 0u64;
    let mut vertices = 0u64;
    let mut lights = 0u64;
    let mut shadow_casters = 0u64;
    let mut model_bytes = 0u64;

    let mut solids: Vec<SolidItem> = Vec::new();
    let mut spatial_lints: Vec<SpatialLint> = Vec::new();
    let mut scene_bounds = Aabb::EMPTY;

    for (i, r) in resolved.iter().enumerate() {
        let lower_name = r.name.to_ascii_lowercase();
        if lower_name.starts_with("socket_") || lower_name.starts_with("mount_") {
            continue;
        }
        // Check degenerate local scale
        if let Some(sc) = r
            .components
            .get("transform")
            .and_then(|t| t.get("scale"))
            .and_then(parse_vec3_f32)
            && sc.iter().any(|v| !v.is_finite() || v.abs() <= 1e-4)
        {
            spatial_lints.push(SpatialLint {
                code: "degenerate-scale".into(),
                severity: "warning".into(),
                entity: Some(r.id.clone()),
                peer: None,
                metric: 0.0,
                message: format!("{} ({}) has near-zero or non-finite scale", r.name, r.id),
                fix_hint: Some(format!("set {} transform.scale = [1, 1, 1]", r.id)),
            });
        }

        if let Some(light) = r.components.get("light") {
            lights += 1;
            if light
                .get("shadow")
                .and_then(|v| v.float().ok())
                .unwrap_or(0.0)
                > 0.0
            {
                shadow_casters += 1;
            }
        }

        let has_camera = r.components.contains_key("camera");
        let has_env = r.components.contains_key("environment");
        let has_mesh = r.components.contains_key("mesh");
        let has_collider = c_is_solid_collider(r.components.get("collider"));
        let is_kinematic = r
            .components
            .get("rigid_body")
            .and_then(|rb| rb.get("kind"))
            .and_then(|v| v.string().ok())
            == Some("kinematic");

        if let Some(model_id) = r
            .components
            .get("mesh")
            .and_then(|m| m.get("model"))
            .and_then(|v| v.string().ok())
        {
            let geom = model_cache
                .entry(model_id.to_string())
                .or_insert_with(|| inspect_model_geometry(project, model_id));
            if let Some(g) = geom {
                draw_calls += g.draw_primitives;
                triangles += g.triangles;
                vertices += g.vertices;
                if unique_models.insert(model_id.to_string()) {
                    model_bytes += g.bytes;
                }
            } else {
                draw_calls += 1;
            }
        } else if has_collider {
            draw_calls += 1;
            triangles += 12;
            vertices += 24;
        }

        if has_camera
            || has_env
            || (!has_mesh && !has_collider && r.components.contains_key("light"))
        {
            continue;
        }

        let synth = Value::Object(BTreeMap::from([(
            "components".into(),
            Value::Object(r.components.clone()),
        )]));
        let local_box = entity_local_aabb(project, &synth);
        let world_box = local_box.transformed(xforms[i]);
        scene_bounds = scene_bounds.union(world_box);

        let mut ancestors = BTreeSet::new();
        let mut cur = parent_map.get(&r.id).cloned();
        while let Some(pid) = cur {
            if !ancestors.insert(pid.clone()) {
                break;
            }
            cur = parent_map.get(&pid).cloned();
        }

        // Deterministic pleasant clay tint per entity id
        let h = r.id.bytes().fold(0x811c9dc5u32, |acc, b| {
            (acc ^ (b as u32)).wrapping_mul(0x01000193)
        });
        let color = [
            0.35 + 0.45 * (((h & 0xff) as f32) / 255.0),
            0.35 + 0.45 * ((((h >> 8) & 0xff) as f32) / 255.0),
            0.35 + 0.45 * ((((h >> 16) & 0xff) as f32) / 255.0),
        ];

        solids.push(SolidItem {
            id: r.id.clone(),
            name: r.name.clone(),
            aabb: world_box,
            xf: xforms[i],
            is_solid_prop: (has_mesh || has_collider) && !is_kinematic,
            ancestors,
            color,
        });
    }

    for entry in project.entries() {
        if entry.kind == Kind::Texture {
            unique_textures.insert(entry.id.clone());
        }
    }

    // 1. Floating-prop check
    for (idx, item) in solids.iter().enumerate() {
        if !item.is_solid_prop {
            continue;
        }
        let min_y = item.aabb.min[1];
        if min_y <= 0.05 {
            continue; // Resting on or below ground plane y = 0
        }
        let mut nearest_support_y: f32 = 0.0;
        let mut supported = false;
        for (j, other) in solids.iter().enumerate() {
            if idx == j || item.ancestors.contains(&other.id) || other.ancestors.contains(&item.id)
            {
                continue;
            }
            let overlap_x = item.aabb.min[0].max(other.aabb.min[0])
                < item.aabb.max[0].min(other.aabb.max[0]) - 0.005;
            let overlap_z = item.aabb.min[2].max(other.aabb.min[2])
                < item.aabb.max[2].min(other.aabb.max[2]) - 0.005;
            if overlap_x && overlap_z {
                let top = other.aabb.max[1];
                if (min_y - top).abs() <= 0.06 {
                    supported = true;
                    break;
                }
                if top < min_y && top > nearest_support_y {
                    nearest_support_y = top;
                }
            }
        }
        if !supported {
            let gap = min_y - nearest_support_y;
            spatial_lints.push(SpatialLint {
                code: "floating-prop".into(),
                severity: "warning".into(),
                entity: Some(item.id.clone()),
                peer: None,
                metric: gap,
                message: format!(
                    "{} ({}) floats {:.2}m above support (min_y={:.2}m)",
                    item.name, item.id, gap, min_y
                ),
                fix_hint: Some(format!("place {} snap onto ground", item.id)),
            });
        }
    }

    // 2. Static AABB penetration check
    for i in 0..solids.len() {
        if !solids[i].is_solid_prop {
            continue;
        }
        for j in (i + 1)..solids.len() {
            if !solids[j].is_solid_prop {
                continue;
            }
            if solids[i].ancestors.contains(&solids[j].id)
                || solids[j].ancestors.contains(&solids[i].id)
            {
                continue;
            }
            let ox = solids[i].aabb.max[0].min(solids[j].aabb.max[0])
                - solids[i].aabb.min[0].max(solids[j].aabb.min[0]);
            let oy = solids[i].aabb.max[1].min(solids[j].aabb.max[1])
                - solids[i].aabb.min[1].max(solids[j].aabb.min[1]);
            let oz = solids[i].aabb.max[2].min(solids[j].aabb.max[2])
                - solids[i].aabb.min[2].max(solids[j].aabb.min[2]);
            if ox > 0.025 && oy > 0.025 && oz > 0.025 {
                let pen = ox.min(oy).min(oz);
                spatial_lints.push(SpatialLint {
                    code: "static-penetration".into(),
                    severity: "warning".into(),
                    entity: Some(solids[i].id.clone()),
                    peer: Some(solids[j].id.clone()),
                    metric: pen,
                    message: format!(
                        "{} and {} interpenetrate by {:.2}m",
                        solids[i].name, solids[j].name, pen
                    ),
                    fix_hint: Some(format!(
                        "place {} beside {} right",
                        solids[j].id, solids[i].id
                    )),
                });
            }
        }
    }

    // 3. Resolve camera view
    let aspect = (opts.width.max(1) as f32) / (opts.height.max(1) as f32);
    let camera = resolve_camera(
        &resolved,
        &xforms,
        opts.camera_ref.as_deref(),
        selection,
        scene_bounds,
        aspect,
    )?;

    // 4. Screen-Box Projection + Occlusion Ratios
    let mut raw_boxes: Vec<(usize, ScreenBox)> = Vec::new();
    for (idx, item) in solids.iter().enumerate() {
        if let Some(sb) = project_aabb(&camera, &item.id, &item.name, &item.aabb) {
            raw_boxes.push((idx, sb));
        }
    }

    // Compute 2D depth-ordered occlusion ratio on a 16x16 grid inside each on-screen box.
    for a in 0..raw_boxes.len() {
        if !raw_boxes[a].1.on_screen {
            continue;
        }
        let [u0, v0, u1, v1] = [
            raw_boxes[a].1.rect[0].clamp(0.0, 1.0),
            raw_boxes[a].1.rect[1].clamp(0.0, 1.0),
            raw_boxes[a].1.rect[2].clamp(0.0, 1.0),
            raw_boxes[a].1.rect[3].clamp(0.0, 1.0),
        ];
        if u1 - u0 < 1e-3 || v1 - v0 < 1e-3 {
            continue;
        }
        let my_depth = raw_boxes[a].1.depth_m;
        let my_id = raw_boxes[a].1.id.clone();
        let mut covered = 0u32;
        let grid = 8u32;
        for gy in 0..grid {
            let v = v0 + (gy as f32 + 0.5) / (grid as f32) * (v1 - v0);
            for gx in 0..grid {
                let u = u0 + (gx as f32 + 0.5) / (grid as f32) * (u1 - u0);
                let is_occluded = raw_boxes.iter().any(|(_, other)| {
                    other.id != my_id
                        && other.on_screen
                        && other.depth_m + 0.05 < my_depth
                        && u >= other.rect[0]
                        && u <= other.rect[2]
                        && v >= other.rect[1]
                        && v <= other.rect[3]
                });
                if is_occluded {
                    covered += 1;
                }
            }
        }
        raw_boxes[a].1.occlusion_ratio = (covered as f32) / ((grid * grid) as f32);
    }

    let selected_set: BTreeSet<&str> = selection
        .map(|s| s.entity_ids.iter().map(String::as_str).collect())
        .unwrap_or_default();
    for (_, sb) in &raw_boxes {
        if selected_set.contains(sb.id.as_str()) && sb.occlusion_ratio > 0.65 {
            spatial_lints.push(SpatialLint {
                code: "focal-occluded".into(),
                severity: "warning".into(),
                entity: Some(sb.id.clone()),
                peer: None,
                metric: sb.occlusion_ratio,
                message: format!(
                    "selected entity {} is {:.0}% occluded from camera {}",
                    sb.name,
                    sb.occlusion_ratio * 100.0,
                    camera.name
                ),
                fix_hint: Some(format!("cam frame {} {}", camera.name, sb.id)),
            });
        }
    }

    let screen_boxes: Vec<ScreenBox> = raw_boxes.into_iter().map(|(_, sb)| sb).collect();

    // 5. GPU & Memory Budget check
    let vram_bytes_est = model_bytes + (unique_textures.len() as u64) * 512 * 1024;
    if draw_calls > opts.max_draws {
        spatial_lints.push(SpatialLint {
            code: "budget-draws".into(),
            severity: "warning".into(),
            entity: None,
            peer: None,
            metric: draw_calls as f32,
            message: format!(
                "draw calls ({draw_calls}) exceed budget ({})",
                opts.max_draws
            ),
            fix_hint: None,
        });
    }
    if triangles > opts.max_triangles {
        spatial_lints.push(SpatialLint {
            code: "budget-triangles".into(),
            severity: "warning".into(),
            entity: None,
            peer: None,
            metric: triangles as f32,
            message: format!(
                "triangles ({triangles}) exceed budget ({})",
                opts.max_triangles
            ),
            fix_hint: None,
        });
    }
    if vram_bytes_est > opts.max_vram_mb * 1024 * 1024 {
        spatial_lints.push(SpatialLint {
            code: "budget-vram".into(),
            severity: "warning".into(),
            entity: None,
            peer: None,
            metric: (vram_bytes_est as f32) / (1024.0 * 1024.0),
            message: format!(
                "estimated VRAM ({:.1} MiB) exceeds budget ({} MiB)",
                (vram_bytes_est as f64) / (1024.0 * 1024.0),
                opts.max_vram_mb
            ),
            fix_hint: None,
        });
    }

    let budget = BTreeMap::from([
        ("entities".into(), resolved.len() as u64),
        ("drawCalls".into(), draw_calls),
        ("uniqueModels".into(), unique_models.len() as u64),
        ("triangles".into(), triangles),
        ("vertices".into(), vertices),
        ("lights".into(), lights),
        ("shadowCasters".into(), shadow_casters),
        ("vramBytesEst".into(), vram_bytes_est),
    ]);

    let contact_sheet_png = if opts.render_contact_sheet {
        Some(render_contact_sheet(
            opts.width.max(64),
            opts.height.max(64),
            &camera,
            &solids,
            &screen_boxes,
            &spatial_lints,
        )?)
    } else {
        None
    };

    let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let ok = schema_issues.is_empty() && !spatial_lints.iter().any(|l| l.severity == "error");

    Ok(VerifyReport {
        ok,
        elapsed_ms,
        world_hash,
        schema_issues,
        spatial_lints,
        screen_boxes,
        camera,
        budget,
        contact_sheet_png,
    })
}

fn resolve_camera(
    resolved: &[scene::Resolved],
    xforms: &[Transform],
    cam_ref: Option<&str>,
    selection: Option<&SelectionContext>,
    scene_bounds: Aabb,
    aspect: f32,
) -> Result<CameraView, String> {
    // 1. Explicit `--camera` ref
    if let Some(cref) = cam_ref {
        let idx = resolved
            .iter()
            .position(|r| r.id == cref || r.name == cref)
            .ok_or_else(|| format!("camera entity {cref:?} not found"))?;
        return Ok(camera_from_entity(
            &resolved[idx],
            xforms[idx],
            "explicit",
            aspect,
        ));
    }

    // 2. Live Studio active camera from `SelectionContext`
    if let Some(sel) = selection
        && let Some(cam_val) = &sel.camera
        && let (Some(eye), Some(target)) = (
            cam_val.get("eye").and_then(parse_vec3_f32),
            cam_val.get("target").and_then(parse_vec3_f32),
        )
    {
        let up = cam_val
            .get("up")
            .and_then(parse_vec3_f32)
            .unwrap_or([0.0, 1.0, 0.0]);
        let fov = cam_val
            .get("fov")
            .and_then(|v| v.float().ok())
            .unwrap_or(50.0);
        let name = cam_val
            .get("name")
            .and_then(|v| v.string().ok())
            .unwrap_or("studio")
            .to_string();
        return Ok(CameraView::from_look(
            name,
            "studio".into(),
            eye,
            target,
            up,
            fov,
            0.08,
            aspect,
        ));
    }

    // 3. First `camera` entity in scene
    if let Some((idx, r)) = resolved
        .iter()
        .enumerate()
        .find(|(_, r)| r.components.contains_key("camera"))
    {
        return Ok(camera_from_entity(r, xforms[idx], "scene", aspect));
    }

    // 4. Overview camera framing `scene_bounds`
    let bounds = if scene_bounds.is_valid() {
        scene_bounds
    } else {
        Aabb {
            min: [-2.0, 0.0, -2.0],
            max: [2.0, 2.0, 2.0],
        }
    };
    let center = bounds.center();
    let ext = bounds.extents();
    let radius = (0.5 * (ext[0] * ext[0] + ext[1] * ext[1] + ext[2] * ext[2]).sqrt()).max(1.5);
    let fov = 50.0f32;
    let dist = (radius * 1.45) / (0.5 * fov.to_radians()).sin();
    let eye = [
        center[0] + dist * 0.55,
        center[1] + dist * 0.45,
        center[2] + dist * 0.70,
    ];
    Ok(CameraView::from_look(
        "overview".into(),
        "overview".into(),
        eye,
        center,
        [0.0, 1.0, 0.0],
        fov,
        0.08,
        aspect,
    ))
}

fn camera_from_entity(r: &scene::Resolved, xf: Transform, source: &str, aspect: f32) -> CameraView {
    let Vec3(ex, ey, ez) = xf.point(Vec3(0.0, 0.0, 0.0));
    // Camera looks along its local -Z axis with local +Y up.
    let Vec3(tx, ty, tz) = xf.point(Vec3(0.0, 0.0, -1.0));
    let Vec3(ux, uy, uz) = xf.point(Vec3(0.0, 1.0, 0.0));
    let up = norm([ux - ex, uy - ey, uz - ez]);
    let cam = r.components.get("camera");
    let fov = cam
        .and_then(|c| c.get("fov"))
        .and_then(|v| v.float().ok())
        .unwrap_or(50.0);
    let near = cam
        .and_then(|c| c.get("near"))
        .and_then(|v| v.float().ok())
        .unwrap_or(0.08);
    CameraView::from_look(
        r.name.clone(),
        source.into(),
        [ex, ey, ez],
        [tx, ty, tz],
        up,
        fov,
        near,
        aspect,
    )
}

const BOX_EDGES: [(usize, usize); 12] = [
    (0, 1),
    (2, 3),
    (4, 5),
    (6, 7),
    (0, 2),
    (1, 3),
    (4, 6),
    (5, 7),
    (0, 4),
    (1, 5),
    (2, 6),
    (3, 7),
];

fn project_aabb(cam: &CameraView, id: &str, name: &str, aabb: &Aabb) -> Option<ScreenBox> {
    if !aabb.is_valid() {
        return None;
    }
    let corners_world = aabb.corners();
    let corners_cam: [[f32; 3]; 8] = corners_world.map(|p| cam.world_to_cam(p));

    let mut u_min = f32::INFINITY;
    let mut v_min = f32::INFINITY;
    let mut u_max = f32::NEG_INFINITY;
    let mut v_max = f32::NEG_INFINITY;
    let mut min_depth = f32::INFINITY;
    let mut any_in_front = false;
    let mut clipped_near = false;

    for c in &corners_cam {
        if c[2] >= cam.near {
            any_in_front = true;
            min_depth = min_depth.min(c[2]);
            let [u, v] = cam.cam_to_uv(*c);
            u_min = u_min.min(u);
            v_min = v_min.min(v);
            u_max = u_max.max(u);
            v_max = v_max.max(v);
        } else {
            clipped_near = true;
        }
    }

    if !any_in_front {
        return Some(ScreenBox {
            id: id.into(),
            name: name.into(),
            rect: [0.0, 0.0, 0.0, 0.0],
            depth_m: -1.0,
            on_screen: false,
            clipped: true,
            occlusion_ratio: 0.0,
        });
    }

    // Clip near-plane crossing edges so partially behind-camera boxes project accurately.
    if clipped_near {
        for &(a, b) in &BOX_EDGES {
            let (ca, cb) = (corners_cam[a], corners_cam[b]);
            if (ca[2] >= cam.near) != (cb[2] >= cam.near) {
                let denom = cb[2] - ca[2];
                if denom.abs() > 1e-6 {
                    let t = (cam.near - ca[2]) / denom;
                    let hit = [
                        ca[0] + t * (cb[0] - ca[0]),
                        ca[1] + t * (cb[1] - ca[1]),
                        cam.near,
                    ];
                    min_depth = min_depth.min(cam.near);
                    let [u, v] = cam.cam_to_uv(hit);
                    u_min = u_min.min(u);
                    v_min = v_min.min(v);
                    u_max = u_max.max(u);
                    v_max = v_max.max(v);
                }
            }
        }
    }

    let on_screen = u_max > 0.0 && u_min < 1.0 && v_max > 0.0 && v_min < 1.0;
    let clipped = clipped_near || u_min < 0.0 || v_min < 0.0 || u_max > 1.0 || v_max > 1.0;

    Some(ScreenBox {
        id: id.into(),
        name: name.into(),
        rect: [
            u_min.clamp(-2.0, 3.0),
            v_min.clamp(-2.0, 3.0),
            u_max.clamp(-2.0, 3.0),
            v_max.clamp(-2.0, 3.0),
        ],
        depth_m: min_depth,
        on_screen,
        clipped,
        occlusion_ratio: 0.0,
    })
}

/// Renders a fast pure-CPU 2×2 diagnostic AOV contact sheet PNG:
/// - Top-Left: Shaded Clay + Ground Plane
/// - Top-Right: World-Space Surface Normals
/// - Bottom-Left: Linearized Depth
/// - Bottom-Right: Screen-Box & Spatial Lint Overlay
fn render_contact_sheet(
    total_w: u32,
    total_h: u32,
    cam: &CameraView,
    solids: &[SolidItem],
    boxes: &[ScreenBox],
    lints: &[SpatialLint],
) -> Result<Vec<u8>, String> {
    let qw = (total_w / 2).max(32);
    let qh = (total_h / 2).max(32);
    let out_w = qw * 2;
    let out_h = qh * 2;
    let mut rgba = vec![0u8; (out_w * out_h * 4) as usize];

    let tan_half_y = (0.5 * cam.fov_deg.to_radians()).tan().max(1e-4);
    let tan_half_x = tan_half_y * cam.aspect;
    let sun_dir = norm([0.45, 0.78, 0.43]);

    let floating_ids: BTreeSet<&str> = lints
        .iter()
        .filter(|l| l.code == "floating-prop")
        .filter_map(|l| l.entity.as_deref())
        .collect();
    let pen_ids: BTreeSet<&str> = lints
        .iter()
        .filter(|l| l.code == "static-penetration")
        .flat_map(|l| [l.entity.as_deref(), l.peer.as_deref()])
        .flatten()
        .collect();

    let mut set_px = |x: u32, y: u32, c: [u8; 4]| {
        if x < out_w && y < out_h {
            let idx = ((y * out_w + x) * 4) as usize;
            rgba[idx..idx + 4].copy_from_slice(&c);
        }
    };

    // Raycast primary rays across one `qw × qh` tile and populate all 4 quadrants simultaneously!
    for py in 0..qh {
        let ndc_y = 1.0 - 2.0 * ((py as f32 + 0.5) / (qh as f32));
        for px in 0..qw {
            let ndc_x = 2.0 * ((px as f32 + 0.5) / (qw as f32)) - 1.0;
            let rd = norm([
                cam.forward[0]
                    + ndc_x * tan_half_x * cam.right[0]
                    + ndc_y * tan_half_y * cam.cam_up[0],
                cam.forward[1]
                    + ndc_x * tan_half_x * cam.right[1]
                    + ndc_y * tan_half_y * cam.cam_up[1],
                cam.forward[2]
                    + ndc_x * tan_half_x * cam.right[2]
                    + ndc_y * tan_half_y * cam.cam_up[2],
            ]);

            let mut best_t = f32::INFINITY;
            let mut hit_normal = [0.0f32, 1.0, 0.0];
            let mut hit_color = [0.11f32, 0.14, 0.20];
            let mut hit_any = false;

            // Ground plane y = 0
            if rd[1].abs() > 1e-5 {
                let tg = -cam.eye[1] / rd[1];
                if tg > cam.near && tg < 120.0 {
                    let gx = cam.eye[0] + tg * rd[0];
                    let gz = cam.eye[2] + tg * rd[2];
                    best_t = tg;
                    hit_any = true;
                    hit_normal = [0.0, 1.0, 0.0];
                    let checker = ((gx.floor() as i32) + (gz.floor() as i32)) & 1 == 0;
                    hit_color = if checker {
                        [0.18, 0.21, 0.26]
                    } else {
                        [0.14, 0.17, 0.22]
                    };
                }
            }

            // World AABB ray-slab intersection for every solid entity
            for item in solids {
                let _ = item.xf;
                if let Some((t_hit, n_hit)) = ray_aabb(cam.eye, rd, &item.aabb, cam.near, best_t) {
                    best_t = t_hit;
                    hit_normal = n_hit;
                    hit_color = item.color;
                    hit_any = true;
                }
            }

            // Q1: Shaded Clay (top-left)
            let clay_rgb = if hit_any {
                let ndotl = dot(hit_normal, sun_dir).max(0.0);
                let lit = 0.28 + 0.72 * ndotl;
                [
                    to_u8(hit_color[0] * lit),
                    to_u8(hit_color[1] * lit),
                    to_u8(hit_color[2] * lit),
                    255,
                ]
            } else {
                let sky = (0.5 * (rd[1] + 1.0)).clamp(0.0, 1.0);
                [
                    to_u8(0.08 + 0.14 * sky),
                    to_u8(0.11 + 0.20 * sky),
                    to_u8(0.18 + 0.28 * sky),
                    255,
                ]
            };
            set_px(px, py, clay_rgb);

            // Q2: World Normal AOV (top-right)
            let norm_rgb = if hit_any {
                [
                    to_u8(0.5 * (hit_normal[0] + 1.0)),
                    to_u8(0.5 * (hit_normal[1] + 1.0)),
                    to_u8(0.5 * (hit_normal[2] + 1.0)),
                    255,
                ]
            } else {
                [18, 22, 30, 255]
            };
            set_px(qw + px, py, norm_rgb);

            // Q3: Linearized Depth AOV (bottom-left)
            let depth_rgb = if hit_any {
                let d_norm = (1.0 - (best_t / 40.0).clamp(0.0, 1.0)).powf(0.8);
                let v = to_u8(d_norm);
                [
                    v,
                    (v as u16 * 220 / 255) as u8,
                    (60 + (v as u16 * 195 / 255)) as u8,
                    255,
                ]
            } else {
                [10, 12, 18, 255]
            };
            set_px(px, qh + py, depth_rgb);

            // Q4 base: darkened clay background for screen-box & lint overlay (bottom-right)
            set_px(
                qw + px,
                qh + py,
                [clay_rgb[0] / 2, clay_rgb[1] / 2, clay_rgb[2] / 2, 255],
            );
        }
    }

    // Draw screen-space AABB rectangles on Q4 (bottom-right)
    for sb in boxes {
        if !sb.on_screen {
            continue;
        }
        let color = if pen_ids.contains(sb.id.as_str()) {
            [255, 72, 72, 255] // Red: static penetration
        } else if floating_ids.contains(sb.id.as_str()) {
            [255, 196, 56, 255] // Amber: floating prop
        } else {
            [72, 218, 255, 255] // Cyan: clean entity screen box
        };
        let x0 = (sb.rect[0].clamp(0.0, 1.0) * ((qw - 1) as f32)).round() as u32;
        let y0 = (sb.rect[1].clamp(0.0, 1.0) * ((qh - 1) as f32)).round() as u32;
        let x1 = (sb.rect[2].clamp(0.0, 1.0) * ((qw - 1) as f32)).round() as u32;
        let y1 = (sb.rect[3].clamp(0.0, 1.0) * ((qh - 1) as f32)).round() as u32;
        for x in x0..=x1 {
            set_px(qw + x, qh + y0, color);
            set_px(qw + x, qh + y1, color);
        }
        for y in y0..=y1 {
            set_px(qw + x0, qh + y, color);
            set_px(qw + x1, qh + y, color);
        }
    }

    // Quadrant divider lines
    for x in 0..out_w {
        set_px(x, qh, [40, 52, 70, 255]);
    }
    for y in 0..out_h {
        set_px(qw, y, [40, 52, 70, 255]);
    }

    ganbate_assets::png::encode(out_w, out_h, &rgba)
}

fn ray_aabb(
    ro: [f32; 3],
    rd: [f32; 3],
    aabb: &Aabb,
    t_min_in: f32,
    t_max_in: f32,
) -> Option<(f32, [f32; 3])> {
    let mut tmin = t_min_in;
    let mut tmax = t_max_in;
    let mut hit_n = [0.0f32, 1.0, 0.0];

    for axis in 0..3 {
        if rd[axis].abs() < 1e-6 {
            if ro[axis] < aabb.min[axis] || ro[axis] > aabb.max[axis] {
                return None;
            }
        } else {
            let inv = 1.0 / rd[axis];
            let mut t1 = (aabb.min[axis] - ro[axis]) * inv;
            let mut t2 = (aabb.max[axis] - ro[axis]) * inv;
            let mut n = [0.0f32; 3];
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
                n[axis] = 1.0;
            } else {
                n[axis] = -1.0;
            }
            if t1 > tmin {
                tmin = t1;
                hit_n = n;
            }
            if t2 < tmax {
                tmax = t2;
            }
            if tmin > tmax {
                return None;
            }
        }
    }
    Some((tmin, hit_n))
}

fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(v: [f32; 3]) -> [f32; 3] {
    let l = dot(v, v).sqrt();
    if l < 1e-6 {
        [0.0, 1.0, 0.0]
    } else {
        [v[0] / l, v[1] / l, v[2] / l]
    }
}
