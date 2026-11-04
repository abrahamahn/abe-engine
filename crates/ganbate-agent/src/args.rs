//! The command line: words (the command and its operands) and options, in any order.
//! `--name value` and `--name=value` are the same; `--` ends the options; `-` is a word.
use std::collections::BTreeMap;

use crate::error::{Error, usage};

/// Options that take a value. `--component` may repeat; the others may not.
const VALUED: [&str; 19] = [
    "--entity",
    "--component",
    "--depth",
    "--out",
    "--camera",
    "--size",
    "--time",
    "--weather",
    "--samples",
    "--kind",
    "--limit",
    "--id",
    "--param",
    "--contact-sheet",
    "--studio",
    "--token",
    "--project",
    "--iterations",
    "--format",
];
const SWITCHES: [&str; 5] = ["--defaults", "--dry-run", "--from-tx", "--pretty", "--help"];

#[derive(Debug, Default)]
pub struct Args {
    pub words: Vec<String>,
    values: BTreeMap<&'static str, Vec<String>>,
    switches: Vec<&'static str>,
}

pub fn parse(args: &[String]) -> Result<Args, Error> {
    let mut out = Args::default();
    let mut it = args.iter();
    let mut options = true;
    while let Some(a) = it.next() {
        if !options || a == "-" || !a.starts_with('-') {
            out.words.push(a.clone());
            continue;
        }
        if a == "--" {
            options = false;
            continue;
        }
        let (name, inline) = match a.split_once('=') {
            Some((n, v)) => (n, Some(v.to_string())),
            None => (a.as_str(), None),
        };
        let name = if name == "-h" { "--help" } else { name };
        if let Some(flag) = SWITCHES.iter().find(|f| **f == name) {
            if inline.is_some() {
                return Err(usage(format!("{name} takes no value")));
            }
            out.switches.push(flag);
            continue;
        }
        let Some(flag) = VALUED.iter().find(|f| **f == name) else {
            return Err(usage(format!("unknown option {a}")));
        };
        let value = match inline {
            Some(v) => v,
            None => it
                .next()
                .cloned()
                .ok_or_else(|| usage(format!("{name} needs a value")))?,
        };
        out.values.entry(flag).or_default().push(value);
    }
    Ok(out)
}

impl Args {
    /// Refuses options the command does not take (`--pretty` and `--help` go with every one).
    pub fn only(&self, allowed: &[&str], command: &str) -> Result<(), Error> {
        let given = self
            .values
            .keys()
            .copied()
            .chain(self.switches.iter().copied());
        for flag in given {
            if !allowed.contains(&flag) && !matches!(flag, "--pretty" | "--help") {
                return Err(usage(format!("{command} does not take {flag}")));
            }
        }
        Ok(())
    }
    pub fn value(&self, flag: &str) -> Result<Option<&str>, Error> {
        match self.values.get(flag).map(Vec::as_slice) {
            None | Some([]) => Ok(None),
            Some([v]) => Ok(Some(v)),
            Some(_) => Err(usage(format!("{flag} is given more than once"))),
        }
    }
    pub fn values(&self, flag: &str) -> &[String] {
        self.values.get(flag).map_or(&[], Vec::as_slice)
    }
    pub fn switch(&self, flag: &str) -> bool {
        self.switches.contains(&flag)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn words_and_options_mix() {
        let a = parse(&strings(&[
            "scene",
            "--depth=2",
            "inspect",
            "p",
            "--component",
            "light",
            "s",
            "--component=mesh",
            "--pretty",
            "--",
            "--odd",
        ]))
        .unwrap();
        assert_eq!(a.words, ["scene", "inspect", "p", "s", "--odd"]);
        assert_eq!(a.value("--depth").unwrap(), Some("2"));
        assert_eq!(a.values("--component"), ["light", "mesh"]);
        assert!(a.switch("--pretty"));
        assert!(a.only(&["--depth", "--component"], "x").is_ok());
        assert!(a.only(&["--depth"], "x").is_err());
        assert!(a.value("--component").is_err());
        for bad in [&["--nope"][..], &["--out"], &["--pretty=1"]] {
            assert_eq!(parse(&strings(bad)).unwrap_err().code, crate::error::USAGE);
        }
    }
}
