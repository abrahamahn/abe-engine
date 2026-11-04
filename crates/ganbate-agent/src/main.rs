//! `ganbate`: Ganbate's command line for agents (and people). It inspects, validates, edits, diffs,
//! undoes and renders scenes and answers in one line of JSON; `ganbate help` lists the commands and
//! the README's "For agents: the ganbate command" shows each with an example.
//!
//! The commands live in `ganbate_agent`; this binary adds the offscreen renderer, which it drives
//! exactly as `ganbate-render` does for a cooked file: the same quality preset, defaults,
//! environment resolution and fallback views.
use std::io::Write;
use std::panic::AssertUnwindSafe;
use std::time::Duration;

use ganbate_agent::render::{Backend, Focus, Rendered, Shot, Still};
use ganbate_engine::gpu::Device;
use ganbate_engine::render::{
    self, Environment, LookFile, Output, Quality, Renderer, View, Weather,
};

/// `ganbate-render`'s animation clock for stills (water, clouds and weather are at this second).
const SECONDS: f32 = 12.0;

struct Offscreen;

fn view_of(s: &Shot) -> View {
    View {
        name: s.name.clone(),
        eye: s.eye,
        target: s.target,
        fov: s.fov,
        up: s.up,
        shift: s.shift,
        near: s.near,
        focus: s.focus.map(|f| render::Focus {
            distance: f.distance,
            depth: f.depth,
            blur: f.blur,
        }),
    }
}

fn shot_of(v: &View) -> Shot {
    Shot {
        name: v.name.clone(),
        eye: v.eye,
        target: v.target,
        up: v.up,
        fov: v.fov,
        near: v.near,
        shift: v.shift,
        focus: v.focus.map(|f| Focus {
            distance: f.distance,
            depth: f.depth,
            blur: f.blur,
        }),
    }
}

impl Backend for Offscreen {
    fn render(&self, s: &Still<'_>) -> Result<Rendered, String> {
        let world = ganbate_engine::assets::cooked::decode(s.world)?;
        let mut file = match s.look {
            Some(bytes) => LookFile::parse(bytes)?,
            None => LookFile::default(),
        };
        if let Some(h) = s.time {
            file.time.hours = h;
        }
        if let Some(w) = s.weather {
            file.weather = Weather::preset(w)
                .ok_or_else(|| format!("unknown weather {w:?}; try {:?}", Weather::PRESETS))?;
        }
        let mut device = Device::headless()?;
        // Offline stills on slow (software) devices can take minutes per submission.
        device.set_submit_timeout(Duration::from_secs(1800));
        let mut renderer = Renderer::new(
            &mut device,
            &world,
            file.look.clone(),
            Quality::still(),
            (s.width, s.height),
            Output::Still,
        )?;
        let (view, source) = match s.shot {
            Some(shot) => (view_of(shot), "camera"),
            None => match file.views.first() {
                Some(v) => (v.clone(), "look"),
                None => {
                    let (lo, hi) = renderer.bounds();
                    let c = [
                        (lo[0] + hi[0]) * 0.5,
                        (lo[1] + hi[1]) * 0.5,
                        (lo[2] + hi[2]) * 0.5,
                    ];
                    let r = ((hi[0] - lo[0]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt() * 0.4;
                    let overview = View {
                        name: "overview".into(),
                        eye: [c[0] + r, c[1] + r * 0.6, c[2] + r],
                        target: c,
                        fov: 45.0,
                        ..View::default()
                    };
                    (overview, "overview")
                }
            },
        };
        let env = Environment::resolve(&file.time, &file.weather, &file.look.palette, SECONDS);
        let pixels = renderer.render_still(&mut device, &view, &env, s.samples)?;
        let png = ganbate_engine::assets::png::encode(s.width, s.height, &pixels)?;
        let errors = device.validation_errors();
        if let Some(first) = errors.first() {
            return Err(format!(
                "{} validation errors, first: {first}",
                errors.len()
            ));
        }
        Ok(Rendered {
            png,
            device: device.name().to_string(),
            view: shot_of(&view),
            source: source.into(),
            time: file.time.hours,
        })
    }
}

fn main() {
    let args: Result<Vec<String>, _> = std::env::args_os()
        .skip(1)
        .map(|a| a.into_string())
        .collect();
    let outcome = match args {
        Ok(args) => {
            if args.first().map(String::as_str) == Some("mcp")
                && !args.iter().any(|a| a == "--help" || a == "-h")
            {
                let code = ganbate_agent::run_mcp_stdio(
                    &args[1..],
                    &mut std::io::stdin().lock(),
                    &mut std::io::stdout().lock(),
                );
                std::process::exit(code);
            }
            let pretty = args.iter().any(|a| a == "--pretty");
            std::panic::catch_unwind(AssertUnwindSafe(|| {
                ganbate_agent::run(&args, &mut std::io::stdin().lock(), Some(&Offscreen))
            }))
            .unwrap_or_else(|_| {
                ganbate_agent::failure(
                    ganbate_agent::Error::new(
                        "internal",
                        "ganbate failed unexpectedly (a bug; see stderr)",
                    ),
                    pretty,
                )
            })
        }
        Err(_) => ganbate_agent::failure(
            ganbate_agent::error::usage("every argument must be valid UTF-8"),
            false,
        ),
    };
    let mut out = std::io::stdout().lock();
    // A reader that went away (ganbate ... | head) is not an error worth a panic.
    let _ = writeln!(out, "{}", outcome.stdout).and_then(|()| out.flush());
    std::process::exit(outcome.status);
}
