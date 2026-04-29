//! Core data types: work items, schema definitions, and project configuration.

pub mod assertion;
pub mod condition;
pub mod config;
pub mod diagnostic;
pub mod duration;
pub mod rule;
pub mod schema;
pub mod template;
pub mod views;

use std::borrow::Borrow;
use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize, Serializer};

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
///
/// Serialized untagged: consumers see bare JSON scalars/arrays
/// (`"open"`, `42`, `["a","b"]`, `"2026-04-23"`) and consult the schema
/// separately to interpret the type. Duration uses a custom serializer
/// that emits the formatted string (`"5d"`) — same convention `Date`
/// follows for human-readable output. Deserialize is not derived —
/// values are produced by the coercion layer, not parsed from JSON.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
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
    /// A calendar date. On disk `YYYY-MM-DD`; in memory a native `NaiveDate`.
    Date(chrono::NaiveDate),
    /// A signed duration in canonical seconds. Formatted as suffix
    /// shorthand (`"5d"`, `"1w 2d 3h"`) for human-readable output.
    Duration(#[serde(serialize_with = "serialize_duration_seconds")] i64),
    /// A boolean flag.
    Boolean(bool),
    /// A list of free-form strings.
    List(Vec<String>),
    /// A reference to a single work item by ID.
    Link(WorkItemId),
    /// References to multiple work items by ID.
    Links(Vec<WorkItemId>),
}

/// Serializer for `FieldValue::Duration`: emits the formatted string
/// (`"5d"`) rather than the raw i64, matching how `Date` emits
/// `"YYYY-MM-DD"` rather than its internal representation.
fn serialize_duration_seconds<S>(seconds: &i64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&duration::format_duration_seconds(*seconds))
}
