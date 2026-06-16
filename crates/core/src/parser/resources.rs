//! Resources loader: parse `resources.yaml` into [`Resources`].
//!
//! The public API is [`parse_resources`] (from a string) and
//! [`load_resources`] (from disk). Each section is a list of entries; an
//! entry must carry an `id` and may carry a `name`. Other attributes are
//! freeform and ignored (unlike `views.yaml`, this parser does **not**
//! reject unknown keys — resource entries are deliberately open-ended).
//!
//! An empty document (a file that is blank or all comments — the shape the
//! default `resources.yaml` ships in) parses to an empty [`Resources`].
//!
//! Cross-file validation (an item's value matching a known resource id)
//! is **not** done here — see the `resource-option-lists` issue.

use std::path::Path;

use indexmap::IndexMap;
use serde::Deserialize;

use crate::model::resources::{ResourceEntry, Resources};

// ── Public API ────────────────────────────────────────────────────────

/// Parse resources from a YAML string.
pub fn parse_resources(yaml: &str) -> Result<Resources, ResourcesLoadError> {
    // An empty or comment-only document deserializes to `None` (YAML null),
    // which we treat as "no resources" rather than an error.
    let raw: Option<IndexMap<String, Vec<RawEntry>>> =
        serde_yaml::from_str(yaml).map_err(ResourcesLoadError::InvalidYaml)?;

    let sections = raw
        .unwrap_or_default()
        .into_iter()
        .map(|(name, entries)| {
            let entries = entries
                .into_iter()
                .map(|entry| ResourceEntry {
                    id: entry.id,
                    name: entry.name,
                })
                .collect();
            (name, entries)
        })
        .collect();

    Ok(Resources { sections })
}

/// Load and parse a resources file from disk.
pub fn load_resources(path: &Path) -> Result<Resources, ResourcesLoadError> {
    let content = std::fs::read_to_string(path).map_err(ResourcesLoadError::ReadFailed)?;
    parse_resources(&content)
}

// ── Errors ────────────────────────────────────────────────────────────

/// Errors from loading or parsing a resources file.
#[derive(Debug, thiserror::Error)]
pub enum ResourcesLoadError {
    #[error("failed to read resources file: {0}")]
    ReadFailed(std::io::Error),

    #[error("invalid YAML in resources: {0}")]
    InvalidYaml(serde_yaml::Error),
}

// ── Raw deserialization target ────────────────────────────────────────

/// One entry as written in YAML. `id` is required; `name` is the optional
/// display label. Any other attributes are accepted and ignored — entries
/// are freeform, so this struct intentionally does *not* deny unknown
/// fields.
#[derive(Deserialize)]
struct RawEntry {
    id: String,
    #[serde(default)]
    name: Option<String>,
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_string_parses_to_empty() {
        let resources = parse_resources("").unwrap();
        assert!(resources.is_empty());
    }

    #[test]
    fn comment_only_parses_to_empty() {
        let resources = parse_resources("# just a comment\n").unwrap();
        assert!(resources.is_empty());
    }

    #[test]
    fn single_section_with_entries() {
        let yaml = "\
people:
  - id: alice
    name: Alice Smith
    email: alice@example.com
  - id: bob
    name: Bob Jones
";
        let resources = parse_resources(yaml).unwrap();
        let people = resources.section("people").unwrap();
        assert_eq!(people.len(), 2);
        assert_eq!(people[0].id, "alice");
        assert_eq!(people[0].name.as_deref(), Some("Alice Smith"));
        assert_eq!(people[1].id, "bob");
    }

    #[test]
    fn extra_attributes_are_ignored() {
        // `email` and `start` are not modelled — they must not cause a
        // parse error, and must not appear anywhere in the loaded data.
        let yaml = "\
sprints:
  - id: sprint-1
    name: Sprint 1
    start: 2026-04-01
    end: 2026-04-14
";
        let resources = parse_resources(yaml).unwrap();
        let sprints = resources.section("sprints").unwrap();
        assert_eq!(sprints.len(), 1);
        assert_eq!(sprints[0].id, "sprint-1");
        assert_eq!(sprints[0].name.as_deref(), Some("Sprint 1"));
    }

    #[test]
    fn entry_without_name_falls_back_to_id_for_label() {
        let yaml = "teams:\n  - id: backend\n";
        let resources = parse_resources(yaml).unwrap();
        let teams = resources.section("teams").unwrap();
        assert_eq!(teams[0].name, None);
        assert_eq!(teams[0].label(), "backend");
    }

    #[test]
    fn label_prefers_name_over_id() {
        let yaml = "people:\n  - id: alice\n    name: Alice Smith\n";
        let resources = parse_resources(yaml).unwrap();
        assert_eq!(resources.section("people").unwrap()[0].label(), "Alice Smith");
    }

    #[test]
    fn declaration_order_is_preserved() {
        let yaml = "\
people:
  - id: alice
teams:
  - id: backend
sprints:
  - id: sprint-1
";
        let resources = parse_resources(yaml).unwrap();
        let names: Vec<&str> = resources.sections.keys().map(String::as_str).collect();
        assert_eq!(names, vec!["people", "teams", "sprints"]);
    }

    #[test]
    fn entry_missing_id_is_an_error() {
        let yaml = "people:\n  - name: No Id Here\n";
        let error = parse_resources(yaml).unwrap_err();
        assert!(matches!(error, ResourcesLoadError::InvalidYaml(_)));
    }
}
