//! `ganbate render`: the scene cooked in memory as `ganbate-build cook-scene` would write it, then drawn
//! by a [`Backend`] the `ganbate` binary supplies (the renderer lives in the engine crate, which
//! depends on this one) exactly as `ganbate-render` draws a cooked file.
//!
//! The view is the `--camera` entity, else the scene's first camera entity (in tree order),
//! else the Look file's first view, else an overview of the world's bounds (the last two as
//! `ganbate-render` chooses). A camera looks along its world transform's -Z with +Y up; its target
//! lies at its focus distance when depth of field is on, otherwise at the depth of the world's
//! centre along that axis (the renderer fits shadows and the ground map around the target).
//! Time, weather and Look come from the scene's `environment` component (the first by entity
//! id, as Ganbate Studio picks it); `--time` and `--weather` override them, and without either the
//! Look file's own apply.
use ganbate_core::json::Value;
use ganbate_core::world::{EntityTable, World};
use ganbate_pipeline::project::Kind;
use ganbate_scene::scene;

use crate::Reply;
use crate::args::Args;
use crate::error::{self, Error};
use crate::json::{J, Obj};
use crate::refs::{self, Tree};
use crate::validate;
use crate::workspace::{self, Located, Workspace, about};

/// `ganbate-render`'s defaults.
pub const WIDTH: u32 = 1920;
pub const HEIGHT: u32 = 1080;
pub const SAMPLES: u32 = 8;
/// Largest image side (the largest texture the world format allows).
pub const MAX_SIDE: u32 = 16_384;

/// Depth of field: nothing nearer than `distance` blurs; the blur reaches `blur` (a share of
/// the image height) `depth` metres further on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Focus {
    pub distance: f32,
    pub depth: f32,
    pub blur: f32,
}

/// A camera as the renderer's `View` takes it.
#[derive(Clone, Debug, PartialEq)]
pub struct Shot {
    pub name: String,
    pub eye: [f32; 3],
    pub target: [f32; 3],
    pub up: [f32; 3],
    /// Vertical field of view in degrees.
    pub fov: f32,
    pub near: f32,
    pub shift: [f32; 2],
    pub focus: Option<Focus>,
}

/// One still to draw.
pub struct Still<'a> {
    /// The cooked world as GBEW bytes (what `ganbate-build cook-scene` writes).
    pub world: &'a [u8],
    /// The Look file's bytes, if the scene names one.
    pub look: Option<&'a [u8]>,
    /// The camera; `None`: the Look file's first view, else an overview.
    pub shot: Option<&'a Shot>,
    pub time: Option<f32>,
    pub weather: Option<&'a str>,
    pub width: u32,
    pub height: u32,
    pub samples: u32,
}

/// What a backend drew.
pub struct Rendered {
    pub png: Vec<u8>,
    /// The device's name, for the record of which driver produced the image.
    pub device: String,
    /// The view used, and where it came from: `camera`, `look` or `overview`.
    pub view: Shot,
    pub source: String,
    /// The hour drawn.
    pub time: f32,
}

/// Draws stills; the `ganbate` binary implements it with the engine's offscreen renderer.
pub trait Backend {
    fn render(&self, still: &Still<'_>) -> Result<Rendered, String>;
}

fn shot_json(s: &Shot) -> Obj {
    let mut o = Obj::new()
        .with("name", s.name.as_str())
        .with("eye", s.eye)
        .with("target", s.target)
        .with("up", s.up)
        .with("fov", s.fov);
    if let Some(f) = s.focus {
        o.put(
            "focus",
            Obj::new()
                .with("distance", f.distance)
                .with("depth", f.depth)
                .with("blur", f.blur),
        );
    }
    o
}

fn size(v: Option<&str>) -> Result<(u32, u32), Error> {
    let Some(v) = v else {
        return Ok((WIDTH, HEIGHT));
    };
    let bad = || {
        error::usage(format!(
            "--size is WIDTHxHEIGHT (such as 640x360), not {v:?}"
        ))
    };
    let (w, h) = v.split_once(['x', 'X']).ok_or_else(bad)?;
    let (w, h): (u32, u32) = (w.parse().map_err(|_| bad())?, h.parse().map_err(|_| bad())?);
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        return Err(error::usage(format!(
            "--size: each side is 1 to {MAX_SIDE} pixels"
        )));
    }
    Ok((w, h))
}

fn number(v: &Value, key: &str, default: f32) -> f32 {
    match v.get(key) {
        Some(Value::Number(n)) if n.is_finite() => *n as f32,
        _ => default,
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn unit(v: [f32; 3]) -> Option<[f32; 3]> {
    let n = dot(v, v).sqrt();
    (n > 1e-6 && n.is_finite()).then(|| [v[0] / n, v[1] / n, v[2] / n])
}

/// World bounds of everything the world draws.
fn bounds(w: &World) -> Option<([f32; 3], [f32; 3])> {
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for inst in &w.instances {
        let Some(mesh) = w.meshes.get(inst.mesh as usize) else {
            continue;
        };
        for p in &mesh.positions {
            let q = inst.transform.point(*p);
            for (k, v) in [q.0, q.1, q.2].into_iter().enumerate() {
                lo[k] = lo[k].min(v);
                hi[k] = hi[k].max(v);
            }
        }
    }
    (lo[0] <= hi[0]).then_some((lo, hi))
}

/// The view from camera entity `id`, placed where the cook put it.
fn camera(
    ws: &Workspace,
    tree: &Tree,
    id: &str,
    table: &EntityTable,
    world: &World,
) -> Result<Shot, Error> {
    let stored = tree
        .entity(id)
        .and_then(|e| e.get("components"))
        .and_then(|c| c.get("camera"))
        .ok_or_else(|| error::invalid(format!("{} has no camera component", tree.label(id))))?;
    let cam = ws.with_defaults("camera", stored);
    let key = u64::from_str_radix(id, 16)
        .map_err(|_| error::invalid(format!("{id} is not an entity id")))?;
    let record = table.entities.iter().find(|r| r.id == key).ok_or_else(|| {
        error::cook(format!(
            "{} is not in the cooked entity table",
            tree.label(id)
        ))
    })?;
    let m = record.transform.0;
    let eye = [m[12], m[13], m[14]];
    let forward = unit([-m[8], -m[9], -m[10]]).ok_or_else(|| {
        error::invalid(format!(
            "{}'s transform has no direction to look in",
            tree.label(id)
        ))
    })?;
    let up = unit([m[4], m[5], m[6]]).unwrap_or([0.0, 1.0, 0.0]);
    let focus = match cam.get("focus") {
        Some(f) if f.get("enabled") == Some(&Value::Bool(true)) => Some(Focus {
            distance: number(f, "distance", 0.0),
            depth: number(f, "depth", 4.0),
            blur: number(f, "blur", 0.006),
        }),
        _ => None,
    };
    let reach = match (focus.filter(|f| f.distance > 0.0), bounds(world)) {
        (Some(f), _) => f.distance,
        (None, Some((lo, hi))) => {
            let centre = [
                (lo[0] + hi[0]) * 0.5,
                (lo[1] + hi[1]) * 0.5,
                (lo[2] + hi[2]) * 0.5,
            ];
            dot(sub(centre, eye), forward).max(1.0)
        }
        (None, None) => 5.0,
    };
    let shift = match cam.get("shift") {
        Some(Value::Array(a)) if a.len() == 2 => {
            let n = |x: &Value| {
                if let Value::Number(n) = x {
                    *n as f32
                } else {
                    0.0
                }
            };
            [n(&a[0]), n(&a[1])]
        }
        _ => [0.0, 0.0],
    };
    Ok(Shot {
        name: tree.name(id).to_string(),
        eye,
        target: [
            eye[0] + forward[0] * reach,
            eye[1] + forward[1] * reach,
            eye[2] + forward[2] * reach,
        ],
        up,
        fov: number(&cam, "fov", 50.0),
        near: number(&cam, "near", 0.08),
        shift,
        focus,
    })
}

pub(crate) fn run(
    ws: &mut Workspace,
    at: &Located,
    args: &Args,
    backend: Option<&dyn Backend>,
) -> Result<Reply, Error> {
    let out = args
        .value("--out")?
        .ok_or_else(|| error::usage("ganbate render needs --out FILE.png"))?
        .to_string();
    if !out.to_ascii_lowercase().ends_with(".png") {
        return Err(error::usage(format!(
            "--out names a .png file (it is written as PNG), not {out:?}"
        )));
    }
    let (width, height) = size(args.value("--size")?)?;
    let samples = match args.value("--samples")? {
        None => SAMPLES,
        Some(s) => match s.parse::<u32>() {
            Ok(n) if (1..=256).contains(&n) => n,
            _ => return Err(error::usage(format!("--samples is 1 to 256, not {s:?}"))),
        },
    };
    let time = match args.value("--time")? {
        None => None,
        Some(t) => match t.parse::<f32>() {
            Ok(h) if (0.0..=24.0).contains(&h) => Some(h),
            _ => {
                return Err(error::usage(format!(
                    "--time is an hour from 0 to 24, not {t:?}"
                )));
            }
        },
    };
    let weathers: Vec<String> = match ws
        .registry
        .get("environment")
        .and_then(|c| c.fields.get("weather"))
    {
        Some(f) => match &f.ty {
            ganbate_scene::schema::FieldType::Enum(v) => v.clone(),
            _ => vec![],
        },
        None => vec![],
    };
    let weather_flag = args.value("--weather")?;
    if let Some(w) = weather_flag {
        if !weathers.is_empty() && !weathers.iter().any(|x| x == w) {
            return Err(error::usage(format!(
                "--weather is one of {}, not {w:?}",
                weathers.join(", ")
            )));
        }
    }
    let backend = backend.ok_or_else(|| {
        error::render("this entry point has no renderer; run the ganbate binary to render")
    })?;

    ws.index()?;
    let ws = &*ws;
    let file = ws.read(at)?;
    let root = &file.root;
    let errors = scene::check(root, &ws.registry);
    let tree = Tree::new(root);
    if !errors.is_empty() {
        let issues: Vec<J> = errors
            .iter()
            .map(|i| validate::issue(&tree, Some("error"), &i.path, &i.message).into())
            .collect();
        return Err(error::invalid(format!(
            "{} has {} error(s); fix them before rendering",
            at.rel,
            errors.len()
        ))
        .with("issues", issues));
    }
    // As `cook_scene`, with prefabs read through a loader that refuses parent cycles.
    let resolved = scene::flatten(root, &|id| ws.prefab(id)).map_err(error::cook)?;
    let cooked =
        ganbate_pipeline::cook::cook_resolved(&ws.project, &ws.registry, root, &resolved, false)
            .map_err(error::cook)?;
    let world_bytes = ganbate_assets::cooked::encode_scene(&cooked.world, &cooked.table, false)
        .map_err(error::cook)?;
    let mut warnings: Vec<String> = cooked.warnings.clone();

    // The scene's environment: the first entity (by id) that has one.
    let env = scene::entities(root).values().find_map(|e| {
        e.get("components")
            .and_then(|c| c.get("environment"))
            .map(|c| ws.with_defaults("environment", c))
    });
    let env_time = env.as_ref().and_then(|e| match e.get("time") {
        Some(Value::Number(n)) => Some(*n as f32),
        _ => None,
    });
    let env_weather = env.as_ref().and_then(|e| {
        e.get("weather")
            .and_then(|w| w.string().ok())
            .map(String::from)
    });
    let weather = weather_flag.map(String::from).or(env_weather);
    let mut look_rel = None;
    let mut look_bytes = None;
    if let Some(look) = env
        .as_ref()
        .and_then(|e| e.get("look"))
        .and_then(|l| l.string().ok())
    {
        match ws.project.entry(look) {
            Some(entry) if entry.kind == Kind::Look => {
                look_bytes = Some(workspace::read_file(&ws.project.root.join(&entry.path))?);
                look_rel = Some(entry.path.clone());
            }
            Some(entry) => warnings.push(format!(
                "the environment's look {look} is a {} ({}); drawing with the default Look",
                entry.kind.name(),
                entry.path
            )),
            None => warnings.push(format!(
                "the environment's look {look} is not in the project; drawing with the default Look"
            )),
        }
    }

    let shot = match args.value("--camera")? {
        Some(r) => {
            let id = refs::resolve(&tree, r)?;
            Some((camera(ws, &tree, &id, &cooked.table, &cooked.world)?, id))
        }
        None => {
            let starts = tree.children(None).to_vec();
            let first = tree.walk(&starts, None, true).into_iter().find(|(id, _)| {
                tree.entity(id)
                    .and_then(|e| e.get("components"))
                    .and_then(|c| c.get("camera"))
                    .is_some()
            });
            match first {
                Some((id, _)) => Some((
                    camera(ws, &tree, id, &cooked.table, &cooked.world)?,
                    id.to_string(),
                )),
                None => None,
            }
        }
    };
    let still = Still {
        world: &world_bytes,
        look: look_bytes.as_deref(),
        shot: shot.as_ref().map(|(s, _)| s),
        time: time.or(env_time),
        weather: weather.as_deref(),
        width,
        height,
        samples,
    };
    let drawn = backend.render(&still).map_err(error::render)?;
    let path = std::path::Path::new(&out);
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| error::io(format!("{}: {e}", dir.display())))?;
    }
    ganbate_assets::write_asset(path, &drawn.png).map_err(|e| error::io(format!("{out}: {e}")))?;

    let mut view = shot_json(&drawn.view).with("source", drawn.source.as_str());
    if let Some((_, id)) = &shot {
        view.put("entity", id.as_str());
    }
    let body = Obj::new()
        .with("scene", about(at, root))
        .with("file", out.as_str())
        .with("bytes", drawn.png.len())
        .with("sha256", workspace::sha256(&drawn.png))
        .with("width", width)
        .with("height", height)
        .with("samples", samples)
        .with("view", view)
        .with("time", drawn.time)
        .with("weather", weather)
        .with("look", look_rel)
        .with("entities", cooked.table.entities.len())
        .with("device", drawn.device.as_str())
        .with("warnings", warnings);
    Ok(Reply::ok(body))
}
