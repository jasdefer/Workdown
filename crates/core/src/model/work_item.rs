//! Work-item identity and the parsed work-item record.

use std::borrow::Borrow;
use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::FieldValue;

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

/// Check whether a string is a valid work-item ID: non-empty, starts with a
/// lowercase letter or digit, contains only lowercase letters, digits, and
/// hyphens, and doesn't end with a hyphen.
pub(crate) fn is_valid_id(id: &str) -> bool {
    if id.is_empty() {
        return false;
    }

    let mut chars = id.chars();

    // Must start with a lowercase letter or digit.
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit() => {}
        _ => return false,
    }

    // Remaining: lowercase letters, digits, hyphens.
    for c in chars {
        if !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            return false;
        }
    }

    // Must not end with a hyphen.
    if id.ends_with('-') {
        return false;
    }

    true
}

// ── WorkItem ────────────────────────────────────────────────────────

/// A work item with typed field values, ready for use by commands.
/// Produced by coercing a `crate::parser::RawWorkItem`'s fields against the project schema.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_ids() {
        assert!(is_valid_id("fix-login"));
        assert!(is_valid_id("a"));
        assert!(is_valid_id("task-42"));
        assert!(is_valid_id("implement-auth-epic"));
        assert!(is_valid_id("a1b2c3"));
        assert!(is_valid_id("1-task"));
        assert!(is_valid_id("42"));
        assert!(is_valid_id("9-lives"));
    }

    #[test]
    fn invalid_ids() {
        assert!(!is_valid_id(""));
        assert!(!is_valid_id("-fix"));
        assert!(!is_valid_id("fix-"));
        assert!(!is_valid_id("Fix"));
        assert!(!is_valid_id("fix_login"));
        assert!(!is_valid_id("1-"));
        assert!(!is_valid_id("fix login"));
    }
}
