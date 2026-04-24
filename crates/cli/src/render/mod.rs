//! Markdown renderers.
//!
//! Each submodule turns a `view_data::*Data` intermediate into a single
//! Markdown string. Renderers are pure: no filesystem access, no config
//! loading. The `workdown render` command is the orchestrator that reads
//! `config.yaml` + `views.yaml`, calls the extractors, calls the renderers,
//! and writes `views/<id>.md`.

pub mod board;
pub mod common;
pub mod tree;
