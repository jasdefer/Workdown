//! Project configuration types, deserialized from `config.yaml`.

use std::path::PathBuf;

use serde::Deserialize;

/// A parsed project configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Project metadata (name, description).
    pub project: ProjectMeta,
    /// File paths for work items, templates, and resources.
    pub paths: Paths,
    /// Path to the schema file (relative to project root).
    pub schema: PathBuf,
    /// CLI default settings (which fields to use for views).
    pub defaults: ViewDefaults,
}

/// Project-level metadata.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectMeta {
    /// Project name.
    pub name: String,
    /// Optional project description.
    #[serde(default)]
    pub description: String,
}

/// Paths to key directories and files, relative to the project root.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Paths {
    /// Directory containing work item `.md` files.
    pub work_items: PathBuf,
    /// Directory containing work item templates.
    pub templates: PathBuf,
    /// Path to the resources file.
    pub resources: PathBuf,
    /// Path to the views file.
    pub views: PathBuf,
}

/// Default field selections for CLI views.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewDefaults {
    /// Field used for board columns (must be a `choice` field).
    pub board_field: String,
    /// Field used for tree hierarchy (must be a `link` field).
    pub tree_field: String,
    /// Field used for dependency graph (must be a `links` field).
    pub graph_field: String,
}
