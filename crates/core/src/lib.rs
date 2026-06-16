//! Workdown core — domain library for git-based project management.
//!
//! Work items are structured Markdown files (YAML frontmatter + freeform body).
//! The repo is the single source of truth.

pub mod generators;
pub mod item_data;
pub mod model;
pub mod mutation_data;
pub mod operations;
pub mod parser;
pub mod project;
pub mod query;
pub mod resolve;
pub mod resources_check;
pub mod rules;
pub mod schema_data;
pub mod store;
pub mod view_data;
pub mod views_check;
pub mod walker;
