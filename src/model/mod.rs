//! Core data types: work items, schema definitions, and project configuration.

pub mod assertion;
pub mod condition;
pub mod config;
pub mod diagnostic;
pub mod rule;
pub mod schema;
pub mod template;

use std::borrow::Borrow;
use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// ── WorkItemId ──────────────────────────────────────────────────────

/// A unique identifier for a work item.
///
/// Wraps a `String` to distinguish work item IDs from arbitrary strings
/// at the type level. Construction is open (`From<String>`); validation
/// happens in the parser before wrapping.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkItemId(String);

impl WorkItemId {
    /// View the ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for WorkItemId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for WorkItemId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for WorkItemId {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl From<String> for WorkItemId {
    fn from(s: String) -> Self {
        WorkItemId(s)
    }
}

impl PartialEq<str> for WorkItemId {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for WorkItemId {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

// ── WorkItem ────────────────────────────────────────────────────────

/// A work item with typed field values, ready for use by commands.
/// Produced by coercing a [`crate::parser::RawWorkItem`]'s fields against the project schema.
#[derive(Debug)]
pub struct WorkItem {
    /// Resolved ID: from frontmatter `id` field if present, otherwise filename without `.md`.
    pub id: WorkItemId,
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
    Link(WorkItemId),
    /// References to multiple work items by ID.
    Links(Vec<WorkItemId>),
}
