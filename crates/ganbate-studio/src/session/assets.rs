//! Asset catalog metadata, reference ("where used") queries, non-destructive move/rename, and
//! direct file import over the Studio edit protocol.
use super::*;
use ganbate_pipeline::project::{MANIFEST, new_sidecar, sidecar_path};
use ganbate_scene::schema::{Field, FieldType};

/// Maximum raw file size accepted by `asset-import` (16 MiB, well within `json::MAX_BYTES`).
pub const MAX_IMPORT_BYTES: usize = 16 * 1024 * 1024;

struct AssetRef {
    target: String,
    doc: Option<String>,
    doc_path: String,
    doc_kind: &'static str,
    entity: Option<String>,
    entity_name: Option<String>,
    field: String,
}

impl Studio {
    pub(super) fn asset_bytes(&self, rel: &str) -> u64 {
        if self.inside(rel).is_err() {
            return 0;
        }
        std::fs::metadata(self.project.root.join(rel)).map_or(0, |m| m.len())
    }

    pub(super) fn plain_asset_path(&self, path: &str) -> Result<(String, Kind), String> {
        if path
            .split('/')
            .any(|p| p.is_empty() || p.starts_with('.') || p.contains(['\\', ':', '\0']))
        {
            return Err(format!(
                "{path:?} is not a plain path (names separated by /, none starting with a dot)"
            ));
        }
        let Some(folder) = self
            .project
            .manifest
            .content
            .iter()
            .find(|c| path.starts_with(&format!("{c}/")))
        else {
            return Err("put assets in a content folder".into());
        };
        let depth = path[folder.len() + 1..].matches('/').count();
        if depth > MAX_FOLDER_DEPTH {
            return Err(format!(
                "{path} is {depth} folders deep in {folder}; a project keeps to {MAX_FOLDER_DEPTH}"
            ));
        }
        let kind = self.project.check_asset_path(path)?;
        self.inside(path)?;
        if !kind.self_identified() {
            self.inside(&format!("{path}.meta.json"))?;
        }
        Ok((path.to_string(), kind))
    }

    pub(super) fn asset_usage(&self, from: ClientId, msg: &Value) -> Result<Outbox, String> {
        let req = msg.get("req").cloned().unwrap_or(Value::Null);
        match msg.get("id") {
            None | Some(Value::Null) => {
                let mut counts: BTreeMap<String, u64> = BTreeMap::new();
                self.visit_project_references(|r| {
                    *counts.entry(r.target).or_insert(0) += 1;
                });
                let counts_obj =
                    Value::Object(counts.into_iter().map(|(k, v)| (k, n(v))).collect());
                Ok(vec![(
                    from,
                    obj(vec![
                        ("type", s("asset-usage")),
                        ("req", req),
                        ("counts", counts_obj),
                    ]),
                )])
            }
            Some(v) => {
                let target = v
                    .string()
                    .ok()
                    .filter(|id| is_asset_id(id))
                    .ok_or("missing or invalid asset id")?;
                let mut uses = Vec::new();
                self.visit_project_references(|r| {
                    if r.target == target {
                        uses.push(obj(vec![
                            ("doc", r.doc.as_deref().map_or(Value::Null, s)),
                            ("docPath", s(&r.doc_path)),
                            ("docKind", s(r.doc_kind)),
                            ("entity", r.entity.as_deref().map_or(Value::Null, s)),
                            (
                                "entityName",
                                r.entity_name.as_deref().map_or(Value::Null, s),
                            ),
                            ("field", s(&r.field)),
                        ]));
                    }
                });
                Ok(vec![(
                    from,
                    obj(vec![
                        ("type", s("asset-usage")),
                        ("id", s(target)),
                        ("req", req),
                        ("uses", Value::Array(uses)),
                    ]),
                )])
            }
        }
    }

    fn visit_project_references(&self, mut emit: impl FnMut(AssetRef)) {
        if let Some(startup) = &self.project.manifest.startup {
            emit(AssetRef {
                target: startup.clone(),
                doc: None,
                doc_path: MANIFEST.into(),
                doc_kind: "project",
                entity: None,
                entity_name: None,
                field: "startup".into(),
            });
        }
        for entry in self.project.entries() {
            match entry.kind {
                Kind::Scene | Kind::Prefab => {
                    let root = if let Some(open) = self.open.get(&entry.id)
                        && open.unavailable.is_none()
                    {
                        open.doc.root().clone()
                    } else if self.inside(&entry.path).is_ok()
                        && let Ok(disk) = self.project.read_document(&entry.id)
                    {
                        disk
                    } else {
                        continue;
                    };
                    visit_scene_references(
                        &self.registry,
                        &entry.id,
                        &entry.path,
                        entry.kind.name(),
                        &root,
                        &mut emit,
                    );
                }
                Kind::Material => {
                    if self.inside(&entry.path).is_ok()
                        && let Ok(disk) = self.project.read_document(&entry.id)
                    {
                        visit_material_references(&entry.id, &entry.path, &disk, "", &mut emit);
                    }
                }
                _ => {}
            }
        }
    }

    pub(super) fn asset_move(&mut self, from: ClientId, msg: &Value) -> Result<Outbox, String> {
        let req = msg.get("req").cloned().unwrap_or(Value::Null);
        let id = msg
            .get("id")
            .and_then(|v| v.string().ok())
            .filter(|id| is_asset_id(id))
            .ok_or("missing or invalid asset id")?
            .to_string();
        let entry = self
            .project
            .entry(&id)
            .ok_or_else(|| format!("no asset {id}"))?
            .clone();
        let to_raw = msg
            .get("to")
            .and_then(|v| v.string().ok())
            .ok_or("give a target path")?;
        let (to, to_kind) = self.plain_asset_path(to_raw)?;
        if to_kind != entry.kind {
            return Err(format!(
                "cannot move {} ({}) to {} ({})",
                entry.path,
                entry.kind.name(),
                to,
                to_kind.name()
            ));
        }
        if to == entry.path {
            return Ok(vec![(
                from,
                obj(vec![
                    ("type", s("asset-moved")),
                    ("id", s(&id)),
                    ("kind", s(entry.kind.name())),
                    ("from", s(&entry.path)),
                    ("to", s(&to)),
                    ("req", req),
                ]),
            )]);
        }

        self.inside(&entry.path)?;
        if !entry.kind.self_identified() {
            self.inside(&format!("{}.meta.json", entry.path))?;
        }
        let from_file = self.project.root.join(&entry.path);
        if !std::fs::symlink_metadata(&from_file).is_ok_and(|m| m.is_file()) {
            return Err(format!("{} is missing on disk", entry.path));
        }
        let from_meta = sidecar_path(&from_file);
        if !entry.kind.self_identified()
            && !std::fs::symlink_metadata(&from_meta).is_ok_and(|m| m.is_file())
        {
            return Err(format!("{}.meta.json is missing on disk", entry.path));
        }

        let to_file = self.project.root.join(&to);
        let to_meta = sidecar_path(&to_file);
        if std::fs::symlink_metadata(&to_file).is_ok() || self.project.find_path(&to).is_some() {
            return Err(format!("{to} already exists"));
        }
        if !entry.kind.self_identified() && std::fs::symlink_metadata(&to_meta).is_ok() {
            return Err(format!("{to}.meta.json already exists"));
        }

        if let Some(parent) = to_file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        self.inside(&to)?;
        if !entry.kind.self_identified() {
            self.inside(&format!("{to}.meta.json"))?;
        }

        std::fs::rename(&from_file, &to_file)
            .map_err(|e| format!("{} -> {to}: {e}", entry.path))?;
        if !entry.kind.self_identified()
            && let Err(e) = std::fs::rename(&from_meta, &to_meta)
        {
            let _ = std::fs::rename(&to_file, &from_file);
            return Err(format!("{}.meta.json -> {to}.meta.json: {e}", entry.path));
        }

        self.project.relocate_entry(&id, &to)?;
        if let Some(open) = self.open.get_mut(&id) {
            open.path = to.clone();
        }

        let assets = obj(vec![("type", s("assets")), ("assets", self.assets())]);
        let mut out: Outbox = self.clients.keys().map(|c| (*c, assets.clone())).collect();
        out.push((
            from,
            obj(vec![
                ("type", s("asset-moved")),
                ("id", s(&id)),
                ("kind", s(entry.kind.name())),
                ("from", s(&entry.path)),
                ("to", s(&to)),
                ("req", req),
            ]),
        ));
        if let Some(open) = self.open.get(&id)
            && !open.watchers.is_empty()
        {
            let doc_msg = self.doc_message(&id);
            out.extend(open.watchers.iter().map(|w| (*w, doc_msg.clone())));
        }
        Ok(out)
    }

    pub(super) fn asset_import(&mut self, from: ClientId, msg: &Value) -> Result<Outbox, String> {
        let req = msg.get("req").cloned().unwrap_or(Value::Null);
        let path_raw = msg
            .get("path")
            .and_then(|v| v.string().ok())
            .ok_or("give the imported asset a path")?;
        let (path, kind) = self.plain_asset_path(path_raw)?;
        let b64 = msg
            .get("dataBase64")
            .and_then(|v| v.string().ok())
            .ok_or("asset-import needs dataBase64")?;
        if b64.len() > MAX_IMPORT_BYTES / 3 * 4 + 4096 {
            return Err("asset exceeds 16 MiB import limit".into());
        }
        let bytes = ganbate_core::digest::base64_decode(b64)?;
        if bytes.is_empty() {
            return Err("imported file is empty".into());
        }
        if bytes.len() > MAX_IMPORT_BYTES {
            return Err("asset exceeds 16 MiB import limit".into());
        }

        let dest = self.project.root.join(&path);
        if std::fs::symlink_metadata(&dest).is_ok() || self.project.find_path(&path).is_some() {
            return Err(format!("{path} already exists"));
        }
        if !kind.self_identified() && std::fs::symlink_metadata(sidecar_path(&dest)).is_ok() {
            return Err(format!("{path}.meta.json already exists"));
        }

        let id = match kind {
            Kind::Scene | Kind::Prefab => {
                let mut doc = json::parse(&bytes).map_err(|e| format!("{path}: {e}"))?;
                let id = self.prepare_imported_document(&mut doc, kind)?;
                let issues = scene::check(&doc, &self.registry);
                if let Some(first) = issues.first() {
                    return Err(format!("{path} · {}: {}", first.path, first.message));
                }
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("{}: {e}", parent.display()))?;
                }
                self.inside(&path)?;
                self.project.write_document(&path, kind, &doc)?;
                id
            }
            Kind::Material => {
                let mut doc = json::parse(&bytes).map_err(|e| format!("{path}: {e}"))?;
                let id = self.prepare_imported_material(&mut doc)?;
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("{}: {e}", parent.display()))?;
                }
                self.inside(&path)?;
                self.project.write_document(&path, Kind::Material, &doc)?;
                id
            }
            Kind::Look => {
                let doc = json::parse(&bytes).map_err(|e| format!("{path}: {e}"))?;
                validate_look(&doc).map_err(|e| format!("{path}: {e}"))?;
                let canonical = json::to_canonical(&doc)?;
                self.install_sidecar_asset(&path, Kind::Look, canonical.as_bytes())?
            }
            Kind::Model => {
                ganbate_assets::glb::import_nodes(&bytes, true)
                    .map_err(|e| format!("{path}: {e}"))?;
                self.install_sidecar_asset(&path, Kind::Model, &bytes)?
            }
            Kind::Texture => {
                validate_texture_header(&path, &bytes)?;
                self.install_sidecar_asset(&path, Kind::Texture, &bytes)?
            }
            Kind::Sound => {
                validate_sound_header(&path, &bytes)?;
                self.install_sidecar_asset(&path, Kind::Sound, &bytes)?
            }
        };

        let assets = obj(vec![("type", s("assets")), ("assets", self.assets())]);
        let mut out: Outbox = self.clients.keys().map(|c| (*c, assets.clone())).collect();
        out.push((
            from,
            obj(vec![
                ("type", s("asset-imported")),
                ("id", s(&id)),
                ("kind", s(kind.name())),
                ("path", s(&path)),
                ("req", req),
            ]),
        ));
        Ok(out)
    }

    fn fresh_asset_id(&mut self) -> String {
        loop {
            let candidate = self.ids.asset();
            if self.project.entry(&candidate).is_none() {
                return candidate;
            }
        }
    }

    fn prepare_imported_document(&mut self, doc: &mut Value, kind: Kind) -> Result<String, String> {
        let existing_id = doc
            .get("id")
            .and_then(|v| v.string().ok())
            .filter(|id| is_asset_id(id) && self.project.entry(id).is_none())
            .map(String::from);
        let id = existing_id.unwrap_or_else(|| self.fresh_asset_id());
        let Value::Object(map) = doc else {
            return Err(format!("a {} document is a JSON object", kind.name()));
        };
        map.insert("id".into(), Value::String(id.clone()));
        Ok(id)
    }

    fn prepare_imported_material(&mut self, doc: &mut Value) -> Result<String, String> {
        let existing_id = doc
            .get("id")
            .and_then(|v| v.string().ok())
            .filter(|id| is_asset_id(id) && self.project.entry(id).is_none())
            .map(String::from);
        let id = existing_id.unwrap_or_else(|| self.fresh_asset_id());
        let Value::Object(map) = doc else {
            return Err("a material document is a JSON object".into());
        };
        map.insert("id".into(), Value::String(id.clone()));
        Ok(id)
    }

    fn install_sidecar_asset(
        &mut self,
        rel: &str,
        kind: Kind,
        bytes: &[u8],
    ) -> Result<String, String> {
        let dest = self.project.root.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        self.inside(rel)?;
        self.inside(&format!("{rel}.meta.json"))?;
        let id = self.fresh_asset_id();
        let meta_path = sidecar_path(&dest);
        let meta_text = json::to_canonical(&new_sidecar(&id, kind))?;
        ganbate_assets::create_asset(&meta_path, meta_text.as_bytes())
            .map_err(|e| format!("{rel}.meta.json: {e}"))?;
        if let Err(e) = ganbate_assets::create_asset(&dest, bytes) {
            let _ = std::fs::remove_file(&meta_path);
            return Err(format!("{rel}: {e}"));
        }
        if let Err(e) = self
            .project
            .register_sidecar_asset(id.clone(), kind, rel.to_string())
        {
            let _ = std::fs::remove_file(&dest);
            let _ = std::fs::remove_file(&meta_path);
            return Err(e);
        }
        Ok(id)
    }
}

fn validate_texture_header(path: &str, bytes: &[u8]) -> Result<(), String> {
    let lower = path.to_ascii_lowercase();
    let ok = if lower.ends_with(".png") {
        bytes.starts_with(b"\x89PNG\r\n\x1a\n")
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        bytes.starts_with(b"\xff\xd8\xff")
    } else if lower.ends_with(".webp") {
        bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP"
    } else {
        false
    };
    if !ok {
        return Err(format!("{path}: file header does not match image format"));
    }
    Ok(())
}

fn validate_sound_header(path: &str, bytes: &[u8]) -> Result<(), String> {
    let lower = path.to_ascii_lowercase();
    let ok = if lower.ends_with(".wav") {
        bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE"
    } else if lower.ends_with(".ogg") {
        bytes.starts_with(b"OggS")
    } else {
        false
    };
    if !ok {
        return Err(format!("{path}: file header does not match audio format"));
    }
    Ok(())
}

fn visit_field_refs(field: &Field, value: &Value, path: &str, emit: &mut dyn FnMut(&str, &str)) {
    match (&field.ty, value) {
        (FieldType::Asset { .. }, Value::String(id)) if is_asset_id(id) => {
            emit(id, path);
        }
        (FieldType::Array { items, .. }, Value::Array(values)) => {
            for (index, item) in values.iter().enumerate() {
                visit_field_refs(items, item, &format!("{path}[{index}]"), emit);
            }
        }
        (FieldType::Object(fields), Value::Object(values)) => {
            for (name, inner) in values {
                if let Some(sub) = fields.get(name) {
                    visit_field_refs(sub, inner, &format!("{path}.{name}"), emit);
                }
            }
        }
        _ => {}
    }
}

fn visit_component_refs(
    registry: &Registry,
    holder: &Value,
    prefix: &str,
    emit: &mut dyn FnMut(&str, &str),
) {
    let Some(Value::Object(components)) = holder.get("components") else {
        return;
    };
    for (comp_name, stored) in components {
        let Some(schema) = registry.get(comp_name) else {
            continue;
        };
        for (field_name, field_def) in &schema.fields {
            if let Some(value) = stored.get(field_name) {
                let field_path = if prefix.is_empty() {
                    format!("{comp_name}.{field_name}")
                } else {
                    format!("{prefix}.{comp_name}.{field_name}")
                };
                visit_field_refs(field_def, value, &field_path, emit);
            }
        }
    }
}

fn visit_scene_references(
    registry: &Registry,
    doc_id: &str,
    doc_path: &str,
    doc_kind: &'static str,
    root: &Value,
    emit: &mut impl FnMut(AssetRef),
) {
    let Some(Value::Object(entities)) = root.get("entities") else {
        return;
    };
    for (ent_id, entity) in entities {
        let ent_name = entity
            .get("name")
            .and_then(|v| v.string().ok())
            .unwrap_or("Entity")
            .to_string();
        visit_component_refs(registry, entity, "", &mut |target, field| {
            emit(AssetRef {
                target: target.to_string(),
                doc: Some(doc_id.to_string()),
                doc_path: doc_path.to_string(),
                doc_kind,
                entity: Some(ent_id.clone()),
                entity_name: Some(ent_name.clone()),
                field: field.to_string(),
            });
        });
        if let Some(link) = entity.get("prefab") {
            if let Some(Ok(prefab_id)) = link.get("asset").map(Value::string)
                && is_asset_id(prefab_id)
            {
                emit(AssetRef {
                    target: prefab_id.to_string(),
                    doc: Some(doc_id.to_string()),
                    doc_path: doc_path.to_string(),
                    doc_kind,
                    entity: Some(ent_id.clone()),
                    entity_name: Some(ent_name.clone()),
                    field: "prefab".into(),
                });
            }
            if let Some(Value::Object(overrides)) = link.get("overrides") {
                for (inner, data) in overrides {
                    let prefix = format!("overrides.{inner}");
                    visit_component_refs(registry, data, &prefix, &mut |target, field| {
                        emit(AssetRef {
                            target: target.to_string(),
                            doc: Some(doc_id.to_string()),
                            doc_path: doc_path.to_string(),
                            doc_kind,
                            entity: Some(ent_id.clone()),
                            entity_name: Some(ent_name.clone()),
                            field: field.to_string(),
                        });
                    });
                }
            }
        }
    }
}

fn visit_material_references(
    doc_id: &str,
    doc_path: &str,
    value: &Value,
    prefix: &str,
    emit: &mut impl FnMut(AssetRef),
) {
    match value {
        Value::String(id) if !prefix.is_empty() && prefix != "id" && is_asset_id(id) => {
            emit(AssetRef {
                target: id.clone(),
                doc: Some(doc_id.to_string()),
                doc_path: doc_path.to_string(),
                doc_kind: "material",
                entity: None,
                entity_name: None,
                field: prefix.to_string(),
            });
        }
        Value::Object(map) => {
            for (k, v) in map {
                if prefix.is_empty() && k == "id" {
                    continue;
                }
                let next = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                visit_material_references(doc_id, doc_path, v, &next, emit);
            }
        }
        Value::Array(items) => {
            for (i, v) in items.iter().enumerate() {
                visit_material_references(doc_id, doc_path, v, &format!("{prefix}[{i}]"), emit);
            }
        }
        _ => {}
    }
}
