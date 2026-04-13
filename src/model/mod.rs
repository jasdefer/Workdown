//! Core data types: work items, schema definitions, and project configuration.

pub mod schema;

use std::collections::HashMap;
use std::path::PathBuf;

/// A work item as parsed from a Markdown file, before type coercion.
/// Has a resolved ID but raw YAML field values. Internal intermediate — the
/// store converts this into a [`WorkItem`] with typed fields.
#[derive(Debug)]
pub(crate) struct RawWorkItem {
    /// Resolved ID: from frontmatter `id` field if present, otherwise filename without `.md`.
    pub id: String,
    /// Field names to their raw YAML values, as written in the frontmatter.
    /// The `id` field (if present in frontmatter) is excluded — use `id` above.
    pub frontmatter: HashMap<String, serde_yaml::Value>,
    /// Everything below the closing `---` delimiter — freeform Markdown.
    pub body: String,
    /// The file this was parsed from, kept for error messages downstream.
    pub source_path: PathBuf,
}

/// A work item with typed field values, ready for use by commands.
/// Produced by coercing a [`RawWorkItem`]'s fields against the project schema.
#[derive(Debug)]
pub struct WorkItem {
    /// Resolved ID: from frontmatter `id` field if present, otherwise filename without `.md`.
    pub id: String,
    /// Field names to their typed values, coerced according to the schema.
    pub fields: HashMap<String, FieldValue>,
    /// Everything below the closing `---` delimiter — freeform Markdown.
    pub body: String,
    /// The file this was parsed from, kept for error messages downstream.
    pub source_path: PathBuf,
}

/// A typed field value, coerced from raw YAML according to the schema's field type.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    /// A free-form string.
    String(String),
    /// A single value from an allowed set.
    Choice(String),
    /// Multiple values from an allowed set.
    Multichoice(Vec<String>),
    /// A signed integer.
    Integer(i64),
    /// A floating-point number.
    Float(f64),
    /// A date in `YYYY-MM-DD` format (stored as string, validated at coercion time).
    Date(String),
    /// A boolean flag.
    Boolean(bool),
    /// A list of free-form strings.
    List(Vec<String>),
    /// A reference to a single work item by ID.
    Link(String),
    /// References to multiple work items by ID.
    Links(Vec<String>),
}
