//! `.gbs` Relational Spatial Patch DSL (`M-AI.3`).
//!
//! Line-oriented, indentation-free spatial command language that compiles directly into validated,
//! undoable `ganbate_scene::doc::Op` batches with engine-solved spatial constraints (`snap`, `socket`,
//! `ring`, `beside`, `look_at`, `cam frame`).

use std::collections::{BTreeMap, BTreeSet};

use ganbate_core::Vec3;
use ganbate_core::id::{Ids, is_asset_id};
use ganbate_core::json::{self, Value};
use ganbate_core::world::Transform;
use ganbate_pipeline::project::{Kind, Project};
use ganbate_scene::doc::{Document, Op};
use ganbate_scene::scene;
use ganbate_scene::schema::Registry;

use super::catalog::{
    BrickParam, Catalog, build_catalog, compute_entities_bounds_and_sockets, entity_local_aabb,
    load_brick_document,
};
use super::{Aabb, SelectionContext, parse_vec3_f32, quat_from_yaw_pitch_roll_deg, quat_look_at};

#[derive(Clone, Debug)]
pub struct GbsOutcome {
    pub label: String,
    pub ops: Vec<Op>,
    pub undo: Vec<Op>,
    pub root: Value,
    pub spawned: Vec<(String, String)>,
    pub statements: usize,
}

impl GbsOutcome {
    pub fn spawned_json(&self) -> Value {
        Value::Array(
            self.spawned
                .iter()
                .map(|(id, name)| {
                    Value::Object(BTreeMap::from([
                        ("id".into(), Value::String(id.clone())),
                        ("name".into(), Value::String(name.clone())),
                    ]))
                })
                .collect(),
        )
    }
}

/// Tokenizes a single `.gbs` line while keeping `[...]` arrays, `{...}` objects, and `"..."`
/// strings intact, even inside `key=[1, 2, 3]` or `name="Harbor Lantern"`.
pub fn tokenize_line(line: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut chars = line.chars().peekable();
    let mut in_quotes = false;
    let mut bracket_depth = 0i32;
    let mut brace_depth = 0i32;

    while let Some(ch) = chars.next() {
        if in_quotes {
            cur.push(ch);
            if ch == '\\' {
                if let Some(next) = chars.next() {
                    cur.push(next);
                }
            } else if ch == '"' {
                in_quotes = false;
            }
            continue;
        }
        match ch {
            '#' if bracket_depth == 0 && brace_depth == 0 => break,
            '"' => {
                in_quotes = true;
                cur.push(ch);
            }
            '[' => {
                bracket_depth += 1;
                cur.push(ch);
            }
            ']' => {
                bracket_depth -= 1;
                if bracket_depth < 0 {
                    return Err("unmatched ']'".into());
                }
                cur.push(ch);
            }
            '{' => {
                brace_depth += 1;
                cur.push(ch);
            }
            '}' => {
                brace_depth -= 1;
                if brace_depth < 0 {
                    return Err("unmatched '}'".into());
                }
                cur.push(ch);
            }
            c if c.is_whitespace() && bracket_depth == 0 && brace_depth == 0 => {
                if !cur.is_empty() {
                    tokens.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(ch),
        }
    }
    if in_quotes {
        return Err("unterminated string literal".into());
    }
    if bracket_depth != 0 || brace_depth != 0 {
        return Err("unclosed bracket or brace".into());
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    Ok(tokens)
}

fn parse_scalar_or_json(raw: &str) -> Result<Value, String> {
    let trimmed = raw.trim();
    if let Ok(v) = json::parse(trimmed.as_bytes()) {
        return Ok(v);
    }
    // Bare identifier / enum string convenience (e.g. `face=in`, `kind=point`)
    Ok(Value::String(trimmed.to_string()))
}

fn unquote(raw: &str) -> String {
    if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
        if let Ok(Value::String(s)) = json::parse(raw.as_bytes()) {
            return s;
        }
        return raw[1..raw.len() - 1].to_string();
    }
    raw.to_string()
}

fn parse_kv_args(tokens: &[String]) -> Result<BTreeMap<String, Value>, String> {
    let mut out = BTreeMap::new();
    for tok in tokens {
        let (k, v) = tok
            .split_once('=')
            .ok_or_else(|| format!("expected key=value option, got {tok:?}"))?;
        out.insert(k.trim().to_string(), parse_scalar_or_json(v)?);
    }
    Ok(out)
}

fn vec3_val(v: [f32; 3]) -> Value {
    Value::Array(
        v.iter()
            .map(|x| {
                let clean = if x.abs() < 1e-6 { 0.0 } else { *x as f64 };
                Value::Number((clean * 1_000_000.0).round() / 1_000_000.0)
            })
            .collect(),
    )
}

fn quat_val(q: [f32; 4]) -> Value {
    Value::Array(
        q.iter()
            .map(|x| {
                let clean = if x.abs() < 1e-6 { 0.0 } else { *x as f64 };
                Value::Number((clean * 1_000_000.0).round() / 1_000_000.0)
            })
            .collect(),
    )
}

struct SolverCtx<'a> {
    project: &'a Project,
    registry: &'a Registry,
    catalog: Catalog,
    load_prefab: &'a dyn Fn(&str) -> Option<Value>,
    ids: &'a mut Ids,
    doc: Document,
    ops: Vec<Op>,
    aliases: BTreeMap<String, String>,
    last_this: Option<String>,
    selection: Vec<String>,
    spawned: Vec<(String, String)>,
}

impl<'a> SolverCtx<'a> {
    fn apply(&mut self, batch: Vec<Op>) -> Result<(), String> {
        if batch.is_empty() {
            return Ok(());
        }
        self.doc.apply_all(&batch)?;
        self.ops.extend(batch);
        Ok(())
    }

    fn resolve_targets(&self, token: &str) -> Result<Vec<String>, String> {
        let t = token.trim();
        if t == "@this" {
            let id = self
                .last_this
                .clone()
                .or_else(|| self.selection.first().cloned())
                .ok_or("@this is unbound (spawn an entity first or select one in Studio)")?;
            return Ok(vec![id]);
        }
        if t == "@selection" {
            if self.selection.is_empty() {
                if let Some(id) = &self.last_this {
                    return Ok(vec![id.clone()]);
                }
                return Err("@selection is empty".into());
            }
            return Ok(self.selection.clone());
        }
        let clean = t.strip_prefix('$').unwrap_or(t);
        if let Some(id) = self.aliases.get(clean) {
            return Ok(vec![id.clone()]);
        }
        let ents = scene::entities(self.doc.root());
        if ents.contains_key(clean) {
            return Ok(vec![clean.to_string()]);
        }
        let name_target = unquote(clean);
        let matches: Vec<String> = ents
            .iter()
            .filter(|(_, e)| e.get("name").and_then(|v| v.string().ok()) == Some(&name_target))
            .map(|(k, _)| k.clone())
            .collect();
        match matches.as_slice() {
            [only] => Ok(vec![only.clone()]),
            [] => Err(format!("entity {token:?} not found")),
            _ => Err(format!("entity name {token:?} is ambiguous")),
        }
    }

    fn resolve_single(&self, token: &str) -> Result<String, String> {
        let list = self.resolve_targets(token)?;
        list.into_iter()
            .next()
            .ok_or_else(|| format!("entity {token:?} not found"))
    }

    fn resolve_point_or_entity(&self, token: &str) -> Result<[f32; 3], String> {
        if token.starts_with('[') {
            let v = json::parse(token.as_bytes()).map_err(|e| format!("bad point {token}: {e}"))?;
            return parse_vec3_f32(&v).ok_or_else(|| format!("expected [x, y, z], got {token}"));
        }
        let id = self.resolve_single(token)?;
        let (wf, aabb) = self.entity_world_bounds(&id)?;
        let Vec3(ox, oy, oz) = wf.point(Vec3(0.0, 0.0, 0.0));
        if aabb.is_valid() {
            let c = aabb.center();
            Ok([c[0], oy, c[2]])
        } else {
            Ok([ox, oy, oz])
        }
    }

    fn flattened_world(&self) -> Result<(Vec<scene::Resolved>, Vec<Transform>), String> {
        let resolved = scene::flatten(self.doc.root(), self.load_prefab)?;
        let xforms = scene::world_transforms(&resolved);
        Ok((resolved, xforms))
    }

    /// Computes world transform and world-space AABB of an entity (including its subtree & expanded prefab children).
    fn entity_world_bounds(&self, entity_id: &str) -> Result<(Transform, Aabb), String> {
        let (resolved, xforms) = self.flattened_world()?;
        let mut idx_by_id = BTreeMap::new();
        for (i, r) in resolved.iter().enumerate() {
            idx_by_id.insert(r.id.clone(), i);
        }
        let &root_idx = idx_by_id
            .get(entity_id)
            .ok_or_else(|| format!("entity {entity_id} not in flattened scene"))?;
        let root_xf = xforms[root_idx];

        // Collect `entity_id` and all its descendants in flattened hierarchy.
        let mut in_sub = BTreeSet::from([entity_id.to_string()]);
        for r in &resolved {
            if let Some(p) = &r.parent
                && in_sub.contains(p)
            {
                in_sub.insert(r.id.clone());
            }
        }

        let mut aabb = Aabb::EMPTY;
        for (i, r) in resolved.iter().enumerate() {
            if !in_sub.contains(&r.id) {
                continue;
            }
            let lower = r.name.to_ascii_lowercase();
            if lower.starts_with("socket_") || lower.starts_with("mount_") {
                continue;
            }
            let synth = Value::Object(BTreeMap::from([(
                "components".into(),
                Value::Object(r.components.clone()),
            )]));
            let local_box = entity_local_aabb(self.project, &synth);
            aabb = aabb.union(local_box.transformed(xforms[i]));
        }
        if !aabb.is_valid() {
            aabb = (Aabb {
                min: [-0.25, 0.0, -0.25],
                max: [0.25, 0.5, 0.25],
            })
            .transformed(root_xf);
        }
        Ok((root_xf, aabb))
    }

    /// Resolves a named socket on `host_id` in world coordinates.
    fn resolve_world_socket(&self, host_id: &str, socket_name: &str) -> Result<[f32; 3], String> {
        let (resolved, xforms) = self.flattened_world()?;
        let mut in_sub = BTreeSet::from([host_id.to_string()]);
        for r in &resolved {
            if let Some(p) = &r.parent
                && in_sub.contains(p)
            {
                in_sub.insert(r.id.clone());
            }
        }
        let target_sock = socket_name.to_ascii_lowercase();
        // 1. Check explicit `socket_<name>` / `mount_<name>` child nodes in scene/prefab.
        for (i, r) in resolved.iter().enumerate() {
            if !in_sub.contains(&r.id) {
                continue;
            }
            let lower = r.name.to_ascii_lowercase();
            if let Some(s) = lower
                .strip_prefix("socket_")
                .or_else(|| lower.strip_prefix("mount_"))
                && s == target_sock
            {
                let Vec3(x, y, z) = xforms[i].point(Vec3(0.0, 0.0, 0.0));
                return Ok([x, y, z]);
            }
            // Check sockets embedded inside a `.glb` model on this entity.
            if let Some(model_id) = r
                .components
                .get("mesh")
                .and_then(|m| m.get("model"))
                .and_then(|v| v.string().ok())
                && let Some(geom) = super::inspect_model_geometry(self.project, model_id)
                && let Some(&local_sock) = geom.sockets.get(&target_sock)
            {
                let Vec3(x, y, z) =
                    xforms[i].point(Vec3(local_sock[0], local_sock[1], local_sock[2]));
                return Ok([x, y, z]);
            }
        }
        // 2. Fallback to standard AABB named sockets (`top_center`, `base_bottom`, `center`, `front`, `back`, `left`, `right`).
        let (_, host_aabb) = self.entity_world_bounds(host_id)?;
        host_aabb
            .named_socket(&target_sock)
            .ok_or_else(|| format!("socket {socket_name:?} not found on entity {host_id}"))
    }

    fn spawn_source(
        &mut self,
        alias: Option<&str>,
        source: &str,
        explicit_name: Option<&str>,
        parent: Option<&str>,
        params: &BTreeMap<String, Value>,
    ) -> Result<String, String> {
        let default_name = explicit_name
            .map(String::from)
            .or_else(|| alias.map(|a| a.trim_start_matches('$').to_string()))
            .unwrap_or_else(|| "Entity".to_string());

        // 1. Empty group / node
        if source == "empty" || source == "group" || source == "node" {
            let eid = self.ids.entity();
            let mut comps = BTreeMap::new();
            comps.insert(
                "transform".into(),
                Value::Object(BTreeMap::from([
                    ("translation".into(), vec3_val([0.0, 0.0, 0.0])),
                    ("rotation".into(), quat_val([0.0, 0.0, 0.0, 1.0])),
                    ("scale".into(), vec3_val([1.0, 1.0, 1.0])),
                ])),
            );
            let ops = scene::create(&eid, &default_name, parent, None, comps);
            self.apply(ops)?;
            self.bind_spawned(alias, &eid, &default_name);
            return Ok(eid);
        }

        // 2. Crystallized Brick (`brick:<id>` or catalog match of kind `brick`)
        if source.starts_with("brick:")
            || self
                .catalog
                .find(source)
                .is_some_and(|it| it.kind == "brick")
        {
            let brick_doc = load_brick_document(self.project, source)?;
            let Some(Value::Object(brick_ents)) = brick_doc.get("entities") else {
                return Err(format!("brick {source:?} has no entities"));
            };
            // Create a container root if brick has >1 roots so `@this` moves the whole brick cleanly!
            let brick_roots: Vec<&String> = brick_ents
                .iter()
                .filter(|(_, e)| matches!(e.get("parent"), None | Some(Value::Null)))
                .map(|(id, _)| id)
                .collect();

            let mut id_remap = BTreeMap::new();
            for old_id in brick_ents.keys() {
                id_remap.insert(old_id.clone(), self.ids.entity());
            }

            let root_id = if brick_roots.len() == 1 {
                id_remap[brick_roots[0]].clone()
            } else {
                let group_id = self.ids.entity();
                let mut comps = BTreeMap::new();
                comps.insert(
                    "transform".into(),
                    Value::Object(BTreeMap::from([
                        ("translation".into(), vec3_val([0.0, 0.0, 0.0])),
                        ("rotation".into(), quat_val([0.0, 0.0, 0.0, 1.0])),
                        ("scale".into(), vec3_val([1.0, 1.0, 1.0])),
                    ])),
                );
                self.apply(scene::create(&group_id, &default_name, parent, None, comps))?;
                group_id
            };

            // Emit brick entities in parent-before-child order.
            let mut emitted = BTreeSet::new();
            let mut progress = true;
            while progress && emitted.len() < brick_ents.len() {
                progress = false;
                for (old_id, ent) in brick_ents {
                    if emitted.contains(old_id) {
                        continue;
                    }
                    let old_parent = ent.get("parent").and_then(|v| v.string().ok());
                    if let Some(op) = old_parent
                        && !emitted.contains(op)
                    {
                        continue;
                    }
                    let new_id = id_remap[old_id].clone();
                    let new_parent = match old_parent {
                        Some(op) => Some(id_remap[op].clone()),
                        None if brick_roots.len() == 1 => parent.map(String::from),
                        None => Some(root_id.clone()),
                    };
                    let mut ent_obj = ent.clone();
                    if let Value::Object(ref mut m) = ent_obj {
                        if brick_roots.len() == 1 && old_parent.is_none() {
                            m.insert("name".into(), Value::String(default_name.clone()));
                        }
                        match new_parent {
                            Some(p) => {
                                m.insert("parent".into(), Value::String(p));
                            }
                            None => {
                                m.remove("parent");
                            }
                        }
                    }
                    self.apply(vec![Op::Set {
                        path: scene::path(&["entities", &new_id]),
                        value: ent_obj,
                    }])?;
                    emitted.insert(old_id.clone());
                    progress = true;
                }
            }

            // Apply brick parameter overrides if declared.
            if let Some(Value::Array(brick_params)) = brick_doc.get("params") {
                for p in brick_params {
                    if let Ok(bp) = BrickParam::from_json(p)
                        && let Some(override_val) = params.get(&bp.name)
                        && let Some(mapped_eid) = id_remap.get(&bp.target_entity)
                    {
                        let mut full_path = vec!["entities".to_string(), mapped_eid.clone()];
                        full_path.extend(bp.path);
                        self.apply(vec![Op::Set {
                            path: full_path,
                            value: override_val.clone(),
                        }])?;
                    }
                }
            }

            let _ = compute_entities_bounds_and_sockets(self.project, brick_ents);
            self.bind_spawned(alias, &root_id, &default_name);
            return Ok(root_id);
        }

        // 3. Prefab or Model asset (by catalog key, asset id, or relative path)
        let resolved_asset = if is_asset_id(source) {
            self.project.entry(source).cloned()
        } else if let Some(item) = self.catalog.find(source)
            && let Some(aid) = &item.id
        {
            self.project.entry(aid).cloned()
        } else {
            self.project.find_path(source).cloned()
        };

        let Some(entry) = resolved_asset else {
            return Err(format!(
                "spawn source {source:?} not found in catalog or project"
            ));
        };

        let eid = self.ids.entity();
        match entry.kind {
            Kind::Prefab => {
                let mut ent_map = BTreeMap::from([
                    ("name".to_string(), Value::String(default_name.clone())),
                    (
                        "components".to_string(),
                        Value::Object(BTreeMap::from([(
                            "transform".into(),
                            Value::Object(BTreeMap::from([
                                ("translation".into(), vec3_val([0.0, 0.0, 0.0])),
                                ("rotation".into(), quat_val([0.0, 0.0, 0.0, 1.0])),
                                ("scale".into(), vec3_val([1.0, 1.0, 1.0])),
                            ])),
                        )])),
                    ),
                    (
                        "prefab".to_string(),
                        Value::Object(BTreeMap::from([
                            ("asset".into(), Value::String(entry.id.clone())),
                            ("overrides".into(), Value::Object(BTreeMap::new())),
                        ])),
                    ),
                ]);
                if let Some(p) = parent {
                    ent_map.insert("parent".into(), Value::String(p.into()));
                }
                self.apply(vec![Op::Set {
                    path: scene::path(&["entities", &eid]),
                    value: Value::Object(ent_map),
                }])?;
            }
            Kind::Model => {
                let mut comps = BTreeMap::new();
                comps.insert(
                    "transform".into(),
                    Value::Object(BTreeMap::from([
                        ("translation".into(), vec3_val([0.0, 0.0, 0.0])),
                        ("rotation".into(), quat_val([0.0, 0.0, 0.0, 1.0])),
                        ("scale".into(), vec3_val([1.0, 1.0, 1.0])),
                    ])),
                );
                comps.insert(
                    "mesh".into(),
                    Value::Object(BTreeMap::from([(
                        "model".into(),
                        Value::String(entry.id.clone()),
                    )])),
                );
                self.apply(scene::create(&eid, &default_name, parent, None, comps))?;
            }
            other => {
                return Err(format!(
                    "asset {} ({}) cannot be spawned as a scene entity",
                    entry.path,
                    other.name()
                ));
            }
        }

        self.bind_spawned(alias, &eid, &default_name);
        Ok(eid)
    }

    fn bind_spawned(&mut self, alias: Option<&str>, eid: &str, name: &str) {
        if let Some(a) = alias {
            let clean = a.trim_start_matches('$');
            if !clean.is_empty() && clean != "_" {
                self.aliases.insert(clean.to_string(), eid.to_string());
            }
        }
        self.last_this = Some(eid.to_string());
        self.spawned.push((eid.to_string(), name.to_string()));
    }

    fn set_entity_world_translation(
        &mut self,
        eid: &str,
        target_world: [f32; 3],
    ) -> Result<(), String> {
        let cur_local = scene::entities(self.doc.root())
            .get(eid)
            .and_then(|e| e.get("components"))
            .and_then(|c| c.get("transform"))
            .and_then(|t| t.get("translation"))
            .and_then(parse_vec3_f32)
            .unwrap_or([0.0, 0.0, 0.0]);
        let (wf, _) = self.entity_world_bounds(eid)?;
        let Vec3(wx, wy, wz) = wf.point(Vec3(0.0, 0.0, 0.0));
        // If unparented (most spatial placements), target_world == local translation directly;
        // if parented under an unrotated parent, add world delta cleanly.
        let has_parent = scene::parent_of(self.doc.root(), eid).is_some();
        let next_local = if !has_parent {
            target_world
        } else {
            [
                cur_local[0] + (target_world[0] - wx),
                cur_local[1] + (target_world[1] - wy),
                cur_local[2] + (target_world[2] - wz),
            ]
        };
        self.apply(vec![scene::set_field(
            eid,
            "transform",
            &["translation"],
            vec3_val(next_local),
        )])
    }

    /// Shifts `eid` in world space so its `base_bottom` (`[center_x, min_y, center_z]`) lands on `anchor_world`.
    fn align_entity_base_to(&mut self, eid: &str, anchor_world: [f32; 3]) -> Result<(), String> {
        let (wf, aabb) = self.entity_world_bounds(eid)?;
        let Vec3(ox, oy, oz) = wf.point(Vec3(0.0, 0.0, 0.0));
        let base = aabb.named_socket("base_bottom").unwrap_or([ox, oy, oz]);
        let desired_origin = [
            ox + (anchor_world[0] - base[0]),
            oy + (anchor_world[1] - base[1]),
            oz + (anchor_world[2] - base[2]),
        ];
        self.set_entity_world_translation(eid, desired_origin)
    }

    fn snap_entity_onto(
        &mut self,
        eid: &str,
        surface_ref: &str,
        xz_override: Option<[f32; 2]>,
        clearance: f32,
    ) -> Result<(), String> {
        let (_, cur_aabb) = self.entity_world_bounds(eid)?;
        let cur_center = cur_aabb.center();
        let (target_x, surface_top_y, target_z) =
            if matches!(surface_ref, "ground" | "floor" | "origin")
                && self.resolve_single(surface_ref).is_err()
            {
                let x = xz_override.map_or(cur_center[0], |p| p[0]);
                let z = xz_override.map_or(cur_center[2], |p| p[1]);
                (x, 0.0, z)
            } else {
                let surf_id = self.resolve_single(surface_ref)?;
                let (_, surf_aabb) = self.entity_world_bounds(&surf_id)?;
                let sc = surf_aabb.center();
                let x = xz_override.map_or(sc[0], |p| p[0]);
                let z = xz_override.map_or(sc[2], |p| p[1]);
                (x, surf_aabb.max[1], z)
            };
        self.align_entity_base_to(eid, [target_x, surface_top_y + clearance, target_z])
    }
}

/// Executes a `.gbs` script against `root`, validating every operation against `registry` and `load_prefab`.
pub fn execute_gbs(
    project: &Project,
    registry: &Registry,
    root: &Value,
    script: &str,
    selection: Option<&SelectionContext>,
    load_prefab: &dyn Fn(&str) -> Option<Value>,
    ids: &mut Ids,
) -> Result<GbsOutcome, String> {
    let is_look = root.get("ganbate").is_none()
        && (root.get("shading").is_some()
            || root.get("sky").is_some()
            || root.get("post").is_some()
            || root.get("views").is_some());

    let catalog = build_catalog(project, registry, false)?;
    let doc = Document::new(root.clone())?;
    let sel_ids = selection.map(|s| s.entity_ids.clone()).unwrap_or_default();

    let mut ctx = SolverCtx {
        project,
        registry,
        catalog,
        load_prefab,
        ids,
        doc,
        ops: Vec::new(),
        aliases: BTreeMap::new(),
        last_this: sel_ids.first().cloned(),
        selection: sel_ids,
        spawned: Vec::new(),
    };

    let mut tx_label = String::from("GBS Patch");
    let mut statements = 0usize;

    for (idx, raw_line) in script.lines().enumerate() {
        let line_no = idx + 1;
        let tokens = tokenize_line(raw_line).map_err(|e| format!("line {line_no}: {e}"))?;
        if tokens.is_empty() {
            continue;
        }
        statements += 1;
        let step_res: Result<(), String> = (|| {
            match tokens[0].as_str() {
                "tx" => {
                    if tokens.len() < 2 {
                        return Err("tx requires a label".into());
                    }
                    tx_label = unquote(&tokens[1..].join(" "));
                    Ok(())
                }
                "look" if tokens.get(1).map(String::as_str) == Some("patch") => {
                    if tokens.len() < 4 {
                        return Err("usage: look patch <section.field> [=] <value>".into());
                    }
                    let path_parts: Vec<String> = tokens[2].split('.').map(String::from).collect();
                    let val_tok = if tokens[3] == "=" {
                        tokens.get(4).ok_or("missing value after '='")?
                    } else {
                        &tokens[3]
                    };
                    let val = parse_scalar_or_json(val_tok)?;
                    if is_look {
                        ctx.apply(vec![Op::Set {
                            path: path_parts,
                            value: val,
                        }])
                    } else {
                        Err("`look patch` requires an open Look document".into())
                    }
                }
                "spawn" => {
                    if is_look {
                        return Err("cannot spawn entities in a Look document".into());
                    }
                    if tokens.len() < 3 {
                        return Err(
                            "usage: spawn <alias> <prefab|brick|model|empty> [key=val ...]".into(),
                        );
                    }
                    let alias = &tokens[1];
                    let source = &tokens[2];
                    let mut opts = parse_kv_args(&tokens[3..])?;
                    let name_opt = opts
                        .remove("name")
                        .and_then(|v| v.string().ok().map(String::from));
                    let parent_id = match opts.remove("parent") {
                        Some(v) => {
                            let pref = v.string().map_err(|_| "parent must be an entity ref")?;
                            Some(ctx.resolve_single(pref)?)
                        }
                        None => None,
                    };
                    ctx.spawn_source(
                        Some(alias),
                        source,
                        name_opt.as_deref(),
                        parent_id.as_deref(),
                        &opts,
                    )?;
                    Ok(())
                }
                "place" => {
                    if tokens.len() < 4 {
                        return Err("usage: place <target> <at|snap|socket|beside> ...".into());
                    }
                    let targets = ctx.resolve_targets(&tokens[1])?;
                    let mode = tokens[2].as_str();
                    match mode {
                        "at" => {
                            let pt = ctx.resolve_point_or_entity(&tokens[3])?;
                            let opts = parse_kv_args(&tokens[4..])?;
                            apply_place_common(&mut ctx, &targets, Some(pt), &opts)?;
                        }
                        "snap" => {
                            let mut pos = 3;
                            if tokens.get(pos).map(String::as_str) == Some("onto") {
                                pos += 1;
                            }
                            let surf = tokens
                                .get(pos)
                                .ok_or("usage: place <target> snap onto <surface|ground>")?
                                .clone();
                            pos += 1;
                            let mut xz_override = None;
                            if tokens.get(pos).map(String::as_str) == Some("at") {
                                let arr_tok =
                                    tokens.get(pos + 1).ok_or("expected [x, z] after `at`")?;
                                let parsed = json::parse(arr_tok.as_bytes())
                                    .map_err(|e| format!("bad [x, z]: {e}"))?;
                                let arr = parsed.array()?;
                                if arr.len() == 2 {
                                    xz_override = Some([arr[0].float()?, arr[1].float()?]);
                                } else if arr.len() == 3 {
                                    xz_override = Some([arr[0].float()?, arr[2].float()?]);
                                } else {
                                    return Err("expected [x, z] or [x, y, z] after `at`".into());
                                }
                                pos += 2;
                            }
                            let opts = parse_kv_args(&tokens[pos..])?;
                            let clearance = opts
                                .get("clearance")
                                .and_then(|v| v.float().ok())
                                .unwrap_or(0.0);
                            for tid in &targets {
                                ctx.snap_entity_onto(tid, &surf, xz_override, clearance)?;
                            }
                            apply_place_common(&mut ctx, &targets, None, &opts)?;
                        }
                        "socket" => {
                            let sock_name = tokens
                                .get(3)
                                .ok_or("usage: place <target> socket <socket_name> on <host>")?
                                .clone();
                            let mut pos = 4;
                            if tokens.get(pos).map(String::as_str) == Some("on") {
                                pos += 1;
                            }
                            let host_tok = tokens
                                .get(pos)
                                .ok_or("usage: place <target> socket <socket_name> on <host>")?
                                .clone();
                            pos += 1;
                            let opts = parse_kv_args(&tokens[pos..])?;
                            let host_id = ctx.resolve_single(&host_tok)?;
                            let sock_world = ctx.resolve_world_socket(&host_id, &sock_name)?;
                            for tid in &targets {
                                ctx.align_entity_base_to(tid, sock_world)?;
                            }
                            apply_place_common(&mut ctx, &targets, None, &opts)?;
                        }
                        "beside" => {
                            let peer_tok = tokens
                                .get(3)
                                .ok_or("usage: place <target> beside <peer> <left|right|front|back|above|below>")?
                                .clone();
                            let dir = tokens.get(4).map(String::as_str).unwrap_or("right");
                            let opts = parse_kv_args(tokens.get(5..).unwrap_or(&[]))?;
                            let gap = opts.get("gap").and_then(|v| v.float().ok()).unwrap_or(0.1);
                            let peer_id = ctx.resolve_single(&peer_tok)?;
                            let (_, peer_aabb) = ctx.entity_world_bounds(&peer_id)?;
                            let pc = peer_aabb.center();
                            for tid in &targets {
                                let (_, t_aabb) = ctx.entity_world_bounds(tid)?;
                                let ext = t_aabb.extents();
                                let anchor = match dir {
                                    "left" | "-x" => [
                                        peer_aabb.min[0] - gap - 0.5 * ext[0],
                                        peer_aabb.min[1],
                                        pc[2],
                                    ],
                                    "right" | "+x" => [
                                        peer_aabb.max[0] + gap + 0.5 * ext[0],
                                        peer_aabb.min[1],
                                        pc[2],
                                    ],
                                    "front" | "-z" => [
                                        pc[0],
                                        peer_aabb.min[1],
                                        peer_aabb.min[2] - gap - 0.5 * ext[2],
                                    ],
                                    "back" | "+z" => [
                                        pc[0],
                                        peer_aabb.min[1],
                                        peer_aabb.max[2] + gap + 0.5 * ext[2],
                                    ],
                                    "above" | "+y" => [pc[0], peer_aabb.max[1] + gap, pc[2]],
                                    "below" | "-y" => {
                                        [pc[0], peer_aabb.min[1] - gap - ext[1], pc[2]]
                                    }
                                    other => {
                                        return Err(format!(
                                            "unknown beside direction {other:?}; expected left, right, front, back, above, below"
                                        ));
                                    }
                                };
                                ctx.align_entity_base_to(tid, anchor)?;
                            }
                            apply_place_common(&mut ctx, &targets, None, &opts)?;
                        }
                        other => {
                            return Err(format!(
                                "unknown place mode {other:?}; expected at, snap, socket, or beside"
                            ));
                        }
                    }
                    Ok(())
                }
                "ring" => {
                    if tokens.len() < 3 {
                        return Err(
                            "usage: ring <count|alias> <prefab|brick|model> [around <center|[x,y,z]>] [count=<n>] [radius=<r>] [face=in|out|tangent]"
                                .into(),
                        );
                    }
                    let (alias_opt, parsed_count, source, mut pos) =
                        if let Ok(n_count) = tokens[1].parse::<usize>() {
                            (None, Some(n_count), tokens[2].clone(), 3usize)
                        } else {
                            (Some(tokens[1].clone()), None, tokens[2].clone(), 3usize)
                        };
                    let center = if tokens.get(pos).map(String::as_str) == Some("around") {
                        let center_tok = tokens
                            .get(pos + 1)
                            .ok_or("ring requires a center ref or [x,y,z] after `around`")?
                            .clone();
                        pos += 2;
                        ctx.resolve_point_or_entity(&center_tok)?
                    } else if tokens.get(pos).is_some_and(|t| !t.contains('=')) {
                        let center_tok = tokens[pos].clone();
                        pos += 1;
                        ctx.resolve_point_or_entity(&center_tok)?
                    } else {
                        [0.0, 0.0, 0.0]
                    };
                    let mut opts = parse_kv_args(&tokens[pos..])?;
                    let count = match parsed_count {
                        Some(c) => c,
                        None => opts.remove("count").and_then(|v| v.unsigned().ok()).ok_or(
                            "ring requires a count (either `ring <count> ...` or `count=<n>`)",
                        )? as usize,
                    };
                    if count == 0 || count > 256 {
                        return Err("ring count must be 1..=256".into());
                    }
                    let radius = opts
                        .remove("radius")
                        .and_then(|v| v.float().ok())
                        .unwrap_or(2.0);
                    let phase_deg = opts
                        .remove("phase")
                        .and_then(|v| v.float().ok())
                        .unwrap_or(0.0);
                    let face = opts
                        .remove("face")
                        .and_then(|v| v.string().ok().map(String::from))
                        .unwrap_or_else(|| "in".into());
                    let snap_surf = opts
                        .remove("snap")
                        .and_then(|v| v.string().ok().map(String::from));
                    let base_name = opts
                        .remove("name")
                        .and_then(|v| v.string().ok().map(String::from))
                        .or_else(|| {
                            alias_opt
                                .as_ref()
                                .map(|a| a.trim_start_matches('$').to_string())
                        })
                        .unwrap_or_else(|| "RingItem".into());

                    let mut last_spawned = None;
                    for i in 0..count {
                        let angle = phase_deg.to_radians()
                            + (i as f32) * std::f32::consts::TAU / (count as f32);
                        let x = center[0] + radius * angle.cos();
                        let z = center[2] + radius * angle.sin();
                        let item_name = format!("{}_{}", base_name, i + 1);
                        let eid = ctx.spawn_source(None, &source, Some(&item_name), None, &opts)?;
                        if let Some(ref surf) = snap_surf {
                            ctx.snap_entity_onto(&eid, surf, Some([x, z]), 0.0)?;
                        } else {
                            ctx.align_entity_base_to(&eid, [x, center[1], z])?;
                        }
                        let (_, item_aabb) = ctx.entity_world_bounds(&eid)?;
                        let eye = [x, item_aabb.min[1], z];
                        let rot = match face.as_str() {
                            "in" => quat_look_at(eye, [center[0], eye[1], center[2]]),
                            "out" => quat_look_at([center[0], eye[1], center[2]], eye),
                            "tangent" => {
                                let tx = -angle.sin();
                                let tz = angle.cos();
                                quat_look_at(eye, [eye[0] + tx, eye[1], eye[2] + tz])
                            }
                            _ => [0.0, 0.0, 0.0, 1.0],
                        };
                        ctx.apply(vec![scene::set_field(
                            &eid,
                            "transform",
                            &["rotation"],
                            quat_val(rot),
                        )])?;
                        last_spawned = Some(eid);
                    }
                    if let Some(last) = last_spawned {
                        ctx.last_this = Some(last);
                    }
                    Ok(())
                }
                "attach" => {
                    if tokens.len() < 3 {
                        return Err(
                            "usage: attach <target> <component_kind> [field=val ...]".into()
                        );
                    }
                    let targets = ctx.resolve_targets(&tokens[1])?;
                    let comp_kind = &tokens[2];
                    if ctx.registry.get(comp_kind).is_none() {
                        return Err(format!("unknown component kind {comp_kind:?}"));
                    }
                    let fields = parse_kv_args(&tokens[3..])?;
                    for tid in &targets {
                        let exists = scene::entities(ctx.doc.root())
                            .get(tid)
                            .and_then(|e| e.get("components"))
                            .and_then(|c| c.get(comp_kind))
                            .is_some();
                        if !exists {
                            ctx.apply(vec![Op::Set {
                                path: scene::path(&["entities", tid, "components", comp_kind]),
                                value: Value::Object(BTreeMap::new()),
                            }])?;
                        }
                        for (fk, fv) in &fields {
                            let segs: Vec<&str> = fk.split('.').collect();
                            ctx.apply(vec![scene::set_field(tid, comp_kind, &segs, fv.clone())])?;
                        }
                    }
                    Ok(())
                }
                "move" => {
                    if tokens.len() < 3 {
                        return Err("usage: move <target> [by] [dx, dy, dz]".into());
                    }
                    let targets = ctx.resolve_targets(&tokens[1])?;
                    let delta_tok = if tokens[2] == "by" {
                        tokens.get(3).ok_or("missing [dx, dy, dz] after `by`")?
                    } else {
                        &tokens[2]
                    };
                    let dv = json::parse(delta_tok.as_bytes())
                        .ok()
                        .and_then(|v| parse_vec3_f32(&v))
                        .ok_or_else(|| format!("expected [dx, dy, dz], got {delta_tok:?}"))?;
                    for tid in &targets {
                        let cur = scene::entities(ctx.doc.root())
                            .get(tid)
                            .and_then(|e| e.get("components"))
                            .and_then(|c| c.get("transform"))
                            .and_then(|t| t.get("translation"))
                            .and_then(parse_vec3_f32)
                            .unwrap_or([0.0, 0.0, 0.0]);
                        ctx.apply(vec![scene::set_field(
                            tid,
                            "transform",
                            &["translation"],
                            vec3_val([cur[0] + dv[0], cur[1] + dv[1], cur[2] + dv[2]]),
                        )])?;
                    }
                    Ok(())
                }
                "rotate" => {
                    if tokens.len() < 3 {
                        return Err(
                            "usage: rotate <target> [yaw=<deg>] [pitch=<deg>] [roll=<deg>]".into(),
                        );
                    }
                    let targets = ctx.resolve_targets(&tokens[1])?;
                    let opts = parse_kv_args(&tokens[2..])?;
                    let yaw = opts.get("yaw").and_then(|v| v.float().ok()).unwrap_or(0.0);
                    let pitch = opts
                        .get("pitch")
                        .and_then(|v| v.float().ok())
                        .unwrap_or(0.0);
                    let roll = opts.get("roll").and_then(|v| v.float().ok()).unwrap_or(0.0);
                    let q = quat_from_yaw_pitch_roll_deg(yaw, pitch, roll);
                    for tid in &targets {
                        ctx.apply(vec![scene::set_field(
                            tid,
                            "transform",
                            &["rotation"],
                            quat_val(q),
                        )])?;
                    }
                    Ok(())
                }
                "look_at" => {
                    if tokens.len() < 3 {
                        return Err("usage: look_at <target> <focus_ref|[x,y,z]>".into());
                    }
                    let targets = ctx.resolve_targets(&tokens[1])?;
                    let focus = ctx.resolve_point_or_entity(&tokens[2])?;
                    for tid in &targets {
                        let (wf, _) = ctx.entity_world_bounds(tid)?;
                        let Vec3(ex, ey, ez) = wf.point(Vec3(0.0, 0.0, 0.0));
                        let q = quat_look_at([ex, ey, ez], focus);
                        ctx.apply(vec![scene::set_field(
                            tid,
                            "transform",
                            &["rotation"],
                            quat_val(q),
                        )])?;
                    }
                    Ok(())
                }
                "cam" if tokens.get(1).map(String::as_str) == Some("frame") => {
                    if tokens.len() < 3 {
                        return Err(
                            "usage: cam frame <camera_ref> <subject_ref|@selection|all> [azimuth=<deg>] [elevation=<deg>] [padding=<1.25>] [fov=<deg>]"
                                .into(),
                        );
                    }
                    // Support both `cam frame <camera_ref> <subject_ref> ...` AND `cam frame <subject_ref> [camera=<camera_ref>] ...`
                    let (cam_id, subj_tok, opts) =
                        if tokens.get(3).is_some_and(|t| !t.contains('=')) {
                            let cid = ctx.resolve_single(&tokens[2])?;
                            let st = tokens[3].clone();
                            let kv = parse_kv_args(&tokens[4..])?;
                            (cid, st, kv)
                        } else {
                            let st = tokens[2].clone();
                            let kv = parse_kv_args(&tokens[3..])?;
                            let cid = if let Some(cref) =
                                kv.get("camera").and_then(|v| v.string().ok())
                            {
                                ctx.resolve_single(cref)?
                            } else if let Some((existing_cam, _)) =
                                scene::entities(ctx.doc.root()).iter().find(|(_, e)| {
                                    e.get("components").and_then(|c| c.get("camera")).is_some()
                                })
                            {
                                existing_cam.clone()
                            } else {
                                ctx.spawn_source(
                                    Some("auto_cam"),
                                    "empty",
                                    Some("MainCamera"),
                                    None,
                                    &BTreeMap::new(),
                                )?
                            };
                            (cid, st, kv)
                        };
                    let subj_tok = subj_tok.as_str();
                    let subject_aabb = if subj_tok == "all" || subj_tok == "scene" {
                        let (resolved, xforms) = ctx.flattened_world()?;
                        let mut box_all = Aabb::EMPTY;
                        for (i, r) in resolved.iter().enumerate() {
                            if r.id == cam_id || r.components.contains_key("camera") {
                                continue;
                            }
                            let synth = Value::Object(BTreeMap::from([(
                                "components".into(),
                                Value::Object(r.components.clone()),
                            )]));
                            box_all = box_all.union(
                                entity_local_aabb(ctx.project, &synth).transformed(xforms[i]),
                            );
                        }
                        if box_all.is_valid() {
                            box_all
                        } else {
                            Aabb::UNIT
                        }
                    } else {
                        let subj_ids = ctx.resolve_targets(subj_tok)?;
                        let mut box_sub = Aabb::EMPTY;
                        for sid in &subj_ids {
                            let (_, b) = ctx.entity_world_bounds(sid)?;
                            box_sub = box_sub.union(b);
                        }
                        box_sub
                    };

                    let azimuth_deg = opts
                        .get("azimuth")
                        .or_else(|| opts.get("yaw"))
                        .and_then(|v| v.float().ok())
                        .unwrap_or(35.0);
                    let elevation_deg = opts
                        .get("elevation")
                        .or_else(|| opts.get("elev"))
                        .and_then(|v| v.float().ok())
                        .unwrap_or(25.0);
                    let padding = opts
                        .get("padding")
                        .and_then(|v| v.float().ok())
                        .unwrap_or(1.35)
                        .max(1.0);
                    let fov_deg = opts
                        .get("fov")
                        .and_then(|v| v.float().ok())
                        .or_else(|| {
                            scene::entities(ctx.doc.root())
                                .get(&cam_id)
                                .and_then(|e| e.get("components"))
                                .and_then(|c| c.get("camera"))
                                .and_then(|cam| cam.get("fov"))
                                .and_then(|v| v.float().ok())
                        })
                        .unwrap_or(50.0)
                        .clamp(10.0, 120.0);

                    let center = subject_aabb.center();
                    let ext = subject_aabb.extents();
                    let radius = (0.5
                        * (ext[0] * ext[0] + ext[1] * ext[1] + ext[2] * ext[2]).sqrt())
                    .max(0.5);
                    let half_fov = 0.5 * fov_deg.to_radians();
                    let dist = opts
                        .get("dist")
                        .or_else(|| opts.get("distance"))
                        .and_then(|v| v.float().ok())
                        .unwrap_or_else(|| (radius * padding) / half_fov.sin().max(0.15))
                        .max(0.2);

                    let az = azimuth_deg.to_radians();
                    let el = elevation_deg.to_radians();
                    let eye = [
                        center[0] + dist * el.cos() * az.sin(),
                        center[1] + dist * el.sin(),
                        center[2] + dist * el.cos() * az.cos(),
                    ];
                    let rot = quat_look_at(eye, center);

                    // Ensure entity has camera + transform components.
                    let has_cam = scene::entities(ctx.doc.root())
                        .get(&cam_id)
                        .and_then(|e| e.get("components"))
                        .and_then(|c| c.get("camera"))
                        .is_some();
                    if !has_cam {
                        ctx.apply(vec![Op::Set {
                            path: scene::path(&["entities", &cam_id, "components", "camera"]),
                            value: Value::Object(BTreeMap::from([(
                                "fov".into(),
                                Value::Number(fov_deg as f64),
                            )])),
                        }])?;
                    } else if opts.contains_key("fov") {
                        ctx.apply(vec![scene::set_field(
                            &cam_id,
                            "camera",
                            &["fov"],
                            Value::Number(fov_deg as f64),
                        )])?;
                    }
                    ctx.set_entity_world_translation(&cam_id, eye)?;
                    ctx.apply(vec![scene::set_field(
                        &cam_id,
                        "transform",
                        &["rotation"],
                        quat_val(rot),
                    )])?;
                    Ok(())
                }
                "set" => {
                    if tokens.len() < 4 {
                        return Err("usage: set <target> <component.field> [=] <value>".into());
                    }
                    let targets = ctx.resolve_targets(&tokens[1])?;
                    let field_spec = &tokens[2];
                    let val_tok = if tokens[3] == "=" {
                        tokens.get(4).ok_or("missing value after '='")?
                    } else {
                        &tokens[3]
                    };
                    let val = parse_scalar_or_json(val_tok)?;
                    for tid in &targets {
                        if field_spec == "name" {
                            ctx.apply(vec![Op::Set {
                                path: scene::path(&["entities", tid, "name"]),
                                value: val.clone(),
                            }])?;
                        } else {
                            let segs: Vec<&str> = field_spec.split('.').collect();
                            if segs.len() < 2 {
                                return Err(format!(
                                    "set path {field_spec:?} must be `name` or `<component>.<field>`"
                                ));
                            }
                            let comp = segs[0];
                            let has_comp = scene::entities(ctx.doc.root())
                                .get(tid)
                                .and_then(|e| e.get("components"))
                                .and_then(|c| c.get(comp))
                                .is_some();
                            if !has_comp {
                                ctx.apply(vec![Op::Set {
                                    path: scene::path(&["entities", tid, "components", comp]),
                                    value: Value::Object(BTreeMap::new()),
                                }])?;
                            }
                            ctx.apply(vec![scene::set_field(tid, comp, &segs[1..], val.clone())])?;
                        }
                    }
                    Ok(())
                }
                "remove" => {
                    if tokens.len() < 3 {
                        return Err("usage: remove <target> <component[.field]>".into());
                    }
                    let targets = ctx.resolve_targets(&tokens[1])?;
                    let segs: Vec<&str> = tokens[2].split('.').collect();
                    for tid in &targets {
                        let mut p = vec![
                            "entities".to_string(),
                            tid.clone(),
                            "components".to_string(),
                        ];
                        p.extend(segs.iter().map(|s| (*s).to_string()));
                        ctx.apply(vec![Op::Remove { path: p }])?;
                    }
                    Ok(())
                }
                "delete" => {
                    if tokens.len() < 2 {
                        return Err("usage: delete <target>".into());
                    }
                    let targets = ctx.resolve_targets(&tokens[1])?;
                    let refs: Vec<&str> = targets.iter().map(String::as_str).collect();
                    let del_ops = scene::delete(ctx.doc.root(), &refs, ctx.load_prefab);
                    ctx.apply(del_ops)?;
                    Ok(())
                }
                "reparent" => {
                    if tokens.len() < 3 {
                        return Err("usage: reparent <target> <parent_ref|root>".into());
                    }
                    let targets = ctx.resolve_targets(&tokens[1])?;
                    let parent_id = if matches!(tokens[2].as_str(), "root" | "null" | "none") {
                        None
                    } else {
                        Some(ctx.resolve_single(&tokens[2])?)
                    };
                    for tid in &targets {
                        let rep_ops = scene::reparent(
                            ctx.doc.root(),
                            tid,
                            parent_id.as_deref(),
                            None,
                            ctx.load_prefab,
                        )?;
                        ctx.apply(rep_ops)?;
                    }
                    Ok(())
                }
                other => Err(format!("unknown .gbs command {other:?}")),
            }
        })();

        if let Err(e) = step_res {
            return Err(format!("line {line_no}: {e}"));
        }
    }

    if ctx.ops.is_empty() {
        return Err("script produced no document operations".into());
    }

    // Re-apply cleanly from original `root` in one batch so `undo` is the exact minimal inverse
    // and validate that no new schema/prefab errors were introduced!
    let mut clean_doc = Document::new(root.clone())?;
    let undo = clean_doc.apply_all(&ctx.ops)?;

    if !is_look {
        let before_issues: BTreeSet<(String, String)> =
            scene::check_with_prefabs(root, registry, load_prefab)
                .into_iter()
                .map(|i| (i.path, i.message))
                .collect();
        let after_issues: BTreeSet<(String, String)> =
            scene::check_with_prefabs(clean_doc.root(), registry, load_prefab)
                .into_iter()
                .map(|i| (i.path, i.message))
                .collect();
        if let Some((p, m)) = after_issues.difference(&before_issues).next() {
            return Err(format!("validation failed at {p}: {m}"));
        }
    } else {
        ganbate_engine::render::LookFile::from_value(clean_doc.root())?;
    }

    Ok(GbsOutcome {
        label: tx_label,
        ops: ctx.ops,
        undo,
        root: clean_doc.root().clone(),
        spawned: ctx.spawned,
        statements,
    })
}

fn apply_place_common(
    ctx: &mut SolverCtx<'_>,
    targets: &[String],
    explicit_pos: Option<[f32; 3]>,
    opts: &BTreeMap<String, Value>,
) -> Result<(), String> {
    let offset = opts
        .get("offset")
        .and_then(parse_vec3_f32)
        .unwrap_or([0.0, 0.0, 0.0]);
    for tid in targets {
        if let Some(pos) = explicit_pos {
            ctx.set_entity_world_translation(
                tid,
                [pos[0] + offset[0], pos[1] + offset[1], pos[2] + offset[2]],
            )?;
        } else if offset != [0.0, 0.0, 0.0] {
            let (wf, _) = ctx.entity_world_bounds(tid)?;
            let Vec3(wx, wy, wz) = wf.point(Vec3(0.0, 0.0, 0.0));
            ctx.set_entity_world_translation(
                tid,
                [wx + offset[0], wy + offset[1], wz + offset[2]],
            )?;
        }
        if let Some(yaw) = opts.get("yaw").and_then(|v| v.float().ok()) {
            let pitch = opts
                .get("pitch")
                .and_then(|v| v.float().ok())
                .unwrap_or(0.0);
            let roll = opts.get("roll").and_then(|v| v.float().ok()).unwrap_or(0.0);
            ctx.apply(vec![scene::set_field(
                tid,
                "transform",
                &["rotation"],
                quat_val(quat_from_yaw_pitch_roll_deg(yaw, pitch, roll)),
            )])?;
        }
        if let Some(scale_val) = opts.get("scale") {
            let sv = if let Ok(uniform) = scale_val.float() {
                [uniform, uniform, uniform]
            } else {
                parse_vec3_f32(scale_val).ok_or("scale must be a number or [sx, sy, sz]")?
            };
            ctx.apply(vec![scene::set_field(
                tid,
                "transform",
                &["scale"],
                vec3_val(sv),
            )])?;
        }
    }
    Ok(())
}
