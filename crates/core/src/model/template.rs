//! Work item templates: frontmatter + body, loaded from `.workdown/templates/`.
//!
//! A template is a work-item-shaped Markdown file used as a starting point
//! by `workdown add --template <name>`. Unlike a work item, a template has
//! no resolved ID — the `id` field (if any) stays as raw YAML so generator
//! tokens like `$uuid` can be resolved at add-time.

use std::collections::HashMap;
use std::path::PathBuf;

/// A template parsed from disk: frontmatter map plus freeform body.
#[derive(Debug, Clone)]
pub struct Template {
    /// Raw frontmatter as a YAML mapping. `id` is preserved here if set.
    pub frontmatter: HashMap<String, serde_yaml::Value>,
    /// Everything below the closing `---` delimiter.
    pub body: String,
    /// The file the template was read from.
    pub path: PathBuf,
}

/// An error loading or parsing a template.
#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    /// The templates directory does not exist on disk.
    #[error("templates directory '{}' does not exist", path.display())]
    DirectoryMissing { path: PathBuf },

    /// The named template file could not be found.
    #[error(
        "template '{name}' not found (available: {})",
        format_available(available)
    )]
    NotFound {
        name: String,
        available: Vec<String>,
    },

    /// Reading the template file failed (existed, but could not be read).
    #[error("failed to read template '{}': {source}", path.display())]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },

    /// Parsing the template file failed.
    #[error(transparent)]
    Parse(#[from] crate::parser::ParseError),
}

fn format_available(available: &[String]) -> String {
    if available.is_empty() {
        "none".to_owned()
    } else {
        available.join(", ")
    }
}
