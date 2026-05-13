//! Shared helpers for reading and writing work item frontmatter.
//!
//! Built first for `workdown add`, reused by every command that mutates an
//! item's frontmatter or body (`set`, `unset`, the future `body`,
//! `rename`, etc.). The CLI layer is a thin caller of these.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::model::schema::{FieldDefinition, FieldTypeConfig, Schema};

// ── YAML rendering ────────────────────────────────────────────────────

/// Build YAML frontmatter string with fields in schema-defined order.
///
/// Fields defined in the schema appear first, in the schema's declared
/// order. Fields present in `frontmatter` but absent from the schema
/// appear after, sorted alphabetically for determinism.
///
/// `user_set_id` controls whether the `id` field is emitted: it is
/// omitted by default (the filename carries the id) but kept when the
/// user explicitly set it in the input.
pub(crate) fn build_frontmatter_yaml(
    frontmatter: &HashMap<String, serde_yaml::Value>,
    schema: &Schema,
    user_set_id: bool,
) -> String {
    let mut mapping = serde_yaml::Mapping::new();

    for field_name in schema.fields.keys() {
        if field_name == "id" && !user_set_id {
            continue;
        }
        if let Some(value) = frontmatter.get(field_name) {
            mapping.insert(serde_yaml::Value::String(field_name.clone()), value.clone());
        }
    }

    let mut extra_keys: Vec<&String> = frontmatter
        .keys()
        .filter(|key| !schema.fields.contains_key(key.as_str()))
        .collect();
    extra_keys.sort();
    for key in extra_keys {
        if let Some(value) = frontmatter.get(key) {
            mapping.insert(serde_yaml::Value::String(key.clone()), value.clone());
        }
    }

    serde_yaml::to_string(&mapping).unwrap_or_default()
}

// ── Atomic write ──────────────────────────────────────────────────────

/// Write `content` to `path` via a temp file + rename.
///
/// Writes to `<path>.tmp`, then renames into place. On POSIX the rename
/// is atomic; on Windows `std::fs::rename` overwrites the destination.
/// Either way, an interrupted write cannot leave the destination half-
/// written — at worst it leaves a `<path>.tmp` sibling on disk, which is
/// cleaned up best-effort when the write or rename fails.
pub(crate) fn write_file_atomically(path: &Path, content: &str) -> std::io::Result<()> {
    let temp_path = temp_path_for(path);

    if let Err(error) = std::fs::write(&temp_path, content) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(error);
    }

    if let Err(error) = std::fs::rename(&temp_path, path) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(error);
    }

    Ok(())
}

/// Append `.tmp` to the path's full filename (not its extension).
fn temp_path_for(path: &Path) -> PathBuf {
    let mut buffer = path.as_os_str().to_owned();
    buffer.push(".tmp");
    PathBuf::from(buffer)
}

// ── String → typed value ──────────────────────────────────────────────

/// Parse a user-supplied string into a `serde_yaml::Value` shaped for
/// the given field type.
///
/// Infallible by design: when the string can't be parsed as the natural
/// type (e.g. `"high"` for an integer field), the raw string is returned
/// instead, and the downstream coercion pass flags the type mismatch.
/// This mirrors what would happen if the user hand-edited the file with
/// the same bad value.
///
/// List, links, and multichoice values are comma-split with whitespace
/// trimmed — matching the comma-separated form already accepted by
/// `workdown add`'s schema-derived flags.
pub fn parse_value_for_field(value_str: &str, field_def: &FieldDefinition) -> serde_yaml::Value {
    use serde_yaml::Value;

    match &field_def.type_config {
        FieldTypeConfig::Integer { .. } => match value_str.parse::<i64>() {
            Ok(number) => Value::Number(serde_yaml::Number::from(number)),
            Err(_) => Value::String(value_str.to_owned()),
        },
        FieldTypeConfig::Float { .. } => match value_str.parse::<f64>() {
            Ok(number) => {
                serde_yaml::to_value(number).unwrap_or_else(|_| Value::String(value_str.to_owned()))
            }
            Err(_) => Value::String(value_str.to_owned()),
        },
        FieldTypeConfig::Boolean => match value_str.to_ascii_lowercase().as_str() {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => Value::String(value_str.to_owned()),
        },
        FieldTypeConfig::List
        | FieldTypeConfig::Links { .. }
        | FieldTypeConfig::Multichoice { .. } => {
            let elements: Vec<Value> = value_str
                .split(',')
                .map(|element| Value::String(element.trim().to_owned()))
                .collect();
            Value::Sequence(elements)
        }
        FieldTypeConfig::String { .. }
        | FieldTypeConfig::Choice { .. }
        | FieldTypeConfig::Date
        | FieldTypeConfig::Duration { .. }
        | FieldTypeConfig::Link { .. } => Value::String(value_str.to_owned()),
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use std::collections::HashMap;

    use crate::model::schema::FieldDefinition;

    fn schema_with(fields: Vec<(&str, FieldDefinition)>) -> Schema {
        let fields: IndexMap<String, FieldDefinition> = fields
            .into_iter()
            .map(|(name, definition)| (name.to_owned(), definition))
            .collect();
        let inverse_table = Schema::build_inverse_table(&fields);
        Schema {
            fields,
            rules: vec![],
            inverse_table,
        }
    }

    fn string_field() -> FieldDefinition {
        FieldDefinition::new(FieldTypeConfig::String { pattern: None })
    }

    // ── build_frontmatter_yaml ───────────────────────────────────────

    #[test]
    fn build_emits_fields_in_schema_order() {
        let schema = schema_with(vec![
            ("title", string_field()),
            ("status", string_field()),
            ("priority", string_field()),
        ]);
        let mut frontmatter = HashMap::new();
        frontmatter.insert(
            "priority".to_owned(),
            serde_yaml::Value::String("high".to_owned()),
        );
        frontmatter.insert(
            "title".to_owned(),
            serde_yaml::Value::String("Hello".to_owned()),
        );
        frontmatter.insert(
            "status".to_owned(),
            serde_yaml::Value::String("open".to_owned()),
        );

        let yaml = build_frontmatter_yaml(&frontmatter, &schema, false);

        // Title is listed before status, status before priority — matches schema.
        let title_position = yaml.find("title:").unwrap();
        let status_position = yaml.find("status:").unwrap();
        let priority_position = yaml.find("priority:").unwrap();
        assert!(title_position < status_position);
        assert!(status_position < priority_position);
    }

    #[test]
    fn build_skips_id_when_not_user_set() {
        let schema = schema_with(vec![("id", string_field()), ("title", string_field())]);
        let mut frontmatter = HashMap::new();
        frontmatter.insert(
            "title".to_owned(),
            serde_yaml::Value::String("Hello".to_owned()),
        );

        let yaml = build_frontmatter_yaml(&frontmatter, &schema, false);
        assert!(!yaml.contains("id:"));
        assert!(yaml.contains("title: Hello"));
    }

    #[test]
    fn build_emits_id_when_user_set() {
        let schema = schema_with(vec![("id", string_field()), ("title", string_field())]);
        let mut frontmatter = HashMap::new();
        frontmatter.insert(
            "id".to_owned(),
            serde_yaml::Value::String("custom-id".to_owned()),
        );
        frontmatter.insert(
            "title".to_owned(),
            serde_yaml::Value::String("Hello".to_owned()),
        );

        let yaml = build_frontmatter_yaml(&frontmatter, &schema, true);
        assert!(yaml.contains("id: custom-id"));
    }

    #[test]
    fn build_appends_extra_fields_alphabetically_after_schema_fields() {
        let schema = schema_with(vec![("title", string_field())]);
        let mut frontmatter = HashMap::new();
        frontmatter.insert(
            "title".to_owned(),
            serde_yaml::Value::String("Hello".to_owned()),
        );
        frontmatter.insert("zeta".to_owned(), serde_yaml::Value::String("z".to_owned()));
        frontmatter.insert(
            "alpha".to_owned(),
            serde_yaml::Value::String("a".to_owned()),
        );

        let yaml = build_frontmatter_yaml(&frontmatter, &schema, false);
        let title_position = yaml.find("title:").unwrap();
        let alpha_position = yaml.find("alpha:").unwrap();
        let zeta_position = yaml.find("zeta:").unwrap();
        assert!(title_position < alpha_position);
        assert!(alpha_position < zeta_position);
    }

    #[test]
    fn build_empty_frontmatter_produces_empty_output() {
        let schema = schema_with(vec![("title", string_field())]);
        let frontmatter = HashMap::new();

        let yaml = build_frontmatter_yaml(&frontmatter, &schema, false);
        // serde_yaml renders an empty mapping as "{}\n". Either that or
        // an empty string is acceptable — we just care that it parses.
        let parsed: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap_or_default();
        assert!(parsed.is_null() || parsed.as_mapping().map(|m| m.is_empty()).unwrap_or(false));
    }

    // ── write_file_atomically ────────────────────────────────────────

    #[test]
    fn atomic_write_creates_new_file_with_content() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("note.md");

        write_file_atomically(&target, "hello world").unwrap();

        let read_back = std::fs::read_to_string(&target).unwrap();
        assert_eq!(read_back, "hello world");
    }

    #[test]
    fn atomic_write_replaces_existing_file() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("note.md");
        std::fs::write(&target, "original").unwrap();

        write_file_atomically(&target, "replaced").unwrap();

        let read_back = std::fs::read_to_string(&target).unwrap();
        assert_eq!(read_back, "replaced");
    }

    #[test]
    fn atomic_write_removes_temp_file_on_success() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("note.md");

        write_file_atomically(&target, "content").unwrap();

        let temp = directory.path().join("note.md.tmp");
        assert!(
            !temp.exists(),
            "temp file should be gone after successful write"
        );
    }

    // ── parse_value_for_field ────────────────────────────────────────

    fn integer_field() -> FieldDefinition {
        FieldDefinition::new(FieldTypeConfig::Integer {
            min: None,
            max: None,
        })
    }

    fn float_field() -> FieldDefinition {
        FieldDefinition::new(FieldTypeConfig::Float {
            min: None,
            max: None,
        })
    }

    fn boolean_field() -> FieldDefinition {
        FieldDefinition::new(FieldTypeConfig::Boolean)
    }

    fn list_field() -> FieldDefinition {
        FieldDefinition::new(FieldTypeConfig::List)
    }

    fn choice_field() -> FieldDefinition {
        FieldDefinition::new(FieldTypeConfig::Choice {
            values: vec!["open".into(), "done".into()],
        })
    }

    #[test]
    fn parse_integer_returns_number() {
        let value = parse_value_for_field("42", &integer_field());
        assert_eq!(value.as_i64().unwrap(), 42);
    }

    #[test]
    fn parse_integer_falls_back_to_string_on_bad_input() {
        let value = parse_value_for_field("high", &integer_field());
        assert_eq!(value.as_str().unwrap(), "high");
    }

    #[test]
    fn parse_float_returns_number() {
        let value = parse_value_for_field("2.5", &float_field());
        assert!((value.as_f64().unwrap() - 2.5).abs() < 1e-9);
    }

    #[test]
    fn parse_boolean_true_and_false_are_case_insensitive() {
        assert_eq!(
            parse_value_for_field("true", &boolean_field()),
            serde_yaml::Value::Bool(true)
        );
        assert_eq!(
            parse_value_for_field("TRUE", &boolean_field()),
            serde_yaml::Value::Bool(true)
        );
        assert_eq!(
            parse_value_for_field("False", &boolean_field()),
            serde_yaml::Value::Bool(false)
        );
    }

    #[test]
    fn parse_boolean_falls_back_to_string_on_bad_input() {
        let value = parse_value_for_field("yes", &boolean_field());
        assert_eq!(value.as_str().unwrap(), "yes");
    }

    #[test]
    fn parse_list_splits_on_comma_and_trims() {
        let value = parse_value_for_field("auth, backend ,qa", &list_field());
        let sequence = value.as_sequence().unwrap();
        assert_eq!(sequence.len(), 3);
        assert_eq!(sequence[0].as_str().unwrap(), "auth");
        assert_eq!(sequence[1].as_str().unwrap(), "backend");
        assert_eq!(sequence[2].as_str().unwrap(), "qa");
    }

    #[test]
    fn parse_list_single_element_is_a_one_item_sequence() {
        let value = parse_value_for_field("auth", &list_field());
        let sequence = value.as_sequence().unwrap();
        assert_eq!(sequence.len(), 1);
        assert_eq!(sequence[0].as_str().unwrap(), "auth");
    }

    #[test]
    fn parse_string_field_returns_string() {
        let value = parse_value_for_field("hello world", &string_field());
        assert_eq!(value.as_str().unwrap(), "hello world");
    }

    #[test]
    fn parse_choice_field_returns_string() {
        // Choice membership check happens in coerce, not here.
        let value = parse_value_for_field("not-in-list", &choice_field());
        assert_eq!(value.as_str().unwrap(), "not-in-list");
    }
}
