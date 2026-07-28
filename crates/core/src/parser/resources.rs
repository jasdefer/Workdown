//! Resources loader: parse `resources.yaml` into [`Resources`].
//!
//! The public API is [`parse_resources`] (from a string) and
//! [`load_resources`] (from disk). Each top-level key is a resource
//! section — a list of entries, where an entry must carry an `id` and may
//! carry a `name`. Other entry attributes are freeform and ignored (unlike
//! `views.yaml`, this parser does **not** reject unknown keys — resource
//! entries are deliberately open-ended). A section whose value is null
//! (a key with every entry commented out — the shape the default
//! `resources.yaml` ships in) is an empty section.
//!
//! The top-level key `constants` is reserved: it holds a mapping of
//! constant name → `{type, value}`, where `type` names a scalar from the
//! field type system and `value` is coerced to it at parse time. Unlike
//! entries, constant declarations are closed — unknown keys are rejected.
//!
//! An empty document (a file that is blank or all comments) parses to an
//! empty [`Resources`].
//!
//! Cross-file validation (an item's value matching a known resource id)
//! is **not** done here — see the `resource-option-lists` issue.

use std::path::Path;

use chrono::NaiveDate;
use indexmap::IndexMap;
use serde::Deserialize;

use crate::model::duration::parse_duration;
use crate::model::resources::{ResourceEntry, Resources};
use crate::model::FieldValue;

/// Reserved top-level key holding named scalar constants.
const CONSTANTS_KEY: &str = "constants";

/// Scalar type names a constant may declare. Non-scalar field types
/// (choice, link, list, …) need per-field configuration or item context
/// and are deliberately excluded.
const CONSTANT_TYPES: &str = "string, integer, float, date, duration, boolean";

// ── Public API ────────────────────────────────────────────────────────

/// Parse resources from a YAML string.
pub fn parse_resources(yaml: &str) -> Result<Resources, ResourcesLoadError> {
    // An empty or comment-only document deserializes to `None` (YAML null),
    // which we treat as "no resources" rather than an error.
    let raw: Option<IndexMap<String, serde_yaml::Value>> =
        serde_yaml::from_str(yaml).map_err(ResourcesLoadError::InvalidYaml)?;

    let mut sections = IndexMap::new();
    let mut constants = IndexMap::new();
    for (name, value) in raw.unwrap_or_default() {
        if name == CONSTANTS_KEY {
            constants = parse_constants(value)?;
        } else {
            sections.insert(name.clone(), parse_section(&name, value)?);
        }
    }

    Ok(Resources {
        sections,
        constants,
    })
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

    #[error("invalid resource section '{section}': {detail}")]
    InvalidSection {
        section: String,
        detail: serde_yaml::Error,
    },

    #[error("invalid constants section: {detail}")]
    InvalidConstants { detail: serde_yaml::Error },

    #[error("constant '{name}' has unsupported type '{type_name}' (allowed: {CONSTANT_TYPES})")]
    UnsupportedConstantType { name: String, type_name: String },

    #[error("constant '{name}': {reason}")]
    InvalidConstantValue { name: String, reason: String },
}

// ── Sections ──────────────────────────────────────────────────────────

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

fn parse_section(
    name: &str,
    value: serde_yaml::Value,
) -> Result<Vec<ResourceEntry>, ResourcesLoadError> {
    if value.is_null() {
        return Ok(Vec::new());
    }
    let entries: Vec<RawEntry> =
        serde_yaml::from_value(value).map_err(|error| ResourcesLoadError::InvalidSection {
            section: name.to_owned(),
            detail: error,
        })?;
    Ok(entries
        .into_iter()
        .map(|entry| ResourceEntry {
            id: entry.id,
            name: entry.name,
        })
        .collect())
}

// ── Constants ─────────────────────────────────────────────────────────

/// One constant as written in YAML. Unlike resource entries, the shape is
/// closed — a typo like `vaule:` must surface, not silently produce a
/// constant without a value.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConstant {
    #[serde(rename = "type")]
    type_name: String,
    value: serde_yaml::Value,
}

fn parse_constants(
    value: serde_yaml::Value,
) -> Result<IndexMap<String, FieldValue>, ResourcesLoadError> {
    if value.is_null() {
        return Ok(IndexMap::new());
    }
    let raw: IndexMap<String, RawConstant> = serde_yaml::from_value(value)
        .map_err(|error| ResourcesLoadError::InvalidConstants { detail: error })?;

    let mut constants = IndexMap::new();
    for (name, raw_constant) in raw {
        let coerced = coerce_constant(&name, &raw_constant.type_name, &raw_constant.value)?;
        constants.insert(name, coerced);
    }
    Ok(constants)
}

/// Coerce a constant's YAML value to the scalar type it declares.
fn coerce_constant(
    name: &str,
    type_name: &str,
    value: &serde_yaml::Value,
) -> Result<FieldValue, ResourcesLoadError> {
    let invalid = |reason: String| ResourcesLoadError::InvalidConstantValue {
        name: name.to_owned(),
        reason,
    };

    match type_name {
        "string" => value
            .as_str()
            .map(|text| FieldValue::String(text.to_owned()))
            .ok_or_else(|| invalid("value must be a string".to_owned())),
        "integer" => value
            .as_i64()
            .map(FieldValue::Integer)
            .ok_or_else(|| invalid("value must be an integer".to_owned())),
        "float" => value
            .as_f64()
            .map(FieldValue::Float)
            .ok_or_else(|| invalid("value must be a number".to_owned())),
        "boolean" => value
            .as_bool()
            .map(FieldValue::Boolean)
            .ok_or_else(|| invalid("value must be a boolean".to_owned())),
        "date" => {
            let text = value
                .as_str()
                .ok_or_else(|| invalid("value must be a YYYY-MM-DD string".to_owned()))?;
            let date = NaiveDate::parse_from_str(text, "%Y-%m-%d")
                .map_err(|_| invalid(format!("'{text}' is not a valid YYYY-MM-DD date")))?;
            Ok(FieldValue::Date(date))
        }
        "duration" => {
            let text = value.as_str().ok_or_else(|| {
                invalid("value must be a duration string (e.g. \"8h\")".to_owned())
            })?;
            let seconds = parse_duration(text).map_err(|error| invalid(error.to_string()))?;
            Ok(FieldValue::Duration(seconds))
        }
        other => Err(ResourcesLoadError::UnsupportedConstantType {
            name: name.to_owned(),
            type_name: other.to_owned(),
        }),
    }
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
    fn null_section_is_empty() {
        // A key with every entry commented out — the shape the default
        // resources.yaml ships in.
        let yaml = "\
people:
  # - id: alice
";
        let resources = parse_resources(yaml).unwrap();
        assert_eq!(resources.section("people").unwrap().len(), 0);
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
        assert_eq!(
            resources.section("people").unwrap()[0].label(),
            "Alice Smith"
        );
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
        assert!(matches!(error, ResourcesLoadError::InvalidSection { .. }));
    }

    #[test]
    fn section_error_names_the_section() {
        let yaml = "people:\n  - name: No Id Here\n";
        let message = parse_resources(yaml).unwrap_err().to_string();
        assert!(message.contains("people"), "got: {message}");
    }

    #[test]
    fn section_with_non_sequence_value_is_an_error() {
        let yaml = "people: just a string\n";
        let error = parse_resources(yaml).unwrap_err();
        assert!(matches!(error, ResourcesLoadError::InvalidSection { .. }));
    }

    // ── Constants ─────────────────────────────────────────────────────

    #[test]
    fn constants_of_every_scalar_type() {
        let yaml = "\
constants:
  project_code:
    type: string
    value: WD
  team_size:
    type: integer
    value: 4
  daily_rate:
    type: float
    value: 800.5
  kickoff:
    type: date
    value: 2026-04-20
  work_hours_per_day:
    type: duration
    value: 8h
  billable:
    type: boolean
    value: true
";
        let resources = parse_resources(yaml).unwrap();
        assert_eq!(
            resources.constant("project_code"),
            Some(&FieldValue::String("WD".to_owned()))
        );
        assert_eq!(
            resources.constant("team_size"),
            Some(&FieldValue::Integer(4))
        );
        assert_eq!(
            resources.constant("daily_rate"),
            Some(&FieldValue::Float(800.5))
        );
        assert_eq!(
            resources.constant("kickoff"),
            Some(&FieldValue::Date(
                chrono::NaiveDate::from_ymd_opt(2026, 4, 20).unwrap()
            ))
        );
        assert_eq!(
            resources.constant("work_hours_per_day"),
            Some(&FieldValue::Duration(8 * 3_600))
        );
        assert_eq!(
            resources.constant("billable"),
            Some(&FieldValue::Boolean(true))
        );
    }

    #[test]
    fn constants_coexist_with_sections_and_are_not_a_section() {
        let yaml = "\
people:
  - id: alice
constants:
  daily_rate:
    type: float
    value: 800
";
        let resources = parse_resources(yaml).unwrap();
        assert_eq!(resources.sections.len(), 1);
        assert!(resources.section("constants").is_none());
        assert_eq!(
            resources.constant("daily_rate"),
            Some(&FieldValue::Float(800.0))
        );
    }

    #[test]
    fn constants_declaration_order_is_preserved() {
        let yaml = "\
constants:
  zulu:
    type: integer
    value: 1
  alpha:
    type: integer
    value: 2
";
        let resources = parse_resources(yaml).unwrap();
        let names: Vec<&str> = resources.constants.keys().map(String::as_str).collect();
        assert_eq!(names, vec!["zulu", "alpha"]);
    }

    #[test]
    fn null_constants_section_is_empty() {
        let yaml = "constants:\n  # all commented out\n";
        let resources = parse_resources(yaml).unwrap();
        assert!(resources.constants.is_empty());
    }

    #[test]
    fn float_constant_accepts_integer_value() {
        let yaml = "constants:\n  rate:\n    type: float\n    value: 800\n";
        let resources = parse_resources(yaml).unwrap();
        assert_eq!(resources.constant("rate"), Some(&FieldValue::Float(800.0)));
    }

    #[test]
    fn integer_constant_rejects_fractional_value() {
        let yaml = "constants:\n  size:\n    type: integer\n    value: 4.5\n";
        let error = parse_resources(yaml).unwrap_err();
        assert!(matches!(
            error,
            ResourcesLoadError::InvalidConstantValue { ref name, .. } if name == "size"
        ));
    }

    #[test]
    fn unsupported_constant_type_is_an_error() {
        let yaml = "constants:\n  status:\n    type: choice\n    value: open\n";
        let error = parse_resources(yaml).unwrap_err();
        assert!(matches!(
            error,
            ResourcesLoadError::UnsupportedConstantType { ref type_name, .. }
                if type_name == "choice"
        ));
    }

    #[test]
    fn invalid_date_constant_is_an_error() {
        let yaml = "constants:\n  kickoff:\n    type: date\n    value: 2026-02-30\n";
        let error = parse_resources(yaml).unwrap_err();
        assert!(matches!(
            error,
            ResourcesLoadError::InvalidConstantValue { ref name, .. } if name == "kickoff"
        ));
    }

    #[test]
    fn invalid_duration_constant_is_an_error() {
        let yaml = "constants:\n  pace:\n    type: duration\n    value: garbage\n";
        let error = parse_resources(yaml).unwrap_err();
        assert!(matches!(
            error,
            ResourcesLoadError::InvalidConstantValue { ref name, .. } if name == "pace"
        ));
    }

    #[test]
    fn constant_with_unknown_key_is_an_error() {
        // Constant declarations are closed shapes — `vaule:` is a typo,
        // not freeform metadata.
        let yaml = "constants:\n  rate:\n    type: float\n    vaule: 800\n";
        let error = parse_resources(yaml).unwrap_err();
        assert!(matches!(error, ResourcesLoadError::InvalidConstants { .. }));
    }

    #[test]
    fn constant_missing_value_is_an_error() {
        let yaml = "constants:\n  rate:\n    type: float\n";
        let error = parse_resources(yaml).unwrap_err();
        assert!(matches!(error, ResourcesLoadError::InvalidConstants { .. }));
    }

    #[test]
    fn constants_as_sequence_is_an_error() {
        let yaml = "constants:\n  - id: rate\n";
        let error = parse_resources(yaml).unwrap_err();
        assert!(matches!(error, ResourcesLoadError::InvalidConstants { .. }));
    }
}
