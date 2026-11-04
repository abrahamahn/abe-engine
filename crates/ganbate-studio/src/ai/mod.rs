//! AI-Native Agentic IDE foundation (`M-AI.1..4`) for Ganbate Studio and `ganbate` CLI:
//! - [`catalog`] (`M-AI.2`): `.ganbate/catalog.idx` 1-line typed Brick signatures and `brick crystallize`.
//! - [`gbs`] (`M-AI.3`): `.gbs` relational spatial patch DSL (`snap`, `socket`, `ring`, `beside`, `cam frame`).
//! - [`verify`] (`M-AI.4`): `<100ms` spatial lint, screen-box projection, GPU-budget oracle, and 2×2 AOV contact sheet.
//! - [`mcp`] (`M-AI.1`): Warm JSON-RPC 2.0 MCP server bridging live `@selection` and Studio Undo/Redo transactions.

pub mod catalog;
pub mod gbs;
pub mod mcp;
pub mod verify;
pub mod world_graph;

use std::collections::BTreeMap;

use ganbate_core::Vec3;
use ganbate_core::json::Value;
use ganbate_core::world::Transform;
use ganbate_pipeline::project::{Kind, Project};

/// Axis-aligned bounding box in metres (right-handed, `+Y` up).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Aabb {
    pub const UNIT: Self = Self {
        min: [-0.5, -0.5, -0.5],
        max: [0.5, 0.5, 0.5],
    };

    pub const EMPTY: Self = Self {
        min: [f32::INFINITY; 3],
        max: [f32::NEG_INFINITY; 3],
    };

    pub fn is_valid(&self) -> bool {
        self.min[0] <= self.max[0]
            && self.min[1] <= self.max[1]
            && self.min[2] <= self.max[2]
            && self.min.iter().chain(&self.max).all(|v| v.is_finite())
    }

    pub fn include_point(&mut self, p: [f32; 3]) {
        for (i, &v) in p.iter().enumerate() {
            if v < self.min[i] {
                self.min[i] = v;
            }
            if v > self.max[i] {
                self.max[i] = v;
            }
        }
    }

    pub fn union(self, other: Self) -> Self {
        if !self.is_valid() {
            return other;
        }
        if !other.is_valid() {
            return self;
        }
        Self {
            min: [
                self.min[0].min(other.min[0]),
                self.min[1].min(other.min[1]),
                self.min[2].min(other.min[2]),
            ],
            max: [
                self.max[0].max(other.max[0]),
                self.max[1].max(other.max[1]),
                self.max[2].max(other.max[2]),
            ],
        }
    }

    pub fn center(&self) -> [f32; 3] {
        [
            0.5 * (self.min[0] + self.max[0]),
            0.5 * (self.min[1] + self.max[1]),
            0.5 * (self.min[2] + self.max[2]),
        ]
    }

    pub fn extents(&self) -> [f32; 3] {
        [
            (self.max[0] - self.min[0]).max(0.0),
            (self.max[1] - self.min[1]).max(0.0),
            (self.max[2] - self.min[2]).max(0.0),
        ]
    }

    pub fn corners(&self) -> [[f32; 3]; 8] {
        let [x0, y0, z0] = self.min;
        let [x1, y1, z1] = self.max;
        [
            [x0, y0, z0],
            [x1, y0, z0],
            [x0, y1, z0],
            [x1, y1, z0],
            [x0, y0, z1],
            [x1, y0, z1],
            [x0, y1, z1],
            [x1, y1, z1],
        ]
    }

    pub fn transformed(&self, xf: Transform) -> Self {
        if !self.is_valid() {
            return *self;
        }
        let mut out = Self::EMPTY;
        for c in self.corners() {
            let Vec3(x, y, z) = xf.point(Vec3(c[0], c[1], c[2]));
            out.include_point([x, y, z]);
        }
        out
    }

    /// Standard named socket position on an AABB.
    pub fn named_socket(&self, name: &str) -> Option<[f32; 3]> {
        let c = self.center();
        match name {
            "base_bottom" | "bottom" | "base" => Some([c[0], self.min[1], c[2]]),
            "top_center" | "top" | "mount" => Some([c[0], self.max[1], c[2]]),
            "center" | "origin" => Some(c),
            "front" => Some([c[0], c[1], self.min[2]]),
            "back" => Some([c[0], c[1], self.max[2]]),
            "left" => Some([self.min[0], c[1], c[2]]),
            "right" => Some([self.max[0], c[1], c[2]]),
            _ => None,
        }
    }

    pub fn to_json(&self) -> Value {
        Value::Object(BTreeMap::from([
            (
                "min".into(),
                Value::Array(self.min.iter().map(|v| Value::Number(*v as f64)).collect()),
            ),
            (
                "max".into(),
                Value::Array(self.max.iter().map(|v| Value::Number(*v as f64)).collect()),
            ),
        ]))
    }

    pub fn compact_sig(&self) -> String {
        format!(
            "[{:.2},{:.2},{:.2}..{:.2},{:.2},{:.2}]",
            self.min[0], self.min[1], self.min[2], self.max[0], self.max[1], self.max[2]
        )
    }
}

/// Live `@selection` context synchronized from an active Ganbate Studio tab.
#[derive(Clone, Debug, Default)]
pub struct SelectionContext {
    pub doc: Option<String>,
    pub entity_ids: Vec<String>,
    pub asset_id: Option<String>,
    pub camera: Option<Value>,
    pub cursor_world: Option<[f32; 3]>,
    pub workbench: Option<String>,
}

impl SelectionContext {
    pub fn from_message(msg: &Value) -> Self {
        let doc = msg
            .get("doc")
            .and_then(|v| v.string().ok())
            .filter(|s| !s.is_empty())
            .map(String::from);
        let entity_ids = msg
            .get("entities")
            .or_else(|| msg.get("entityIds"))
            .and_then(|v| v.array().ok())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.string().ok().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let asset_id = msg
            .get("assetId")
            .and_then(|v| v.string().ok())
            .filter(|s| !s.is_empty())
            .map(String::from);
        let camera = msg
            .get("camera")
            .filter(|v| !matches!(v, Value::Null))
            .cloned();
        let cursor_world = msg.get("cursorWorld").and_then(parse_vec3_f32);
        let workbench = msg
            .get("workbench")
            .and_then(|v| v.string().ok())
            .filter(|s| !s.is_empty())
            .map(String::from);
        Self {
            doc,
            entity_ids,
            asset_id,
            camera,
            cursor_world,
            workbench,
        }
    }

    pub fn to_json(&self) -> Value {
        Value::Object(BTreeMap::from([
            (
                "doc".into(),
                self.doc
                    .as_deref()
                    .map_or(Value::Null, |s| Value::String(s.into())),
            ),
            (
                "entityIds".into(),
                Value::Array(
                    self.entity_ids
                        .iter()
                        .map(|s| Value::String(s.clone()))
                        .collect(),
                ),
            ),
            (
                "assetId".into(),
                self.asset_id
                    .as_deref()
                    .map_or(Value::Null, |s| Value::String(s.into())),
            ),
            ("camera".into(), self.camera.clone().unwrap_or(Value::Null)),
            (
                "cursorWorld".into(),
                self.cursor_world.map_or(Value::Null, |v| {
                    Value::Array(v.iter().map(|x| Value::Number(*x as f64)).collect())
                }),
            ),
            (
                "workbench".into(),
                self.workbench
                    .as_deref()
                    .map_or(Value::Null, |s| Value::String(s.into())),
            ),
        ]))
    }
}

pub fn parse_vec3_f32(v: &Value) -> Option<[f32; 3]> {
    let arr = v.array().ok()?;
    if arr.len() != 3 {
        return None;
    }
    Some([
        arr[0].float().ok()?,
        arr[1].float().ok()?,
        arr[2].float().ok()?,
    ])
}

pub fn parse_quat_f32(v: &Value) -> Option<[f32; 4]> {
    let arr = v.array().ok()?;
    if arr.len() != 4 {
        return None;
    }
    Some([
        arr[0].float().ok()?,
        arr[1].float().ok()?,
        arr[2].float().ok()?,
        arr[3].float().ok()?,
    ])
}

pub fn entity_local_transform(entity: &Value) -> Transform {
    let comp = entity.get("components").and_then(|c| c.get("transform"));
    let t = comp
        .and_then(|c| c.get("translation"))
        .and_then(parse_vec3_f32)
        .unwrap_or([0.0, 0.0, 0.0]);
    let r = comp
        .and_then(|c| c.get("rotation"))
        .and_then(parse_quat_f32)
        .unwrap_or([0.0, 0.0, 0.0, 1.0]);
    let s = comp
        .and_then(|c| c.get("scale"))
        .and_then(parse_vec3_f32)
        .unwrap_or([1.0, 1.0, 1.0]);
    Transform::from_trs(t, r, s).unwrap_or(Transform::IDENTITY)
}

/// Computes right-handed orientation quaternion `[x, y, z, w]` looking from `eye` toward `target`
/// along local `-Z` with `+Y` world up.
pub fn quat_look_at(eye: [f32; 3], target: [f32; 3]) -> [f32; 4] {
    let dx = target[0] - eye[0];
    let dy = target[1] - eye[1];
    let dz = target[2] - eye[2];
    let len = (dx * dx + dy * dy + dz * dz).sqrt();
    if len < 1e-6 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    // Local +Z (`back`) points from target to eye because camera/spot looks along -Z.
    let mut back = [-dx / len, -dy / len, -dz / len];
    let up_ref = if back[1].abs() > 0.999 {
        [0.0, 0.0, 1.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let mut right = cross(up_ref, back);
    right = normalize(right);
    let up = normalize(cross(back, right));
    back = normalize(back);
    quat_from_basis(right, up, back)
}

pub fn quat_from_yaw_pitch_roll_deg(yaw_deg: f32, pitch_deg: f32, roll_deg: f32) -> [f32; 4] {
    let (hy, hp, hr) = (
        0.5 * yaw_deg.to_radians(),
        0.5 * pitch_deg.to_radians(),
        0.5 * roll_deg.to_radians(),
    );
    let (sy, cy) = hy.sin_cos();
    let (sp, cp) = hp.sin_cos();
    let (sr, cr) = hr.sin_cos();
    // Y (yaw) * X (pitch) * Z (roll)
    let x = cy * sp * cr + sy * cp * sr;
    let y = sy * cp * cr - cy * sp * sr;
    let z = cy * cp * sr - sy * sp * cr;
    let w = cy * cp * cr + sy * sp * sr;
    normalize_quat([x, y, z, w])
}

pub fn normalize_quat(q: [f32; 4]) -> [f32; 4] {
    let l = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if l < 1e-6 {
        [0.0, 0.0, 0.0, 1.0]
    } else {
        [q[0] / l, q[1] / l, q[2] / l, q[3] / l]
    }
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l < 1e-6 {
        [0.0, 0.0, 1.0]
    } else {
        [v[0] / l, v[1] / l, v[2] / l]
    }
}

fn quat_from_basis(right: [f32; 3], up: [f32; 3], back: [f32; 3]) -> [f32; 4] {
    let m00 = right[0];
    let m10 = right[1];
    let m20 = right[2];
    let m01 = up[0];
    let m11 = up[1];
    let m21 = up[2];
    let m02 = back[0];
    let m12 = back[1];
    let m22 = back[2];
    let trace = m00 + m11 + m22;
    let q = if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        [(m21 - m12) / s, (m02 - m20) / s, (m10 - m01) / s, 0.25 * s]
    } else if m00 > m11 && m00 > m22 {
        let s = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
        [0.25 * s, (m01 + m10) / s, (m02 + m20) / s, (m21 - m12) / s]
    } else if m11 > m22 {
        let s = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
        [(m01 + m10) / s, 0.25 * s, (m12 + m21) / s, (m02 - m20) / s]
    } else {
        let s = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
        [(m02 + m20) / s, (m12 + m21) / s, 0.25 * s, (m10 - m01) / s]
    };
    normalize_quat(q)
}

/// Reads geometry stats and local AABB from a project `.glb` model asset.
#[derive(Clone, Debug)]
pub struct ModelGeometry {
    pub aabb: Aabb,
    pub triangles: u64,
    pub vertices: u64,
    pub draw_primitives: u64,
    pub bytes: u64,
    pub sockets: BTreeMap<String, [f32; 3]>,
}

pub fn inspect_model_geometry(project: &Project, model_id: &str) -> Option<ModelGeometry> {
    let entry = project.entry(model_id)?;
    if entry.kind != Kind::Model {
        return None;
    }
    let path = project.root.join(&entry.path);
    let bytes = ganbate_assets::read_asset(&path).ok()?;
    let nw = ganbate_assets::glb::import_nodes(&bytes, false).ok()?;
    let mut world_xforms: Vec<Transform> = Vec::with_capacity(nw.nodes.len());
    let mut aabb = Aabb::EMPTY;
    let mut triangles = 0u64;
    let mut vertices = 0u64;
    let mut draw_primitives = 0u64;
    let mut sockets = BTreeMap::new();

    for node in &nw.nodes {
        let wf = match node.parent {
            Some(p) => world_xforms[p].compose(node.local),
            None => node.local,
        };
        world_xforms.push(wf);
        let lower = node.name.to_ascii_lowercase();
        if let Some(sock) = lower
            .strip_prefix("socket_")
            .or_else(|| lower.strip_prefix("mount_"))
        {
            let Vec3(x, y, z) = wf.point(Vec3(0.0, 0.0, 0.0));
            sockets.insert(sock.to_string(), [x, y, z]);
        }
        for mesh_idx in node.meshes.clone() {
            if let Some(mesh) = nw.world.meshes.get(mesh_idx as usize) {
                draw_primitives += 1;
                vertices += mesh.positions.len() as u64;
                triangles += (mesh.indices.len() / 3) as u64;
                for &pos in &mesh.positions {
                    let Vec3(x, y, z) = wf.point(pos);
                    aabb.include_point([x, y, z]);
                }
            }
        }
    }
    if !aabb.is_valid() {
        aabb = Aabb::UNIT;
    }
    for std_sock in ["base_bottom", "top_center", "center"] {
        if !sockets.contains_key(std_sock)
            && let Some(pos) = aabb.named_socket(std_sock)
        {
            sockets.insert(std_sock.to_string(), pos);
        }
    }
    Some(ModelGeometry {
        aabb,
        triangles,
        vertices,
        draw_primitives: draw_primitives.max(1),
        bytes: bytes.len() as u64,
        sockets,
    })
}
