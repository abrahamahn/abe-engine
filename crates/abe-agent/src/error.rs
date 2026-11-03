//! Failures with stable codes, so an agent can branch on `error.code` and read the rest.
use crate::json::{J, Obj};

/// A reference names no entity, a scene is not in the project, a file is missing, nothing to undo.
pub const NOT_FOUND: &str = "not-found";
/// A name or path names several entities (`candidates` lists them).
pub const AMBIGUOUS: &str = "ambiguous-ref";
/// An edit is malformed or cannot be done as asked (`edit` says which).
pub const BAD_EDIT: &str = "bad-edit";
/// The document breaks the schemas or the scene rules (`issues` lists the problems).
pub const INVALID: &str = "invalid";
/// The document changed since the transaction an undo would take back (or during a write).
pub const CHANGED: &str = "changed-since";
/// Reading or writing a file failed.
pub const IO: &str = "io";
/// Cooking the scene failed.
pub const COOK: &str = "cook";
/// Rendering failed (no Vulkan driver, a device error, an image that cannot be encoded).
pub const RENDER: &str = "render";
/// The command line is wrong (exit code 2).
pub const USAGE: &str = "usage";

pub const CODES: [&str; 9] = [
    NOT_FOUND, AMBIGUOUS, BAD_EDIT, INVALID, CHANGED, IO, COOK, RENDER, USAGE,
];

#[derive(Clone, Debug, PartialEq)]
pub struct Error {
    pub code: &'static str,
    pub message: String,
    /// More about the failure: the edit's index, the issues, the candidates...
    pub detail: Obj,
}

impl Error {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            detail: Obj::new(),
        }
    }
    pub fn with(mut self, key: &str, value: impl Into<J>) -> Self {
        self.detail.put(key, value);
        self
    }
    pub fn json(&self) -> J {
        let mut o = Obj::new()
            .with("code", self.code)
            .with("message", self.message.as_str());
        o.extend(self.detail.clone());
        o.into()
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

pub fn not_found(m: impl Into<String>) -> Error {
    Error::new(NOT_FOUND, m)
}
pub fn ambiguous(m: impl Into<String>) -> Error {
    Error::new(AMBIGUOUS, m)
}
pub fn bad_edit(m: impl Into<String>) -> Error {
    Error::new(BAD_EDIT, m)
}
pub fn invalid(m: impl Into<String>) -> Error {
    Error::new(INVALID, m)
}
pub fn changed(m: impl Into<String>) -> Error {
    Error::new(CHANGED, m)
}
pub fn io(m: impl Into<String>) -> Error {
    Error::new(IO, m)
}
pub fn cook(m: impl Into<String>) -> Error {
    Error::new(COOK, m)
}
pub fn render(m: impl Into<String>) -> Error {
    Error::new(RENDER, m)
}
pub fn usage(m: impl Into<String>) -> Error {
    Error::new(USAGE, m)
}
