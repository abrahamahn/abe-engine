//! A project opened the way ABE Studio and `abe-build` open one: its manifest, the component
//! schemas (the engine's and the game's) and, when a command needs it, the index of its assets
//! (scanned without writing sidecars). A scene is found by its path without scanning, or by its
//! asset id through the index.
use std::io::Read;
use std::path::{Path, PathBuf};

use abe_core::digest;
use abe_core::id::is_asset_id;
use abe_core::json::{self, Value};
use abe_pipeline::project::{Kind, MANIFEST, Manifest, Project};
use abe_scene::scene;
use abe_scene::schema::{Registry, engine_registry};

use crate::error::{self, Error};
use crate::json::{J, Obj};
use crate::refs::Tree;

/// Largest file read (the JSON reader refuses more than 32 MiB anyway).
const MAX_READ: u64 = 64 * 1024 * 1024;

pub struct Workspace {
    pub project: Project,
    pub registry: Registry,
    indexed: bool,
}

/// Where a scene (or prefab) document lives.
#[derive(Clone, Debug)]
pub struct Located {
    pub id: String,
    /// Its path in the project, with `/` separators.
    pub rel: String,
    pub kind: Kind,
    pub path: PathBuf,
}

/// A document as read: its bytes (for hashes and undo) and its value.
pub struct SceneFile {
    pub bytes: Vec<u8>,
    pub root: Value,
}

pub fn sha256(bytes: &[u8]) -> String {
    digest::hex(&digest::sha256(bytes))
}

/// Reads a whole file; a missing one is `not-found`, anything else `io`.
pub fn read_file(path: &Path) -> Result<Vec<u8>, Error> {
    let file = std::fs::File::open(path).map_err(|e| {
        let m = format!("{}: {e}", path.display());
        if e.kind() == std::io::ErrorKind::NotFound {
            error::not_found(m)
        } else {
            error::io(m)
        }
    })?;
    read_all(file, &path.display().to_string())
}

/// Reads at most `MAX_READ` bytes from `from`.
pub fn read_all(from: impl Read, what: &str) -> Result<Vec<u8>, Error> {
    let mut bytes = vec![];
    from.take(MAX_READ + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| error::io(format!("{what}: {e}")))?;
    if bytes.len() as u64 > MAX_READ {
        return Err(error::io(format!("{what} is larger than {MAX_READ} bytes")));
    }
    Ok(bytes)
}

pub fn open(dir: &str) -> Result<Workspace, Error> {
    let root = Path::new(dir);
    if !root.is_dir() {
        return Err(error::not_found(format!("no project folder {dir:?}")));
    }
    let manifest = read_file(&root.join(MANIFEST)).map_err(|e| {
        Error::new(
            e.code,
            format!("{dir}: no readable {MANIFEST} ({})", e.message),
        )
    })?;
    let value = json::parse(&manifest).map_err(|e| error::invalid(format!("{MANIFEST}: {e}")))?;
    Manifest::parse(&value).map_err(|e| error::invalid(format!("{MANIFEST}: {e}")))?;
    let project = Project::open(root).map_err(error::invalid)?;
    let mut registry = engine_registry();
    let schemas = project.schema_files().map_err(error::invalid)?;
    for (path, file) in schemas {
        registry
            .add(&file, false)
            .map_err(|e| error::invalid(format!("{path}: {e}")))?;
    }
    Ok(Workspace {
        project,
        registry,
        indexed: false,
    })
}

impl Workspace {
    /// Indexes the project's assets, once: finding a document by id, checking references and
    /// cooking need it. Scanning reads every engine document, so commands that only need one
    /// document skip it.
    pub fn index(&mut self) -> Result<(), Error> {
        if !self.indexed {
            self.project.scan(None).map_err(error::invalid)?;
            self.indexed = true;
        }
        Ok(())
    }

    /// The scene or prefab `arg` names: its asset id, its path in the project, or a file path
    /// that lies inside the project.
    pub fn locate(&mut self, arg: &str) -> Result<Located, Error> {
        let missing = || {
            error::not_found(format!(
                "no scene {arg:?} in the project; give its asset id or its path in the project"
            ))
        };
        let (rel, kind) = if is_asset_id(arg) {
            self.index()?;
            let entry = self.project.entry(arg).ok_or_else(missing)?;
            (entry.path.clone(), entry.kind)
        } else {
            let rel = self.relative(arg).ok_or_else(missing)?;
            let in_content = self
                .project
                .manifest
                .content
                .iter()
                .any(|c| rel.starts_with(&format!("{c}/")));
            if !in_content {
                return Err(error::not_found(format!(
                    "{rel} is not in one of the project's content folders ({})",
                    self.project.manifest.content.join(", ")
                )));
            }
            let kind = Kind::of_file(&rel).ok_or_else(missing)?;
            (rel, kind)
        };
        if !matches!(kind, Kind::Scene | Kind::Prefab) {
            return Err(error::invalid(format!(
                "{rel} is a {}, not a scene or prefab",
                kind.name()
            )));
        }
        let path = self.project.root.join(&rel);
        let doc = parse_document(&read_file(&path)?, &rel)?;
        let id = doc
            .get("id")
            .and_then(|v| v.string().ok())
            .filter(|id| is_asset_id(id))
            .ok_or_else(|| error::invalid(format!("{rel} has no valid \"id\"")))?
            .to_string();
        Ok(Located {
            id,
            rel,
            kind,
            path,
        })
    }

    /// `arg` as a path in the project, `/`-separated: given relative to the project, or as a
    /// file path (absolute, or relative to the working directory) inside it.
    fn relative(&self, arg: &str) -> Option<String> {
        let root = self.project.root.canonicalize().ok()?;
        let inside = |p: &Path| -> Option<String> {
            let abs = p.canonicalize().ok()?;
            let rel = abs.strip_prefix(&root).ok()?;
            let parts: Option<Vec<&str>> =
                rel.components().map(|c| c.as_os_str().to_str()).collect();
            Some(parts?.join("/")).filter(|r| !r.is_empty())
        };
        inside(&self.project.root.join(arg.replace('\\', "/"))).or_else(|| inside(Path::new(arg)))
    }

    pub fn read(&self, at: &Located) -> Result<SceneFile, Error> {
        let bytes = read_file(&at.path)?;
        let root = parse_document(&bytes, &at.rel)?;
        Ok(SceneFile { bytes, root })
    }

    /// A prefab by asset id, for `scene::flatten`. A prefab with a parent cycle reads as
    /// missing: expanding it would never end.
    pub fn prefab(&self, id: &str) -> Option<Value> {
        let v = self.project.read_document(id).ok()?;
        (!Tree::new(&v).has_cycle()).then_some(v)
    }

    /// `component` of `kind` with every field it leaves out at its schema default.
    pub fn with_defaults(&self, kind: &str, component: &Value) -> Value {
        match self.registry.default_component(kind) {
            Some(mut full) => {
                scene::merge(&mut full, component);
                full
            }
            None => component.clone(),
        }
    }
}

/// A scene or prefab document from its text.
pub fn parse_document(bytes: &[u8], what: &str) -> Result<Value, Error> {
    let root = json::parse(bytes).map_err(|e| error::invalid(format!("{what}: {e}")))?;
    if scene::kind(&root).is_none() {
        return Err(error::invalid(format!(
            "{what} is not a scene or prefab document (\"abe\": \"scene\" or \"prefab\")"
        )));
    }
    Ok(root)
}

/// The scene, as answers name it.
pub fn about(at: &Located, root: &Value) -> J {
    Obj::new()
        .with("id", at.id.as_str())
        .with("name", root.get("name").map(J::from))
        .with("path", at.rel.as_str())
        .with("kind", at.kind.name())
        .into()
}
