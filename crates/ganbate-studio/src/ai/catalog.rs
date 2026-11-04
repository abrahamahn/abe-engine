//! Token-budgeted Typed Brick Catalog (`.ganbate/catalog.idx`) and `brick crystallize` (`M-AI.2`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use ganbate_core::Vec3;
use ganbate_core::json::{self, Value};
use ganbate_core::world::Transform;
use ganbate_pipeline::project::{Kind, Project};
use ganbate_scene::schema::{FieldType, Registry};

use super::{Aabb, entity_local_transform, inspect_model_geometry, parse_vec3_f32};

pub const CATALOG_REL_PATH: &str = ".ganbate/catalog.idx";
pub const BRICKS_REL_DIR: &str = ".ganbate/bricks";

#[derive(Clone, Debug)]
pub struct BrickParam {
    pub name: String,
    pub target_entity: String,
    pub path: Vec<String>,
    pub ty: String,
    pub default: Value,
}

impl BrickParam {
    pub fn to_json(&self) -> Value {
        Value::Object(BTreeMap::from([
            ("name".into(), Value::String(self.name.clone())),
            (
                "targetEntity".into(),
                Value::String(self.target_entity.clone()),
            ),
            (
                "path".into(),
                Value::Array(self.path.iter().map(|s| Value::String(s.clone())).collect()),
            ),
            ("type".into(), Value::String(self.ty.clone())),
            ("default".into(), self.default.clone()),
        ]))
    }

    pub fn from_json(v: &Value) -> Result<Self, String> {
        let name = v
            .get("name")
            .and_then(|x| x.string().ok())
            .ok_or("brick param missing name")?
            .to_string();
        let target_entity = v
            .get("targetEntity")
            .and_then(|x| x.string().ok())
            .ok_or("brick param missing targetEntity")?
            .to_string();
        let path = v
            .get("path")
            .and_then(|x| x.array().ok())
            .ok_or("brick param missing path")?
            .iter()
            .map(|s| s.string().map(String::from))
            .collect::<Result<Vec<_>, _>>()?;
        let ty = v
            .get("type")
            .and_then(|x| x.string().ok())
            .unwrap_or("value")
            .to_string();
        let default = v.get("default").cloned().unwrap_or(Value::Null);
        Ok(Self {
            name,
            target_entity,
            path,
            ty,
            default,
        })
    }
}

#[derive(Clone, Debug)]
pub struct CatalogItem {
    pub key: String,
    pub kind: String,
    pub id: Option<String>,
    pub path: Option<String>,
    pub aabb: Option<Aabb>,
    pub sockets: BTreeMap<String, [f32; 3]>,
    pub params: Vec<(String, String, Option<String>)>,
    pub signature: String,
}

impl CatalogItem {
    pub fn to_json(&self) -> Value {
        let mut map = BTreeMap::from([
            ("key".into(), Value::String(self.key.clone())),
            ("kind".into(), Value::String(self.kind.clone())),
            ("signature".into(), Value::String(self.signature.clone())),
        ]);
        if let Some(id) = &self.id {
            map.insert("id".into(), Value::String(id.clone()));
        }
        if let Some(path) = &self.path {
            map.insert("path".into(), Value::String(path.clone()));
        }
        if let Some(aabb) = &self.aabb {
            map.insert("aabb".into(), aabb.to_json());
        }
        if !self.sockets.is_empty() {
            let socks = self
                .sockets
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        Value::Array(v.iter().map(|n| Value::Number(*n as f64)).collect()),
                    )
                })
                .collect();
            map.insert("sockets".into(), Value::Object(socks));
        }
        if !self.params.is_empty() {
            let arr = self
                .params
                .iter()
                .map(|(n, ty, def)| {
                    let mut pm = BTreeMap::from([
                        ("name".into(), Value::String(n.clone())),
                        ("type".into(), Value::String(ty.clone())),
                    ]);
                    if let Some(d) = def {
                        pm.insert("default".into(), Value::String(d.clone()));
                    }
                    Value::Object(pm)
                })
                .collect();
            map.insert("params".into(), Value::Array(arr));
        }
        Value::Object(map)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Catalog {
    pub items: Vec<CatalogItem>,
}

impl Catalog {
    pub fn index_text(&self) -> String {
        let mut out = String::from("# ganbate catalog v1\n");
        for item in &self.items {
            out.push_str(&item.signature);
            out.push('\n');
        }
        out
    }

    pub fn search(
        &self,
        query: Option<&str>,
        kind_filter: Option<&str>,
        limit: usize,
    ) -> Vec<&CatalogItem> {
        let q = query.unwrap_or("").trim().to_ascii_lowercase();
        let tokens: Vec<&str> = q.split_whitespace().collect();
        let mut scored: Vec<(i32, &CatalogItem)> = Vec::new();
        for item in &self.items {
            if let Some(kf) = kind_filter
                && !kf.is_empty()
                && !item.kind.eq_ignore_ascii_case(kf)
            {
                continue;
            }
            if tokens.is_empty() {
                scored.push((1, item));
                continue;
            }
            let hay = item.signature.to_ascii_lowercase();
            let key_lower = item.key.to_ascii_lowercase();
            let mut score = 0i32;
            let mut matched_all = true;
            for tok in &tokens {
                if key_lower == *tok {
                    score += 50;
                } else if key_lower.contains(tok) {
                    score += 20;
                } else if hay.contains(tok) {
                    score += 5;
                } else {
                    matched_all = false;
                    break;
                }
            }
            if matched_all {
                scored.push((score, item));
            }
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.key.cmp(&b.1.key)));
        scored
            .into_iter()
            .take(limit.max(1))
            .map(|(_, it)| it)
            .collect()
    }

    pub fn find(&self, reference: &str) -> Option<&CatalogItem> {
        let r = reference.trim();
        self.items.iter().find(|it| {
            it.key == r
                || it.id.as_deref() == Some(r)
                || it.path.as_deref() == Some(r)
                || it.key.strip_prefix(&format!("{}:", it.kind)) == Some(r)
        })
    }
}

fn slug_from_path(rel: &str, suffix: &str) -> String {
    let trimmed = rel.strip_suffix(suffix).unwrap_or(rel);
    let trimmed = trimmed
        .strip_prefix("content/")
        .or_else(|| trimmed.strip_prefix(".ganbate/bricks/"))
        .unwrap_or(trimmed);
    let trimmed = trimmed
        .strip_prefix("prefabs/")
        .or_else(|| trimmed.strip_prefix("models/"))
        .or_else(|| trimmed.strip_prefix("looks/"))
        .or_else(|| trimmed.strip_prefix("materials/"))
        .or_else(|| trimmed.strip_prefix("data/"))
        .unwrap_or(trimmed);
    trimmed.replace(' ', "_")
}

fn field_type_short(ft: &FieldType) -> &'static str {
    match ft {
        FieldType::Bool => "bool",
        FieldType::Int { .. } => "int",
        FieldType::Float { .. } => "float",
        FieldType::String => "string",
        FieldType::Enum(_) => "enum",
        FieldType::Numbers { len: 2, .. } => "vec2",
        FieldType::Numbers {
            len: 3,
            color: true,
            ..
        } => "rgb",
        FieldType::Numbers { len: 3, .. } => "vec3",
        FieldType::Numbers {
            len: 4, quat: true, ..
        } => "quat",
        FieldType::Numbers {
            len: 4,
            color: true,
            ..
        } => "rgba",
        FieldType::Numbers { .. } => "vec",
        FieldType::Asset { .. } => "asset",
        FieldType::Entity => "entity",
        FieldType::Array { .. } => "array",
        FieldType::Object(_) => "object",
    }
}

/// Computes local AABB and named sockets for an entity map (`entities` in a prefab or brick).
pub fn compute_entities_bounds_and_sockets(
    project: &Project,
    entities: &BTreeMap<String, Value>,
) -> (Aabb, BTreeMap<String, [f32; 3]>) {
    let mut world_cache: BTreeMap<String, Transform> = BTreeMap::new();
    fn resolve_xf(
        id: &str,
        entities: &BTreeMap<String, Value>,
        cache: &mut BTreeMap<String, Transform>,
        visiting: &mut BTreeSet<String>,
    ) -> Transform {
        if let Some(xf) = cache.get(id) {
            return *xf;
        }
        if !visiting.insert(id.to_string()) {
            return Transform::IDENTITY;
        }
        let Some(ent) = entities.get(id) else {
            return Transform::IDENTITY;
        };
        let local = entity_local_transform(ent);
        let parent_xf = ent
            .get("parent")
            .and_then(|v| v.string().ok())
            .map(|pid| resolve_xf(pid, entities, cache, visiting))
            .unwrap_or(Transform::IDENTITY);
        visiting.remove(id);
        let wf = parent_xf.compose(local);
        cache.insert(id.to_string(), wf);
        wf
    }

    let mut aabb = Aabb::EMPTY;
    let mut sockets = BTreeMap::new();
    let mut visiting = BTreeSet::new();
    for (id, ent) in entities {
        let wf = resolve_xf(id, entities, &mut world_cache, &mut visiting);
        let name = ent.get("name").and_then(|v| v.string().ok()).unwrap_or("");
        let lower = name.to_ascii_lowercase();
        if let Some(sock) = lower
            .strip_prefix("socket_")
            .or_else(|| lower.strip_prefix("mount_"))
        {
            let Vec3(x, y, z) = wf.point(Vec3(0.0, 0.0, 0.0));
            sockets.insert(sock.to_string(), [x, y, z]);
            continue;
        }
        let local_box = entity_local_aabb(project, ent);
        aabb = aabb.union(local_box.transformed(wf));
    }
    if !aabb.is_valid() {
        aabb = Aabb {
            min: [-0.25, 0.0, -0.25],
            max: [0.25, 0.5, 0.25],
        };
    }
    for std_sock in ["base_bottom", "top_center", "center"] {
        if !sockets.contains_key(std_sock)
            && let Some(pos) = aabb.named_socket(std_sock)
        {
            sockets.insert(std_sock.to_string(), pos);
        }
    }
    (aabb, sockets)
}

pub fn entity_local_aabb(project: &Project, entity: &Value) -> Aabb {
    if let Some(model_id) = entity
        .get("components")
        .and_then(|c| c.get("mesh"))
        .and_then(|m| m.get("model"))
        .and_then(|v| v.string().ok())
        && let Some(geom) = inspect_model_geometry(project, model_id)
    {
        return geom.aabb;
    }
    if let Some(col) = entity.get("components").and_then(|c| c.get("collider")) {
        let shape = col
            .get("shape")
            .and_then(|v| v.string().ok())
            .unwrap_or("box");
        let center = col
            .get("center")
            .and_then(parse_vec3_f32)
            .unwrap_or([0.0, 0.0, 0.0]);
        match shape {
            "sphere" => {
                let r = col
                    .get("radius")
                    .and_then(|v| v.float().ok())
                    .unwrap_or(0.5)
                    .max(0.01);
                return Aabb {
                    min: [center[0] - r, center[1] - r, center[2] - r],
                    max: [center[0] + r, center[1] + r, center[2] + r],
                };
            }
            "capsule" => {
                let r = col
                    .get("radius")
                    .and_then(|v| v.float().ok())
                    .unwrap_or(0.5)
                    .max(0.01);
                let hh = col
                    .get("half_height")
                    .and_then(|v| v.float().ok())
                    .unwrap_or(0.5)
                    .max(0.0);
                return Aabb {
                    min: [center[0] - r, center[1] - (hh + r), center[2] - r],
                    max: [center[0] + r, center[1] + (hh + r), center[2] + r],
                };
            }
            _ => {
                let he = col
                    .get("half_extents")
                    .and_then(parse_vec3_f32)
                    .unwrap_or([0.5, 0.5, 0.5]);
                return Aabb {
                    min: [center[0] - he[0], center[1] - he[1], center[2] - he[2]],
                    max: [center[0] + he[0], center[1] + he[1], center[2] + he[2]],
                };
            }
        }
    }
    Aabb {
        min: [-0.25, 0.0, -0.25],
        max: [0.25, 0.5, 0.25],
    }
}

/// Builds the project's `Catalog` and writes `.ganbate/catalog.idx` if `write_index` is true.
pub fn build_catalog(
    project: &Project,
    registry: &Registry,
    write_index: bool,
) -> Result<Catalog, String> {
    let mut items = Vec::new();

    // 1. Crystallized Bricks in `.ganbate/bricks/*.brick.json`
    let bricks_dir = project.root.join(BRICKS_REL_DIR);
    if bricks_dir.is_dir()
        && let Ok(entries) = std::fs::read_dir(&bricks_dir)
    {
        let mut files: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        files.sort();
        for file_path in files {
            let Some(fname) = file_path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !fname.ends_with(".brick.json") {
                continue;
            }
            let Ok(bytes) = ganbate_assets::read_asset(&file_path) else {
                continue;
            };
            let Ok(doc) = json::parse(&bytes) else {
                continue;
            };
            let brick_id = doc
                .get("id")
                .and_then(|v| v.string().ok())
                .map(String::from)
                .unwrap_or_else(|| fname.trim_end_matches(".brick.json").to_string());
            let rel = format!("{BRICKS_REL_DIR}/{fname}");
            let empty = BTreeMap::new();
            let entities = match doc.get("entities") {
                Some(Value::Object(m)) => m,
                _ => &empty,
            };
            let (aabb, sockets) = compute_entities_bounds_and_sockets(project, entities);
            let mut params = Vec::new();
            if let Some(Value::Array(arr)) = doc.get("params") {
                for p in arr {
                    if let Ok(bp) = BrickParam::from_json(p) {
                        let def_str = json::to_compact(&bp.default).ok();
                        params.push((bp.name, bp.ty, def_str));
                    }
                }
            }
            let sock_list: Vec<&str> = sockets.keys().map(String::as_str).collect();
            let param_sig = if params.is_empty() {
                String::new()
            } else {
                let ps: Vec<String> = params
                    .iter()
                    .map(|(n, ty, d)| match d {
                        Some(dv) => format!("{n}:{ty}={dv}"),
                        None => format!("{n}:{ty}"),
                    })
                    .collect();
                format!(" params=[{}]", ps.join(","))
            };
            let key = format!("brick:{brick_id}");
            let signature = format!(
                "{key} path={rel} entities={} aabb={} sockets=[{}]{param_sig}",
                entities.len(),
                aabb.compact_sig(),
                sock_list.join(",")
            );
            items.push(CatalogItem {
                key,
                kind: "brick".into(),
                id: Some(brick_id),
                path: Some(rel),
                aabb: Some(aabb),
                sockets,
                params,
                signature,
            });
        }
    }

    // 2. Project Assets (Prefabs, Models, Looks, Materials, Data)
    for entry in project.entries() {
        match entry.kind {
            Kind::Prefab => {
                let slug = slug_from_path(&entry.path, ".prefab.json");
                let key = format!("prefab:{slug}");
                let (aabb, sockets) = if let Ok(doc) = project.read_document(&entry.id)
                    && let Some(Value::Object(entities)) = doc.get("entities")
                {
                    compute_entities_bounds_and_sockets(project, entities)
                } else {
                    (Aabb::UNIT, BTreeMap::new())
                };
                let sock_list: Vec<&str> = sockets.keys().map(String::as_str).collect();
                let signature = format!(
                    "{key} id={} path={} aabb={} sockets=[{}]",
                    entry.id,
                    entry.path,
                    aabb.compact_sig(),
                    sock_list.join(",")
                );
                items.push(CatalogItem {
                    key,
                    kind: "prefab".into(),
                    id: Some(entry.id.clone()),
                    path: Some(entry.path.clone()),
                    aabb: Some(aabb),
                    sockets,
                    params: Vec::new(),
                    signature,
                });
            }
            Kind::Model => {
                let slug = slug_from_path(&entry.path, ".glb");
                let key = format!("model:{slug}");
                let (aabb, sockets, tris) =
                    if let Some(g) = inspect_model_geometry(project, &entry.id) {
                        (g.aabb, g.sockets, g.triangles)
                    } else {
                        (Aabb::UNIT, BTreeMap::new(), 0)
                    };
                let sock_list: Vec<&str> = sockets.keys().map(String::as_str).collect();
                let signature = format!(
                    "{key} id={} path={} tris={tris} aabb={} sockets=[{}]",
                    entry.id,
                    entry.path,
                    aabb.compact_sig(),
                    sock_list.join(",")
                );
                items.push(CatalogItem {
                    key,
                    kind: "model".into(),
                    id: Some(entry.id.clone()),
                    path: Some(entry.path.clone()),
                    aabb: Some(aabb),
                    sockets,
                    params: Vec::new(),
                    signature,
                });
            }
            Kind::Look => {
                let slug = slug_from_path(&entry.path, ".look.json");
                let key = format!("look:{slug}");
                let signature = format!("{key} id={} path={}", entry.id, entry.path);
                items.push(CatalogItem {
                    key,
                    kind: "look".into(),
                    id: Some(entry.id.clone()),
                    path: Some(entry.path.clone()),
                    aabb: None,
                    sockets: BTreeMap::new(),
                    params: Vec::new(),
                    signature,
                });
            }
            Kind::Material => {
                let slug = slug_from_path(&entry.path, ".material.json");
                let key = format!("material:{slug}");
                let signature = format!("{key} id={} path={}", entry.id, entry.path);
                items.push(CatalogItem {
                    key,
                    kind: "material".into(),
                    id: Some(entry.id.clone()),
                    path: Some(entry.path.clone()),
                    aabb: None,
                    sockets: BTreeMap::new(),
                    params: Vec::new(),
                    signature,
                });
            }
            Kind::Data => {
                let slug = slug_from_path(&entry.path, ".data.json");
                let schema_tag = project
                    .read_document(&entry.id)
                    .ok()
                    .and_then(|d| {
                        d.get("schema")
                            .and_then(|v| v.string().ok())
                            .map(String::from)
                    })
                    .unwrap_or_default();
                let key = format!("data:{slug}");
                let signature = format!(
                    "{key} id={} schema={schema_tag} path={}",
                    entry.id, entry.path
                );
                items.push(CatalogItem {
                    key,
                    kind: "data".into(),
                    id: Some(entry.id.clone()),
                    path: Some(entry.path.clone()),
                    aabb: None,
                    sockets: BTreeMap::new(),
                    params: Vec::new(),
                    signature,
                });
            }
            Kind::Scene | Kind::Texture | Kind::Sound => {}
        }
    }

    // 3. Engine Traits & Game Schemas
    for kind_name in registry.kinds() {
        let Some(schema) = registry.get(kind_name) else {
            continue;
        };
        let is_game = kind_name.contains('.');
        let prefix = if is_game { "schema" } else { "trait" };
        let key = format!("{prefix}:{kind_name}");
        let mut params = Vec::new();
        let mut field_sigs = Vec::new();
        for (fname, fdef) in &schema.fields {
            let ft = field_type_short(&fdef.ty);
            field_sigs.push(format!("{fname}:{ft}"));
            params.push((
                fname.clone(),
                ft.to_string(),
                json::to_compact(&fdef.default).ok(),
            ));
        }
        let editor_hint = schema
            .meta
            .get("editor")
            .and_then(|v| v.string().ok())
            .map(|e| format!(" editor={e}"))
            .unwrap_or_default();
        let signature = format!("{key}{editor_hint} fields=[{}]", field_sigs.join(","));
        items.push(CatalogItem {
            key,
            kind: prefix.into(),
            id: None,
            path: None,
            aabb: None,
            sockets: BTreeMap::new(),
            params,
            signature,
        });
    }

    items.sort_by(|a, b| a.key.cmp(&b.key));
    let catalog = Catalog { items };

    if write_index {
        let idx_path = project.root.join(CATALOG_REL_PATH);
        if let Some(parent) = idx_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        ganbate_assets::write_asset(&idx_path, catalog.index_text().as_bytes())
            .map_err(|e| format!("{CATALOG_REL_PATH}: {e}"))?;
    }

    Ok(catalog)
}

/// Inspects one catalog item in detail, returning its full specification and a sample `.gbs` usage.
pub fn inspect_item(
    catalog: &Catalog,
    project: &Project,
    reference: &str,
) -> Result<Value, String> {
    let item = catalog
        .find(reference)
        .ok_or_else(|| format!("catalog item {reference:?} not found"))?;
    let mut obj_value = item.to_json();
    let example = match item.kind.as_str() {
        "brick" | "prefab" | "model" => {
            format!("spawn item_1 {}\nplace @this snap onto ground", item.key)
        }
        "trait" | "schema" => {
            let comp = item
                .key
                .split_once(':')
                .map_or(item.key.as_str(), |(_, k)| k);
            format!("attach @selection {comp}")
        }
        "look" => {
            let id = item.id.as_deref().unwrap_or("");
            format!("set @selection environment.look = \"{id}\"")
        }
        _ => format!("# {}", item.key),
    };
    if let Value::Object(ref mut m) = obj_value {
        m.insert("exampleGbs".into(), Value::String(example));
        if item.kind == "brick"
            && let Some(rel) = &item.path
            && let Ok(bytes) = ganbate_assets::read_asset(&project.root.join(rel))
            && let Ok(doc) = json::parse(&bytes)
        {
            m.insert("brickDocument".into(), doc);
        }
    }
    Ok(obj_value)
}

/// Loads a crystallized `.brick.json` document by `brick:<id>`, `<id>`, or path.
pub fn load_brick_document(project: &Project, reference: &str) -> Result<Value, String> {
    let clean = reference.strip_prefix("brick:").unwrap_or(reference);
    let candidate = if clean.ends_with(".brick.json") {
        project.root.join(clean)
    } else {
        project
            .root
            .join(BRICKS_REL_DIR)
            .join(format!("{clean}.brick.json"))
    };
    let bytes = ganbate_assets::read_asset(&candidate)
        .map_err(|_| format!("brick {reference:?} not found at {}", candidate.display()))?;
    json::parse(&bytes).map_err(|e| format!("{}: {e}", candidate.display()))
}

fn valid_brick_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 64
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// Extracts entity IDs touched/created by the latest (or named) transaction in `.ganbate/history/<doc_id>.jsonl`.
pub fn entities_from_history_tx(
    project_root: &Path,
    doc_id: &str,
    tx_index: Option<usize>,
) -> Result<Vec<String>, String> {
    let hist_path = project_root
        .join(".ganbate")
        .join("history")
        .join(format!("{doc_id}.jsonl"));
    let text =
        std::fs::read_to_string(&hist_path).map_err(|e| format!("{}: {e}", hist_path.display()))?;
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return Err(format!("no recorded transactions for {doc_id}"));
    }
    let line = match tx_index {
        Some(idx) => *lines
            .get(idx)
            .ok_or_else(|| format!("transaction index {idx} out of range (0..{})", lines.len()))?,
        None => *lines.last().expect("non-empty"),
    };
    let rec = json::parse(line.as_bytes()).map_err(|e| format!("bad history line: {e}"))?;
    let mut ids = BTreeSet::new();
    if let Some(Value::Array(ops)) = rec.get("ops") {
        for op in ops {
            if let Some(Value::Array(path)) = op.get("set")
                && path.len() >= 2
                && path[0].string().ok() == Some("entities")
                && let Ok(eid) = path[1].string()
            {
                ids.insert(eid.to_string());
            }
        }
    }
    if ids.is_empty() {
        return Err("the selected transaction did not create or modify any entities".into());
    }
    Ok(ids.into_iter().collect())
}

/// Resolves an entity reference (16-hex ID or unique name) inside `scene_root`.
pub fn resolve_entity_ref(scene_root: &Value, r: &str) -> Result<String, String> {
    let Some(Value::Object(entities)) = scene_root.get("entities") else {
        return Err("scene has no entities".into());
    };
    if entities.contains_key(r) {
        return Ok(r.to_string());
    }
    let matches: Vec<&String> = entities
        .iter()
        .filter(|(_, ent)| ent.get("name").and_then(|v| v.string().ok()) == Some(r))
        .map(|(k, _)| k)
        .collect();
    match matches.as_slice() {
        [only] => Ok((*only).clone()),
        [] => Err(format!("entity {r:?} not found")),
        _ => Err(format!("entity name {r:?} is ambiguous")),
    }
}

/// Crystallizes a subtree/selection of entities from `scene_root` into `.ganbate/bricks/<brick_id>.brick.json`
/// and refreshes `.ganbate/catalog.idx`.
///
/// `param_specs` entries have the form `"radius=Lamp_1.light.range"` or `"intensity=light.intensity"`.
pub fn crystallize_brick(
    project: &Project,
    registry: &Registry,
    scene_root: &Value,
    brick_id: &str,
    seed_refs: &[String],
    param_specs: &[String],
) -> Result<Value, String> {
    if !valid_brick_slug(brick_id) {
        return Err(format!(
            "brick id {brick_id:?} must use lowercase letters, digits, '_' or '-'"
        ));
    }
    if seed_refs.is_empty() {
        return Err(
            "crystallize needs at least one entity (via --entity, @selection, or --from-tx)".into(),
        );
    }
    let Some(Value::Object(all_entities)) = scene_root.get("entities") else {
        return Err("document has no entities".into());
    };

    let mut selected_ids = BTreeSet::new();
    for r in seed_refs {
        let eid = resolve_entity_ref(scene_root, r)?;
        selected_ids.insert(eid);
    }

    // Include all descendants of selected entities so subtrees stay intact.
    let mut changed = true;
    while changed {
        changed = false;
        for (eid, ent) in all_entities {
            if !selected_ids.contains(eid)
                && let Some(pid) = ent.get("parent").and_then(|v| v.string().ok())
                && selected_ids.contains(pid)
            {
                selected_ids.insert(eid.clone());
                changed = true;
            }
        }
    }

    // Extract selected entities and normalize root entities' translations so base_bottom is [0, 0, 0].
    let mut extracted = BTreeMap::new();
    for eid in &selected_ids {
        if let Some(ent) = all_entities.get(eid) {
            let mut copy = ent.clone();
            if let Value::Object(ref mut emap) = copy {
                let keep_parent = emap
                    .get("parent")
                    .and_then(|v| v.string().ok())
                    .is_some_and(|pid| selected_ids.contains(pid));
                if !keep_parent {
                    emap.remove("parent");
                }
            }
            extracted.insert(eid.clone(), copy);
        }
    }

    let (raw_aabb, _) = compute_entities_bounds_and_sockets(project, &extracted);
    let pivot = raw_aabb
        .named_socket("base_bottom")
        .unwrap_or([0.0, 0.0, 0.0]);

    for ent in extracted.values_mut() {
        let is_root = matches!(ent.get("parent"), None | Some(Value::Null));
        if !is_root {
            continue;
        }
        if let Value::Object(emap) = ent {
            let comps = emap
                .entry("components".to_string())
                .or_insert_with(|| Value::Object(BTreeMap::new()));
            if let Value::Object(cmap) = comps {
                let xf = cmap
                    .entry("transform".to_string())
                    .or_insert_with(|| Value::Object(BTreeMap::new()));
                if let Value::Object(xmap) = xf {
                    let cur = xmap
                        .get("translation")
                        .and_then(parse_vec3_f32)
                        .unwrap_or([0.0, 0.0, 0.0]);
                    let norm = [cur[0] - pivot[0], cur[1] - pivot[1], cur[2] - pivot[2]];
                    xmap.insert(
                        "translation".into(),
                        Value::Array(norm.iter().map(|n| Value::Number(*n as f64)).collect()),
                    );
                }
            }
        }
    }

    let (norm_aabb, sockets) = compute_entities_bounds_and_sockets(project, &extracted);

    // Parse parameter bindings: `param_name=[entity.]component.field`
    let mut params = Vec::new();
    for spec in param_specs {
        let (pname, target_expr) = spec.split_once('=').ok_or_else(|| {
            format!("invalid --param {spec:?}; expected name=component.field or name=Entity.component.field")
        })?;
        let parts: Vec<&str> = target_expr.split('.').collect();
        if parts.len() < 2 {
            return Err(format!(
                "invalid param path {target_expr:?}; expected component.field"
            ));
        }
        // Check if parts[0] names an entity in `extracted`, otherwise pick the first entity having component parts[0].
        let (target_eid, comp_parts): (String, &[&str]) = if let Some((eid, _)) =
            extracted.iter().find(|(id, e)| {
                *id == parts[0] || e.get("name").and_then(|v| v.string().ok()) == Some(parts[0])
            }) {
            if parts.len() < 3 {
                return Err(format!(
                    "param {spec:?} names entity {:?} without component.field",
                    parts[0]
                ));
            }
            (eid.clone(), &parts[1..])
        } else {
            let comp_name = parts[0];
            let found = extracted
                .iter()
                .find(|(_, e)| e.get("components").and_then(|c| c.get(comp_name)).is_some())
                .or_else(|| extracted.iter().next())
                .map(|(id, _)| id.clone())
                .ok_or("brick has no entities")?;
            (found, &parts[..])
        };

        let mut path_vec = vec!["components".to_string()];
        path_vec.extend(comp_parts.iter().map(|s| (*s).to_string()));

        // Read current value or schema default.
        let mut cur_val: Option<&Value> = extracted.get(&target_eid);
        for seg in &path_vec {
            cur_val = cur_val.and_then(|v| v.get(seg));
        }
        let comp_kind = comp_parts[0];
        let field_name = comp_parts[1];
        let (ty_str, default_val) = if let Some(schema) = registry.get(comp_kind)
            && let Some(fdef) = schema.fields.get(field_name)
        {
            (
                field_type_short(&fdef.ty).to_string(),
                cur_val.cloned().unwrap_or_else(|| fdef.default.clone()),
            )
        } else {
            (
                "value".to_string(),
                cur_val.cloned().unwrap_or(Value::Number(0.0)),
            )
        };

        params.push(BrickParam {
            name: pname.trim().to_string(),
            target_entity: target_eid,
            path: path_vec,
            ty: ty_str,
            default: default_val,
        });
    }

    let brick_doc = Value::Object(BTreeMap::from([
        ("ganbate".into(), Value::String("brick".into())),
        ("version".into(), Value::Number(1.0)),
        ("id".into(), Value::String(brick_id.into())),
        ("aabb".into(), norm_aabb.to_json()),
        (
            "sockets".into(),
            Value::Object(
                sockets
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.clone(),
                            Value::Array(v.iter().map(|n| Value::Number(*n as f64)).collect()),
                        )
                    })
                    .collect(),
            ),
        ),
        (
            "params".into(),
            Value::Array(params.iter().map(BrickParam::to_json).collect()),
        ),
        ("entities".into(), Value::Object(extracted)),
    ]));

    let bricks_dir = project.root.join(BRICKS_REL_DIR);
    std::fs::create_dir_all(&bricks_dir).map_err(|e| format!("{}: {e}", bricks_dir.display()))?;
    let rel_path = format!("{BRICKS_REL_DIR}/{brick_id}.brick.json");
    let full_path = project.root.join(&rel_path);
    let canonical = json::to_canonical(&brick_doc)?;
    ganbate_assets::write_asset(&full_path, canonical.as_bytes())
        .map_err(|e| format!("{rel_path}: {e}"))?;

    let catalog = build_catalog(project, registry, true)?;
    let entry_json = catalog
        .find(&format!("brick:{brick_id}"))
        .map(CatalogItem::to_json)
        .unwrap_or(Value::Null);

    Ok(Value::Object(BTreeMap::from([
        ("id".into(), Value::String(brick_id.into())),
        ("key".into(), Value::String(format!("brick:{brick_id}"))),
        ("path".into(), Value::String(rel_path)),
        ("catalogEntry".into(), entry_json),
    ])))
}
