//! Ganbate's content pipeline: a project folder and its asset database (`project`), importers that
//! turn source files into documents without losing what they are made of, and the cooker that
//! turns documents into what the runtime loads. Native tools only (it reads and writes files).
pub mod cook;
pub mod project;
