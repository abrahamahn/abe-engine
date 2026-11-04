//! Transactions the CLI wrote, so `ganbate scene undo` can take the last one back. One file per
//! document, `PROJECT/.ganbate/history/<scene id>.jsonl` (`.ganbate/` is local and ignored by git),
//! one line per transaction, newest last:
//!
//! ```json
//! {"v":1,"label":"...","edits":2,"before":"<sha256>","after":"<sha256>","ops":[...],"undo":[...]}
//! ```
//!
//! `before` and `after` hash the document's file bytes around the transaction; ops are in the
//! edit protocol's form (`{"set": [path...], "value": v}` or `{"remove": [path...]}`), and
//! `undo` holds the inverse ops in the order they apply. When the file was not canonical text
//! before the transaction (written by hand), `text` keeps it, so undo gives back those exact
//! bytes. A lock file beside it keeps two commands from writing one document at once.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ganbate_core::json::{self as core_json, Value};
use ganbate_scene::doc::Op;

use crate::error::{self, Error};
use crate::json::{self, J, Obj};
use crate::workspace;

/// Transactions kept per document (as many as Ganbate Studio's undo history keeps).
pub const LIMIT: usize = 512;
/// How long a command waits for another to finish with a document.
const WAIT: Duration = Duration::from_secs(10);
/// A lock older than this was left by a command that died (the write it guards takes
/// milliseconds).
const STALE: Duration = Duration::from_secs(30);

pub fn dir(project_root: &Path) -> PathBuf {
    project_root.join(".ganbate").join("history")
}

pub fn file(project_root: &Path, scene_id: &str) -> PathBuf {
    dir(project_root).join(format!("{scene_id}.jsonl"))
}

#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub label: String,
    pub edits: usize,
    pub before: String,
    pub after: String,
    pub ops: Vec<Op>,
    pub undo: Vec<Op>,
    pub text: Option<String>,
}

/// An op in the edit protocol's form (as `ganbate_studio::session::op_to_json` writes it).
pub fn op_json(op: &Op) -> J {
    let path = |p: &[String]| J::Arr(p.iter().map(J::from).collect());
    match op {
        Op::Set { path: p, value } => Obj::new().with("set", path(p)).with("value", value).into(),
        Op::Remove { path: p } => Obj::new().with("remove", path(p)).into(),
    }
}

/// An op from the edit protocol's form (as `ganbate_studio::session::op_from_json` reads it).
pub fn op_from(v: &Value) -> Result<Op, String> {
    let path = |p: &Value| -> Result<Vec<String>, String> {
        let a = p.array()?;
        if a.is_empty() || a.len() > 64 {
            return Err("an op's path has 1 to 64 keys".into());
        }
        a.iter().map(|k| k.string().map(String::from)).collect()
    };
    match (v.get("set"), v.get("remove")) {
        (Some(p), None) => Ok(Op::Set {
            path: path(p)?,
            value: v.get("value").cloned().ok_or("a set op needs a value")?,
        }),
        (None, Some(p)) => Ok(Op::Remove { path: path(p)? }),
        _ => Err("an op is {\"set\": path, \"value\": v} or {\"remove\": path}".into()),
    }
}

impl Record {
    pub fn line(&self) -> String {
        let ops = |list: &[Op]| J::Arr(list.iter().map(op_json).collect());
        let mut o = Obj::new()
            .with("v", 1u32)
            .with("label", self.label.as_str())
            .with("edits", self.edits)
            .with("before", self.before.as_str())
            .with("after", self.after.as_str())
            .with("ops", ops(&self.ops))
            .with("undo", ops(&self.undo));
        if let Some(t) = &self.text {
            o.put("text", t.as_str());
        }
        json::write(&o.into(), false)
    }

    pub fn parse(line: &str) -> Result<Self, String> {
        let v = core_json::parse(line.as_bytes())?;
        if v.get("v").and_then(|x| x.unsigned().ok()) != Some(1) {
            return Err("not a version 1 record".into());
        }
        let text = |k: &str| -> Result<String, String> {
            v.get(k)
                .and_then(|x| x.string().ok())
                .map(String::from)
                .ok_or_else(|| format!("no {k}"))
        };
        let ops = |k: &str| -> Result<Vec<Op>, String> {
            v.get(k)
                .ok_or_else(|| format!("no {k}"))?
                .array()?
                .iter()
                .map(op_from)
                .collect()
        };
        Ok(Self {
            label: text("label")?,
            edits: v.get("edits").and_then(|x| x.integer().ok()).unwrap_or(0),
            before: text("before")?,
            after: text("after")?,
            ops: ops("ops")?,
            undo: ops("undo")?,
            text: v
                .get("text")
                .and_then(|x| x.string().ok())
                .map(String::from),
        })
    }
}

/// The history's lines, oldest first (none when there is no file).
pub fn read(path: &Path) -> Result<Vec<String>, Error> {
    match workspace::read_file(path) {
        Ok(bytes) => {
            let text = String::from_utf8(bytes)
                .map_err(|_| error::invalid(format!("{} is not UTF-8", path.display())))?;
            Ok(text
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(String::from)
                .collect())
        }
        Err(e) if e.code == error::NOT_FOUND => Ok(vec![]),
        Err(e) => Err(e),
    }
}

/// Replaces the history with `lines` (keeping the newest `LIMIT`), or removes it when empty.
pub fn write(path: &Path, lines: &[String]) -> Result<(), Error> {
    if lines.is_empty() {
        return match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(error::io(format!("{}: {e}", path.display())))
            }
            _ => Ok(()),
        };
    }
    let keep = &lines[lines.len().saturating_sub(LIMIT)..];
    let mut text = keep.join("\n");
    text.push('\n');
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| error::io(format!("{}: {e}", d.display())))?;
    }
    ganbate_assets::write_asset(path, text.as_bytes())
        .map_err(|e| error::io(format!("{}: {e}", path.display())))
}

/// Held while a command reads, changes and writes one document and its history.
pub struct Lock(PathBuf);

impl Lock {
    pub fn take(project_root: &Path, scene_id: &str) -> Result<Self, Error> {
        let d = dir(project_root);
        std::fs::create_dir_all(&d).map_err(|e| error::io(format!("{}: {e}", d.display())))?;
        let path = d.join(format!("{scene_id}.lock"));
        let start = Instant::now();
        loop {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut f) => {
                    let _ = writeln!(f, "{}", std::process::id());
                    return Ok(Self(path));
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    let age = std::fs::metadata(&path)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.elapsed().ok());
                    if age.is_some_and(|a| a > STALE) {
                        let _ = std::fs::remove_file(&path);
                        continue;
                    }
                    if start.elapsed() > WAIT {
                        return Err(error::io(format!(
                            "another ganbate command is changing this document ({} exists); try again",
                            path.display()
                        )));
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(e) => return Err(error::io(format!("{}: {e}", path.display()))),
            }
        }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_read_back() {
        let r = Record {
            label: "Move".into(),
            edits: 1,
            before: "aa".into(),
            after: "bb".into(),
            ops: vec![Op::Set {
                path: vec!["entities".into(), "x".into()],
                value: core_json::parse(br#"{"name":"n","v":[0.1,2]}"#).unwrap(),
            }],
            undo: vec![Op::Remove {
                path: vec!["entities".into(), "x".into()],
            }],
            text: Some("{ \"a\": 1 }\n".into()),
        };
        let line = r.line();
        assert!(!line.contains('\n'));
        assert_eq!(Record::parse(&line).unwrap(), r);
        assert!(Record::parse("{}").is_err());
    }
}
