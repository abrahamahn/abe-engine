//! Refreshes the file-backed asset index and reconciles the documents already open in Studio.
use super::*;
use ganbate_scene::schema::{Field, FieldType};

fn inventory(project: &Project) -> BTreeMap<String, String> {
    project
        .entries()
        .map(|entry| (entry.id.clone(), entry.path.clone()))
        .collect()
}

fn strings(values: impl IntoIterator<Item = String>) -> Value {
    Value::Array(values.into_iter().map(Value::String).collect())
}

impl Studio {
    pub(super) fn unavailable_drafts(&self) -> Value {
        Value::Array(
            self.open
                .iter()
                .filter(|(_, doc)| doc.unavailable.is_some() && doc.history.dirty())
                .map(|(id, doc)| {
                    obj(vec![
                        ("id", s(id)),
                        ("path", s(&doc.path)),
                        ("kind", s(doc.kind.name())),
                    ])
                })
                .collect(),
        )
    }

    pub(super) fn rescan(&mut self) -> Outbox {
        let before = inventory(&self.project);
        let scan = match self.project.scan(Some(&mut self.ids)) {
            Ok(report) => report,
            Err(error) => {
                let report = obj(vec![
                    ("type", s("rescanned")),
                    ("ok", Value::Bool(false)),
                    ("problems", strings([error])),
                ]);
                return self
                    .clients
                    .keys()
                    .map(|id| (*id, report.clone()))
                    .collect();
            }
        };
        let after = inventory(&self.project);
        let added = after
            .iter()
            .filter(|(id, _)| !before.contains_key(*id))
            .map(|(_, path)| path.clone());
        let removed = before
            .iter()
            .filter(|(id, _)| !after.contains_key(*id))
            .map(|(_, path)| path.clone());
        let moved = after.iter().filter_map(|(id, path)| {
            let old = before.get(id)?;
            (old != path).then(|| obj(vec![("id", s(id)), ("from", s(old)), ("to", s(path))]))
        });
        let mut problems: BTreeSet<String> = scan.problems.into_iter().collect();
        self.open.retain(|id, doc| {
            after.contains_key(id) || doc.history.dirty() || !doc.watchers.is_empty()
        });
        for (id, doc) in &mut self.open {
            // The draft belongs to its id, never to the path at which it used to live.
            doc.unavailable = reconcile_source(&self.project, &self.real_root, id, doc).err();
            if let Some(error) = &doc.unavailable {
                problems.insert(error.clone());
            } else if doc.changed_on_disk {
                problems.insert(format!(
                    "{}: the file also changed on disk; the unsaved edit was kept",
                    doc.path
                ));
            }
        }

        // Validate after every open prefab has reached its new state. An unrelated edit may
        // still proceed in a scene that already has a missing reference.
        let prefabs = self.open_prefabs();
        let load = |id: &str| load_prefab(&prefabs, &self.project, &self.real_root, id);
        for doc in self.open.values_mut() {
            doc.known_issues = issue_set(doc.kind, doc.doc.root(), &self.registry, &load);
            for (path, message) in &doc.known_issues {
                problems.insert(format!("{} · {path}: {message}", doc.path));
            }
            if matches!(doc.kind, Kind::Scene | Kind::Prefab) {
                collect_references(
                    &self.project,
                    &self.registry,
                    &doc.path,
                    doc.doc.root(),
                    &mut problems,
                );
            }
        }

        let mut out = Vec::new();
        let assets = obj(vec![("type", s("assets")), ("assets", self.assets())]);
        let report = obj(vec![
            ("type", s("rescanned")),
            ("ok", Value::Bool(true)),
            ("count", n(after.len() as u64)),
            ("added", strings(added)),
            ("removed", strings(removed)),
            ("moved", Value::Array(moved.collect())),
            ("created", strings(scan.created)),
            ("drafts", self.unavailable_drafts()),
            ("problems", strings(problems)),
        ]);
        for client in self.clients.keys() {
            out.push((*client, assets.clone()));
            out.push((*client, report.clone()));
        }
        // Refresh the viewport even when a model was re-exported under the same path and id.
        for (id, doc) in &self.open {
            if !doc.watchers.is_empty() {
                let message = self.doc_message(id);
                out.extend(doc.watchers.iter().map(|client| (*client, message.clone())));
            }
        }
        out
    }
}

fn reconcile_source(
    project: &Project,
    real_root: &Path,
    id: &str,
    doc: &mut Open,
) -> Result<(), String> {
    let entry = project.entry(id).filter(|entry| entry.kind == doc.kind).ok_or_else(||
        format!("{}: source {id} is missing or has a different identity; restore it and rescan to save this document", doc.path))?;
    doc.path = entry.path.clone();
    inside(&project.root, real_root, &doc.path)?;
    let disk = project.read_document(id)?;
    if doc.kind.self_identified() && disk.get("id").and_then(|v| v.string().ok()) != Some(id) {
        return Err(format!("{}: identity changed during the scan", doc.path));
    }
    doc.changed_on_disk = disk != doc.on_disk;
    if doc.changed_on_disk && !doc.history.dirty() {
        doc.doc.reload(disk.clone())?;
        doc.on_disk = disk;
        doc.history = History::new(HISTORY_LIMIT);
        doc.changed_on_disk = false;
    }
    Ok(())
}

fn check_id(
    project: &Project,
    id: &Value,
    kind: Option<&str>,
    path: &str,
    errors: &mut BTreeSet<String>,
) {
    if let Ok(id) = id.string() {
        match project.entry(id) {
            None => {
                errors.insert(format!("{path}: asset {id} is missing"));
            }
            Some(entry) if kind.is_some_and(|k| k != entry.kind.name()) => {
                errors.insert(format!(
                    "{path}: {} is a {}, expected {}",
                    entry.path,
                    entry.kind.name(),
                    kind.unwrap_or("asset")
                ));
            }
            _ => {}
        }
    }
}

fn check_field(
    project: &Project,
    field: &Field,
    value: &Value,
    path: &str,
    errors: &mut BTreeSet<String>,
) {
    match (&field.ty, value) {
        (FieldType::Asset { kind }, id) => check_id(project, id, kind.as_deref(), path, errors),
        (FieldType::Array { items, .. }, Value::Array(values)) => {
            for (index, value) in values.iter().enumerate() {
                check_field(project, items, value, &format!("{path}[{index}]"), errors);
            }
        }
        (FieldType::Object(fields), Value::Object(values)) => {
            for (name, value) in values {
                if let Some(field) = fields.get(name) {
                    check_field(project, field, value, &format!("{path}.{name}"), errors);
                }
            }
        }
        _ => {}
    }
}

fn check_components(
    project: &Project,
    registry: &Registry,
    entity: &Value,
    path: &str,
    errors: &mut BTreeSet<String>,
) {
    let Some(Value::Object(components)) = entity.get("components") else {
        return;
    };
    for (kind, stored) in components {
        let Some(schema) = registry.get(kind) else {
            continue;
        };
        for (name, field) in &schema.fields {
            if let Some(value) = stored.get(name) {
                check_field(
                    project,
                    field,
                    value,
                    &format!("{path}.{kind}.{name}"),
                    errors,
                );
            }
        }
    }
}

fn collect_references(
    project: &Project,
    registry: &Registry,
    filename: &str,
    root: &Value,
    errors: &mut BTreeSet<String>,
) {
    let Some(Value::Object(entities)) = root.get("entities") else {
        return;
    };
    for (id, entity) in entities {
        let path = format!(
            "{filename} · {} ({id})",
            entity
                .get("name")
                .and_then(|v| v.string().ok())
                .unwrap_or("entity")
        );
        check_components(project, registry, entity, &path, errors);
        if let Some(link) = entity.get("prefab") {
            if let Some(asset) = link.get("asset") {
                check_id(
                    project,
                    asset,
                    Some("prefab"),
                    &format!("{path}.prefab"),
                    errors,
                );
            }
            if let Some(Value::Object(overrides)) = link.get("overrides") {
                for (inner, data) in overrides {
                    check_components(
                        project,
                        registry,
                        data,
                        &format!("{path}.overrides.{inner}"),
                        errors,
                    );
                }
            }
        }
    }
}
