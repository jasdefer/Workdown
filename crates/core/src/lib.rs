//! Workdown core — domain library for git-based project management.
//!
//! Work items are structured Markdown files (YAML frontmatter + freeform body).
//! The repo is the single source of truth.

pub mod generators;
pub mod model;
pub mod operations;
pub mod parser;
pub mod query;
pub mod resolve;
pub mod rules;
pub mod store;
pub mod views_check;
