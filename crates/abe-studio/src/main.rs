//! `abe-studio PROJECT [--port N] [--lan] [--new NAME] [--editor DIR] [--engine DIR]`
//!
//! Serves ABE Studio for the project folder PROJECT (creating it with `--new`). Open
//! `http://localhost:PORT/` in a browser. With `--lan` the host also listens on the local
//! network and makes a pairing token that every client must present, phones and running games
//! as well as this machine's browser: the addresses it prints carry it (`?token=`).
use std::net::TcpListener;
use std::path::PathBuf;

use abe_core::id::Ids;
use abe_pipeline::project::Project;
use abe_studio::server::{Config, Limits, new_token, serve};
use abe_studio::session::Studio;

fn main() {
    if let Err(e) = run() {
        eprintln!("abe-studio: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mut project: Option<PathBuf> = None;
    let mut port = 7480u16;
    let mut lan = false;
    let mut new_name: Option<String> = None;
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut editor = here.join("../../editor");
    let mut engine = here.join("../../web/dist");
    while let Some(a) = args.next() {
        let mut value = |what: &str| args.next().ok_or(format!("{a} needs {what}"));
        match a.as_str() {
            "--port" => {
                port = value("a port")?
                    .parse()
                    .map_err(|_| "--port needs a number")?
            }
            "--lan" => lan = true,
            "--new" => new_name = Some(value("a project name")?),
            "--editor" => editor = PathBuf::from(value("a folder")?),
            "--engine" => engine = PathBuf::from(value("a folder")?),
            "-h" | "--help" => {
                println!(
                    "usage: abe-studio PROJECT [--port N] [--lan] [--new NAME] [--editor DIR] [--engine DIR]"
                );
                return Ok(());
            }
            p if !p.starts_with('-') && project.is_none() => project = Some(PathBuf::from(p)),
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    let root = project.ok_or("name a project folder")?;
    let mut ids = Ids::from_os();
    if let Some(name) = new_name {
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        Project::create(&root, &name, &mut ids)?;
        println!("abe-studio: created project {name:?} in {}", root.display());
    }
    if !engine.join("abe.js").exists() {
        eprintln!(
            "abe-studio: no web runtime in {} (run scripts/build-web.sh); the viewport will not start",
            engine.display()
        );
    }
    let token = lan.then(new_token);
    let (studio, notes) = Studio::open(&root, ids)?;
    for n in notes {
        println!("abe-studio: {n}");
    }
    let host = if lan { "0.0.0.0" } else { "127.0.0.1" };
    let listener = TcpListener::bind((host, port))
        .map_err(|e| format!("cannot listen on {host}:{port}: {e}"))?;
    // The port asked for may be 0 (any free one).
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let query = token
        .as_ref()
        .map_or(String::new(), |t| format!("?token={t}"));
    println!(
        "abe-studio: {} at http://localhost:{port}/{query}",
        studio.project.manifest.name
    );
    if token.is_some() {
        println!("abe-studio: on the local network, open http://<this machine>:{port}/{query}");
    }
    serve(
        studio,
        listener,
        Config {
            editor,
            engine,
            token,
            limits: Limits::default(),
        },
    )
}
