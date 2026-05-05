//! Markdown renderers.
//!
//! Each submodule turns a `view_data::*Data` intermediate into a single
//! Markdown string. Renderers are pure: no filesystem access, no config
//! loading. The `workdown render` command is the orchestrator that reads
//! `config.yaml` + `views.yaml`, calls the extractors, calls the renderers,
//! and writes `views/<id>.md`.

pub mod bar_chart;
pub mod board;
pub mod description;
pub mod gantt;
pub mod gantt_by_depth;
pub mod gantt_by_initiative;
pub mod graph;
pub mod heatmap;
pub mod line_chart;
pub mod markdown;
pub mod mermaid_gantt;
pub mod metric;
pub mod svg_chart;
pub mod table;
pub mod tree;
pub mod treemap;
pub mod workload;

#[cfg(test)]
mod test_fixtures;
