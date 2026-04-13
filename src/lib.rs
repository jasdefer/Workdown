//! Workdown — Git-based project management CLI.
//!
//! Work items are structured Markdown files (YAML frontmatter + freeform body).
//! The repo is the single source of truth.

pub mod cli;
pub mod commands;
pub mod model;
pub mod parser;
pub mod rules;
pub mod store;
