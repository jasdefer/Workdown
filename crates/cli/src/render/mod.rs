//! Markdown renderers.
//!
//! Each submodule turns a `view_data::*Data` intermediate into a single
//! Markdown string. Renderers are pure: no filesystem access, no config
//! loading. The `workdown render` command is the orchestrator that reads
//! `config.yaml` + `views.yaml`, calls the extractors, calls the renderers,
//! and writes `views/<id>.md`.

pub mod bar_chart;
pub mod board;
pub mod chart_common;
pub mod common;
pub mod description;
pub mod gantt;
pub mod gantt_by_depth;
pub mod gantt_by_initiative;
pub mod gantt_common;
pub mod graph;
pub mod line_chart;
pub mod metric;
pub mod table;
pub mod tree;
pub mod treemap;
