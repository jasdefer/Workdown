//! Core data types: work items, schema definitions, and project configuration.

pub mod schema;

use std::collections::HashMap;
use std::path::PathBuf;

/// A work item as parsed from a Markdown file, before schema validation.
/// The frontmatter is raw YAML key-value pairs; type checking happens later.
#[derive(Debug)]
pub struct RawWorkItem {
    /// Field names to their raw YAML values, as written in the frontmatter.
    pub frontmatter: HashMap<String, serde_yaml::Value>,
    /// Everything below the closing `---` delimiter — freeform Markdown.
    pub body: String,
    /// The file this was parsed from, kept for error messages downstream.
    pub source_path: PathBuf,
}
