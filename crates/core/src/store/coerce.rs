//! Field coercion: convert raw `serde_yaml::Value` fields into typed [`FieldValue`]s.
//!
//! Operates on a single [`crate::parser::RawWorkItem`] and the project [`Schema`].
//! Produces a map of successfully coerced fields plus a list of
//! [`Diagnostic`]s for fields that failed coercion or violated constraints.

use std::collections::HashMap;

use chrono::NaiveDate;
use regex::Regex;

use crate::model::diagnostic::{Diagnostic, DiagnosticKind, FieldValueError};
use crate::model::schema::{FieldDefinition, FieldType, FieldTypeConfig, Schema, Severity};
use crate::model::{FieldValue, WorkItemId};
use crate::parser::RawWorkItem;

/// Coerce raw frontmatter values into typed [`FieldValue`]s according to the schema.
///
/// Returns the successfully coerced fields and any diagnostics.
/// Fields that fail coercion are omitted from the map; required fields
/// that are absent produce a [`DiagnosticKind::MissingRequired`].
pub(crate) fn coerce_fields(
    raw: &RawWorkItem,
    schema: &Schema,
) -> (HashMap<String, FieldValue>, Vec<Diagnostic>) {
    let mut fields = HashMap::new();
    let mut diagnostics = Vec::new();

    // Coerce each schema-defined field (skip `id` — already on RawWorkItem.id).
    for (name, def) in &schema.fields {
        if name == "id" {
            continue;
        }

        match raw.frontmatter.get(name) {
            Some(value) if !value.is_null() => match coerce_value(value, def) {
                Ok(field_value) => {
                    fields.insert(name.clone(), field_value);
                }
                Err(detail) => {
                    diagnostics.push(Diagnostic {
                        severity: Severity::Error,
                        kind: DiagnosticKind::InvalidFieldValue {
                            item_id: raw.id.clone(),
                            field: name.clone(),
                            detail,
                        },
                    });
                }
            },
            _ => {
                // Value is absent or null. Required-field check is deferred
                // for aggregate fields — those can be filled in by the
                // rollup pass, so the post-compute pass in `rollup::run`
                // emits `MissingRequired` only for items that remain blank.
                if def.required && def.aggregate.is_none() {
                    diagnostics.push(Diagnostic {
                        severity: Severity::Error,
                        kind: DiagnosticKind::MissingRequired {
                            item_id: raw.id.clone(),
                            field: name.clone(),
                        },
                    });
                }
            }
        }
    }

    // Warn about fields in frontmatter that aren't in the schema.
    for name in raw.frontmatter.keys() {
        if !schema.fields.contains_key(name) {
            diagnostics.push(Diagnostic {
                severity: Severity::Warning,
                kind: DiagnosticKind::UnknownField {
                    item_id: raw.id.clone(),
                    field: name.clone(),
                },
            });
        }
    }

    (fields, diagnostics)
}

/// Coerce a single YAML value into a [`FieldValue`] according to the field definition.
fn coerce_value(
    value: &serde_yaml::Value,
    def: &FieldDefinition,
) -> Result<FieldValue, FieldValueError> {
    match &def.type_config {
        FieldTypeConfig::String { pattern } => coerce_string(value, pattern.as_deref()),
        FieldTypeConfig::Choice { values } => coerce_choice(value, values),
        FieldTypeConfig::Multichoice { values } => coerce_multichoice(value, values),
        FieldTypeConfig::Integer { min, max } => coerce_integer(value, *min, *max),
        FieldTypeConfig::Float { min, max } => coerce_float(value, *min, *max),
        FieldTypeConfig::Date => coerce_date(value),
        FieldTypeConfig::Duration { min, max } => coerce_duration(value, *min, *max),
        FieldTypeConfig::Boolean => coerce_boolean(value),
        FieldTypeConfig::List => coerce_list(value),
        FieldTypeConfig::Link { .. } => coerce_link(value),
        FieldTypeConfig::Links { .. } => coerce_links(value),
    }
}

// ── Per-type coercion ────────────────────────────────────────────────

fn coerce_string(
    value: &serde_yaml::Value,
    pattern: Option<&str>,
) -> Result<FieldValue, FieldValueError> {
    let s = value
        .as_str()
        .ok_or_else(|| FieldValueError::TypeMismatch {
            expected: FieldType::String,
            got: yaml_type_name(value).into(),
        })?;

    if let Some(pattern) = pattern {
        let re = Regex::new(pattern).map_err(|e| FieldValueError::InvalidPattern {
            pattern: pattern.to_owned(),
            error: e.to_string(),
        })?;
        if !re.is_match(s) {
            return Err(FieldValueError::PatternMismatch {
                value: s.to_owned(),
                pattern: pattern.to_owned(),
            });
        }
    }

    Ok(FieldValue::String(s.to_owned()))
}

fn coerce_choice(
    value: &serde_yaml::Value,
    allowed: &[String],
) -> Result<FieldValue, FieldValueError> {
    let s = value
        .as_str()
        .ok_or_else(|| FieldValueError::TypeMismatch {
            expected: FieldType::Choice,
            got: yaml_type_name(value).into(),
        })?;

    if !allowed.iter().any(|allowed_value| allowed_value == s) {
        return Err(FieldValueError::InvalidChoice {
            value: s.to_owned(),
            allowed: allowed.to_vec(),
        });
    }

    Ok(FieldValue::Choice(s.to_owned()))
}

fn coerce_multichoice(
    value: &serde_yaml::Value,
    allowed: &[String],
) -> Result<FieldValue, FieldValueError> {
    let seq = value
        .as_sequence()
        .ok_or_else(|| FieldValueError::TypeMismatch {
            expected: FieldType::Multichoice,
            got: yaml_type_name(value).into(),
        })?;

    let mut result = Vec::with_capacity(seq.len());
    for item in seq {
        let s = item.as_str().ok_or_else(|| FieldValueError::TypeMismatch {
            expected: FieldType::Multichoice,
            got: format!("sequence containing {}", yaml_type_name(item)),
        })?;
        result.push(s.to_owned());
    }

    let invalid: Vec<String> = result
        .iter()
        .filter(|value| !allowed.contains(value))
        .cloned()
        .collect();
    if !invalid.is_empty() {
        return Err(FieldValueError::InvalidMultichoice {
            values: invalid,
            allowed: allowed.to_vec(),
        });
    }

    Ok(FieldValue::Multichoice(result))
}

fn coerce_integer(
    value: &serde_yaml::Value,
    min: Option<f64>,
    max: Option<f64>,
) -> Result<FieldValue, FieldValueError> {
    let n = value
        .as_i64()
        .ok_or_else(|| FieldValueError::TypeMismatch {
            expected: FieldType::Integer,
            got: yaml_type_name(value).into(),
        })?;

    if let Some(min) = min {
        if (n as f64) < min {
            return Err(FieldValueError::OutOfRange {
                value: n as f64,
                min: Some(min),
                max,
            });
        }
    }
    if let Some(max) = max {
        if (n as f64) > max {
            return Err(FieldValueError::OutOfRange {
                value: n as f64,
                min,
                max: Some(max),
            });
        }
    }

    Ok(FieldValue::Integer(n))
}

fn coerce_float(
    value: &serde_yaml::Value,
    min: Option<f64>,
    max: Option<f64>,
) -> Result<FieldValue, FieldValueError> {
    let n = value
        .as_f64()
        .ok_or_else(|| FieldValueError::TypeMismatch {
            expected: FieldType::Float,
            got: yaml_type_name(value).into(),
        })?;

    if let Some(min) = min {
        if n < min {
            return Err(FieldValueError::OutOfRange {
                value: n,
                min: Some(min),
                max,
            });
        }
    }
    if let Some(max) = max {
        if n > max {
            return Err(FieldValueError::OutOfRange {
                value: n,
                min,
                max: Some(max),
            });
        }
    }

    Ok(FieldValue::Float(n))
}

fn coerce_duration(
    value: &serde_yaml::Value,
    min: Option<i64>,
    max: Option<i64>,
) -> Result<FieldValue, FieldValueError> {
    use crate::model::duration::{format_duration_seconds, parse_duration};

    let s = value
        .as_str()
        .ok_or_else(|| FieldValueError::TypeMismatch {
            expected: FieldType::Duration,
            got: yaml_type_name(value).into(),
        })?;

    let seconds = parse_duration(s).map_err(|err| FieldValueError::InvalidDuration {
        value: s.to_owned(),
        reason: err.to_string(),
    })?;

    if let Some(min) = min {
        if seconds < min {
            return Err(FieldValueError::OutOfRangeDuration {
                value: format_duration_seconds(seconds),
                min: Some(format_duration_seconds(min)),
                max: max.map(format_duration_seconds),
            });
        }
    }
    if let Some(max) = max {
        if seconds > max {
            return Err(FieldValueError::OutOfRangeDuration {
                value: format_duration_seconds(seconds),
                min: min.map(format_duration_seconds),
                max: Some(format_duration_seconds(max)),
            });
        }
    }

    Ok(FieldValue::Duration(seconds))
}

fn coerce_date(value: &serde_yaml::Value) -> Result<FieldValue, FieldValueError> {
    let s = value
        .as_str()
        .ok_or_else(|| FieldValueError::TypeMismatch {
            expected: FieldType::Date,
            got: yaml_type_name(value).into(),
        })?;

    let date =
        NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| FieldValueError::InvalidDate {
            value: s.to_owned(),
        })?;

    Ok(FieldValue::Date(date))
}

fn coerce_boolean(value: &serde_yaml::Value) -> Result<FieldValue, FieldValueError> {
    let b = value
        .as_bool()
        .ok_or_else(|| FieldValueError::TypeMismatch {
            expected: FieldType::Boolean,
            got: yaml_type_name(value).into(),
        })?;

    Ok(FieldValue::Boolean(b))
}

fn coerce_list(value: &serde_yaml::Value) -> Result<FieldValue, FieldValueError> {
    let seq = value
        .as_sequence()
        .ok_or_else(|| FieldValueError::TypeMismatch {
            expected: FieldType::List,
            got: yaml_type_name(value).into(),
        })?;

    let mut result = Vec::with_capacity(seq.len());
    for item in seq {
        let s = item.as_str().ok_or_else(|| FieldValueError::TypeMismatch {
            expected: FieldType::List,
            got: format!("sequence containing {}", yaml_type_name(item)),
        })?;
        result.push(s.to_owned());
    }

    Ok(FieldValue::List(result))
}

fn coerce_link(value: &serde_yaml::Value) -> Result<FieldValue, FieldValueError> {
    let s = value
        .as_str()
        .ok_or_else(|| FieldValueError::TypeMismatch {
            expected: FieldType::Link,
            got: yaml_type_name(value).into(),
        })?;

    Ok(FieldValue::Link(WorkItemId::from(s.to_owned())))
}

fn coerce_links(value: &serde_yaml::Value) -> Result<FieldValue, FieldValueError> {
    let seq = value
        .as_sequence()
        .ok_or_else(|| FieldValueError::TypeMismatch {
            expected: FieldType::Links,
            got: yaml_type_name(value).into(),
        })?;

    let mut result = Vec::with_capacity(seq.len());
    for item in seq {
        let s = item.as_str().ok_or_else(|| FieldValueError::TypeMismatch {
            expected: FieldType::Links,
            got: format!("sequence containing {}", yaml_type_name(item)),
        })?;
        result.push(WorkItemId::from(s.to_owned()));
    }

    Ok(FieldValue::Links(result))
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Human-readable name for a YAML value type (for error messages).
fn yaml_type_name(value: &serde_yaml::Value) -> &'static str {
    match value {
        serde_yaml::Value::Null => "null",
        serde_yaml::Value::Bool(_) => "boolean",
        serde_yaml::Value::Number(_) => "number",
        serde_yaml::Value::String(_) => "string",
        serde_yaml::Value::Sequence(_) => "sequence",
        serde_yaml::Value::Mapping(_) => "mapping",
        serde_yaml::Value::Tagged(_) => "tagged",
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::{FieldDefinition, FieldTypeConfig};
    use indexmap::IndexMap;
    use std::path::PathBuf;

    /// Build a minimal schema with the given fields.
    fn schema(fields: Vec<(&str, FieldDefinition)>) -> Schema {
        let fields: IndexMap<String, FieldDefinition> = fields
            .into_iter()
            .map(|(name, def)| (name.to_owned(), def))
            .collect();
        let inverse_table = Schema::build_inverse_table(&fields);
        Schema {
            fields,
            rules: vec![],
            inverse_table,
        }
    }

    /// Build a RawWorkItem with the given frontmatter.
    fn raw_item(id: &str, frontmatter: Vec<(&str, serde_yaml::Value)>) -> RawWorkItem {
        RawWorkItem {
            id: WorkItemId::from(id.to_owned()),
            frontmatter: frontmatter
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v))
                .collect(),
            body: String::new(),
            source_path: PathBuf::from(format!("{id}.md")),
        }
    }

    fn yaml_str(s: &str) -> serde_yaml::Value {
        serde_yaml::Value::String(s.to_owned())
    }

    fn yaml_int(n: i64) -> serde_yaml::Value {
        serde_yaml::Value::Number(n.into())
    }

    fn yaml_float(n: f64) -> serde_yaml::Value {
        serde_yaml::to_value(n).unwrap()
    }

    fn yaml_bool(b: bool) -> serde_yaml::Value {
        serde_yaml::Value::Bool(b)
    }

    fn yaml_seq(items: Vec<serde_yaml::Value>) -> serde_yaml::Value {
        serde_yaml::Value::Sequence(items)
    }

    /// Assert that diagnostics contain exactly one InvalidFieldValue with the expected error kind.
    fn assert_field_error(diagnostics: &[Diagnostic], expected: fn(&FieldValueError) -> bool) {
        assert_eq!(diagnostics.len(), 1, "expected exactly one diagnostic");
        match &diagnostics[0].kind {
            DiagnosticKind::InvalidFieldValue { detail, .. } => {
                assert!(expected(detail), "unexpected error detail: {detail:?}");
            }
            other => panic!("expected InvalidFieldValue, got {other:?}"),
        }
    }

    // ── String coercion ──────────────────────────────────────────────

    #[test]
    fn coerce_string_valid() {
        let s = schema(vec![(
            "title",
            FieldDefinition::new(FieldTypeConfig::String { pattern: None }),
        )]);
        let raw = raw_item("t", vec![("title", yaml_str("Hello"))]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(diagnostics.is_empty());
        assert_eq!(fields["title"], FieldValue::String("Hello".into()));
    }

    #[test]
    fn coerce_string_rejects_number() {
        let s = schema(vec![(
            "title",
            FieldDefinition::new(FieldTypeConfig::String { pattern: None }),
        )]);
        let raw = raw_item("t", vec![("title", yaml_int(42))]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(fields.get("title").is_none());
        assert_field_error(&diagnostics, |e| {
            matches!(e, FieldValueError::TypeMismatch { .. })
        });
    }

    #[test]
    fn coerce_string_with_pattern() {
        let def = FieldDefinition::new(FieldTypeConfig::String {
            pattern: Some(r"^[A-Z]{3}-\d+$".to_owned()),
        });
        let s = schema(vec![("code", def)]);

        let raw = raw_item("t", vec![("code", yaml_str("ABC-123"))]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);
        assert!(diagnostics.is_empty());
        assert_eq!(fields["code"], FieldValue::String("ABC-123".into()));

        let raw_bad = raw_item("t", vec![("code", yaml_str("abc"))]);
        let (fields, diagnostics) = coerce_fields(&raw_bad, &s);
        assert!(fields.get("code").is_none());
        assert_field_error(&diagnostics, |e| {
            matches!(e, FieldValueError::PatternMismatch { .. })
        });
    }

    // ── Choice coercion ──────────────────────────────────────────────

    #[test]
    fn coerce_choice_valid() {
        let def = FieldDefinition::new(FieldTypeConfig::Choice {
            values: vec!["open".into(), "closed".into()],
        });
        let s = schema(vec![("status", def)]);
        let raw = raw_item("t", vec![("status", yaml_str("open"))]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(diagnostics.is_empty());
        assert_eq!(fields["status"], FieldValue::Choice("open".into()));
    }

    #[test]
    fn coerce_choice_invalid_value() {
        let def = FieldDefinition::new(FieldTypeConfig::Choice {
            values: vec!["open".into(), "closed".into()],
        });
        let s = schema(vec![("status", def)]);
        let raw = raw_item("t", vec![("status", yaml_str("unknown"))]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(fields.get("status").is_none());
        assert_field_error(&diagnostics, |e| {
            matches!(e, FieldValueError::InvalidChoice { .. })
        });
    }

    #[test]
    fn coerce_choice_rejects_number() {
        let def = FieldDefinition::new(FieldTypeConfig::Choice {
            values: vec!["open".into()],
        });
        let s = schema(vec![("status", def)]);
        let raw = raw_item("t", vec![("status", yaml_int(1))]);
        let (_, diagnostics) = coerce_fields(&raw, &s);

        assert_field_error(&diagnostics, |e| {
            matches!(e, FieldValueError::TypeMismatch { .. })
        });
    }

    // ── Multichoice coercion ─────────────────────────────────────────

    #[test]
    fn coerce_multichoice_valid() {
        let def = FieldDefinition::new(FieldTypeConfig::Multichoice {
            values: vec!["a".into(), "b".into(), "c".into()],
        });
        let s = schema(vec![("labels", def)]);
        let raw = raw_item(
            "t",
            vec![("labels", yaml_seq(vec![yaml_str("a"), yaml_str("b")]))],
        );
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(diagnostics.is_empty());
        assert_eq!(
            fields["labels"],
            FieldValue::Multichoice(vec!["a".into(), "b".into()])
        );
    }

    #[test]
    fn coerce_multichoice_invalid_values() {
        let def = FieldDefinition::new(FieldTypeConfig::Multichoice {
            values: vec!["a".into(), "b".into()],
        });
        let s = schema(vec![("labels", def)]);
        let raw = raw_item(
            "t",
            vec![("labels", yaml_seq(vec![yaml_str("a"), yaml_str("x")]))],
        );
        let (_, diagnostics) = coerce_fields(&raw, &s);

        assert_field_error(&diagnostics, |e| {
            matches!(e, FieldValueError::InvalidMultichoice { .. })
        });
    }

    #[test]
    fn coerce_multichoice_rejects_string() {
        let def = FieldDefinition::new(FieldTypeConfig::Multichoice {
            values: vec!["a".into()],
        });
        let s = schema(vec![("labels", def)]);
        let raw = raw_item("t", vec![("labels", yaml_str("a"))]);
        let (_, diagnostics) = coerce_fields(&raw, &s);

        assert_field_error(&diagnostics, |e| {
            matches!(e, FieldValueError::TypeMismatch { .. })
        });
    }

    // ── Integer coercion ─────────────────────────────────────────────

    #[test]
    fn coerce_integer_valid() {
        let s = schema(vec![(
            "priority",
            FieldDefinition::new(FieldTypeConfig::Integer {
                min: None,
                max: None,
            }),
        )]);
        let raw = raw_item("t", vec![("priority", yaml_int(42))]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(diagnostics.is_empty());
        assert_eq!(fields["priority"], FieldValue::Integer(42));
    }

    #[test]
    fn coerce_integer_out_of_range() {
        let def = FieldDefinition::new(FieldTypeConfig::Integer {
            min: Some(1.0),
            max: Some(10.0),
        });
        let s = schema(vec![("priority", def)]);

        let raw = raw_item("t", vec![("priority", yaml_int(0))]);
        let (_, diagnostics) = coerce_fields(&raw, &s);
        assert_field_error(&diagnostics, |e| {
            matches!(e, FieldValueError::OutOfRange { .. })
        });

        let raw = raw_item("t", vec![("priority", yaml_int(11))]);
        let (_, diagnostics) = coerce_fields(&raw, &s);
        assert_field_error(&diagnostics, |e| {
            matches!(e, FieldValueError::OutOfRange { .. })
        });
    }

    #[test]
    fn coerce_integer_rejects_string() {
        let s = schema(vec![(
            "priority",
            FieldDefinition::new(FieldTypeConfig::Integer {
                min: None,
                max: None,
            }),
        )]);
        let raw = raw_item("t", vec![("priority", yaml_str("high"))]);
        let (_, diagnostics) = coerce_fields(&raw, &s);

        assert_field_error(&diagnostics, |e| {
            matches!(e, FieldValueError::TypeMismatch { .. })
        });
    }

    // ── Float coercion ───────────────────────────────────────────────

    #[test]
    fn coerce_float_valid() {
        let s = schema(vec![(
            "weight",
            FieldDefinition::new(FieldTypeConfig::Float {
                min: None,
                max: None,
            }),
        )]);
        let raw = raw_item("t", vec![("weight", yaml_float(2.5))]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(diagnostics.is_empty());
        assert_eq!(fields["weight"], FieldValue::Float(2.5));
    }

    #[test]
    fn coerce_float_from_integer() {
        let s = schema(vec![(
            "weight",
            FieldDefinition::new(FieldTypeConfig::Float {
                min: None,
                max: None,
            }),
        )]);
        let raw = raw_item("t", vec![("weight", yaml_int(5))]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(diagnostics.is_empty());
        assert_eq!(fields["weight"], FieldValue::Float(5.0));
    }

    #[test]
    fn coerce_float_out_of_range() {
        let def = FieldDefinition::new(FieldTypeConfig::Float {
            min: Some(0.0),
            max: Some(1.0),
        });
        let s = schema(vec![("ratio", def)]);
        let raw = raw_item("t", vec![("ratio", yaml_float(1.5))]);
        let (_, diagnostics) = coerce_fields(&raw, &s);

        assert_field_error(&diagnostics, |e| {
            matches!(e, FieldValueError::OutOfRange { .. })
        });
    }

    // ── Date coercion ────────────────────────────────────────────────

    #[test]
    fn coerce_date_valid() {
        let s = schema(vec![(
            "created",
            FieldDefinition::new(FieldTypeConfig::Date),
        )]);
        let raw = raw_item("t", vec![("created", yaml_str("2026-01-15"))]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(diagnostics.is_empty());
        assert_eq!(
            fields["created"],
            FieldValue::Date(NaiveDate::from_ymd_opt(2026, 1, 15).unwrap())
        );
    }

    #[test]
    fn coerce_date_invalid_format() {
        let s = schema(vec![(
            "created",
            FieldDefinition::new(FieldTypeConfig::Date),
        )]);
        let raw = raw_item("t", vec![("created", yaml_str("01/15/2026"))]);
        let (_, diagnostics) = coerce_fields(&raw, &s);

        assert_field_error(&diagnostics, |e| {
            matches!(e, FieldValueError::InvalidDate { .. })
        });
    }

    #[test]
    fn coerce_date_invalid_day() {
        let s = schema(vec![(
            "created",
            FieldDefinition::new(FieldTypeConfig::Date),
        )]);
        let raw = raw_item("t", vec![("created", yaml_str("2026-02-30"))]);
        let (_, diagnostics) = coerce_fields(&raw, &s);

        assert_field_error(&diagnostics, |e| {
            matches!(e, FieldValueError::InvalidDate { .. })
        });
    }

    #[test]
    fn coerce_date_leap_year() {
        let s = schema(vec![(
            "created",
            FieldDefinition::new(FieldTypeConfig::Date),
        )]);

        let raw = raw_item("t", vec![("created", yaml_str("2024-02-29"))]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);
        assert!(diagnostics.is_empty());
        assert_eq!(
            fields["created"],
            FieldValue::Date(NaiveDate::from_ymd_opt(2024, 2, 29).unwrap())
        );

        let raw = raw_item("t", vec![("created", yaml_str("2023-02-29"))]);
        let (_, diagnostics) = coerce_fields(&raw, &s);
        assert!(!diagnostics.is_empty());
    }

    // ── Duration coercion ────────────────────────────────────────────

    #[test]
    fn coerce_duration_simple_days() {
        let s = schema(vec![(
            "estimate",
            FieldDefinition::new(FieldTypeConfig::Duration {
                min: None,
                max: None,
            }),
        )]);
        let raw = raw_item("t", vec![("estimate", yaml_str("5d"))]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(diagnostics.is_empty(), "got diagnostics: {diagnostics:?}");
        assert_eq!(fields["estimate"], FieldValue::Duration(432_000));
    }

    #[test]
    fn coerce_duration_compound() {
        let s = schema(vec![(
            "estimate",
            FieldDefinition::new(FieldTypeConfig::Duration {
                min: None,
                max: None,
            }),
        )]);
        let raw = raw_item("t", vec![("estimate", yaml_str("1w 2d 3h"))]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(diagnostics.is_empty());
        // 1w + 2d + 3h = 604_800 + 172_800 + 10_800 = 788_400
        assert_eq!(fields["estimate"], FieldValue::Duration(788_400));
    }

    #[test]
    fn coerce_duration_negative() {
        let s = schema(vec![(
            "estimate",
            FieldDefinition::new(FieldTypeConfig::Duration {
                min: None,
                max: None,
            }),
        )]);
        let raw = raw_item("t", vec![("estimate", yaml_str("-2d"))]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(diagnostics.is_empty());
        assert_eq!(fields["estimate"], FieldValue::Duration(-172_800));
    }

    #[test]
    fn coerce_duration_below_min_rejected() {
        let s = schema(vec![(
            "estimate",
            FieldDefinition::new(FieldTypeConfig::Duration {
                min: Some(0),
                max: None,
            }),
        )]);
        let raw = raw_item("t", vec![("estimate", yaml_str("-2d"))]);
        let (_, diagnostics) = coerce_fields(&raw, &s);

        assert_field_error(&diagnostics, |e| {
            matches!(e, FieldValueError::OutOfRangeDuration { .. })
        });
    }

    #[test]
    fn coerce_duration_above_max_rejected() {
        let s = schema(vec![(
            "estimate",
            FieldDefinition::new(FieldTypeConfig::Duration {
                min: None,
                max: Some(86_400),
            }),
        )]);
        let raw = raw_item("t", vec![("estimate", yaml_str("2d"))]);
        let (_, diagnostics) = coerce_fields(&raw, &s);

        assert_field_error(&diagnostics, |e| {
            matches!(e, FieldValueError::OutOfRangeDuration { .. })
        });
    }

    #[test]
    fn coerce_duration_bare_integer_rejected() {
        // Bare numeric YAML value rejects with TypeMismatch — design says
        // strings only, no magic interpretation of ints.
        let s = schema(vec![(
            "estimate",
            FieldDefinition::new(FieldTypeConfig::Duration {
                min: None,
                max: None,
            }),
        )]);
        let raw = raw_item("t", vec![("estimate", yaml_int(5))]);
        let (_, diagnostics) = coerce_fields(&raw, &s);

        assert_field_error(&diagnostics, |e| {
            matches!(e, FieldValueError::TypeMismatch { .. })
        });
    }

    #[test]
    fn coerce_duration_invalid_string_rejected() {
        let s = schema(vec![(
            "estimate",
            FieldDefinition::new(FieldTypeConfig::Duration {
                min: None,
                max: None,
            }),
        )]);
        let raw = raw_item("t", vec![("estimate", yaml_str("garbage"))]);
        let (_, diagnostics) = coerce_fields(&raw, &s);

        assert_field_error(&diagnostics, |e| {
            matches!(e, FieldValueError::InvalidDuration { .. })
        });
    }

    #[test]
    fn coerce_duration_unknown_unit_rejected() {
        // `5y` (years) — explicitly out of scope.
        let s = schema(vec![(
            "estimate",
            FieldDefinition::new(FieldTypeConfig::Duration {
                min: None,
                max: None,
            }),
        )]);
        let raw = raw_item("t", vec![("estimate", yaml_str("5y"))]);
        let (_, diagnostics) = coerce_fields(&raw, &s);

        assert_field_error(&diagnostics, |e| {
            matches!(e, FieldValueError::InvalidDuration { .. })
        });
    }

    // ── Boolean coercion ─────────────────────────────────────────────

    #[test]
    fn coerce_boolean_valid() {
        let s = schema(vec![(
            "active",
            FieldDefinition::new(FieldTypeConfig::Boolean),
        )]);
        let raw = raw_item("t", vec![("active", yaml_bool(true))]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(diagnostics.is_empty());
        assert_eq!(fields["active"], FieldValue::Boolean(true));
    }

    #[test]
    fn coerce_boolean_rejects_string() {
        let s = schema(vec![(
            "active",
            FieldDefinition::new(FieldTypeConfig::Boolean),
        )]);
        let raw = raw_item("t", vec![("active", yaml_str("true"))]);
        let (_, diagnostics) = coerce_fields(&raw, &s);

        assert_field_error(&diagnostics, |e| {
            matches!(e, FieldValueError::TypeMismatch { .. })
        });
    }

    // ── List coercion ────────────────────────────────────────────────

    #[test]
    fn coerce_list_valid() {
        let s = schema(vec![("tags", FieldDefinition::new(FieldTypeConfig::List))]);
        let raw = raw_item(
            "t",
            vec![("tags", yaml_seq(vec![yaml_str("a"), yaml_str("b")]))],
        );
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(diagnostics.is_empty());
        assert_eq!(
            fields["tags"],
            FieldValue::List(vec!["a".into(), "b".into()])
        );
    }

    #[test]
    fn coerce_list_rejects_non_string_elements() {
        let s = schema(vec![("tags", FieldDefinition::new(FieldTypeConfig::List))]);
        let raw = raw_item(
            "t",
            vec![("tags", yaml_seq(vec![yaml_str("a"), yaml_int(1)]))],
        );
        let (_, diagnostics) = coerce_fields(&raw, &s);

        assert_field_error(&diagnostics, |e| {
            matches!(e, FieldValueError::TypeMismatch { .. })
        });
    }

    // ── Link coercion ────────────────────────────────────────────────

    #[test]
    fn coerce_link_valid() {
        let s = schema(vec![(
            "parent",
            FieldDefinition::new(FieldTypeConfig::Link {
                allow_cycles: None,
                inverse: None,
            }),
        )]);
        let raw = raw_item("t", vec![("parent", yaml_str("auth-epic"))]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(diagnostics.is_empty());
        assert_eq!(
            fields["parent"],
            FieldValue::Link(WorkItemId::from("auth-epic".to_owned()))
        );
    }

    #[test]
    fn coerce_link_rejects_number() {
        let s = schema(vec![(
            "parent",
            FieldDefinition::new(FieldTypeConfig::Link {
                allow_cycles: None,
                inverse: None,
            }),
        )]);
        let raw = raw_item("t", vec![("parent", yaml_int(1))]);
        let (_, diagnostics) = coerce_fields(&raw, &s);

        assert_field_error(&diagnostics, |e| {
            matches!(e, FieldValueError::TypeMismatch { .. })
        });
    }

    // ── Links coercion ───────────────────────────────────────────────

    #[test]
    fn coerce_links_valid() {
        let s = schema(vec![(
            "depends_on",
            FieldDefinition::new(FieldTypeConfig::Links {
                allow_cycles: None,
                inverse: None,
            }),
        )]);
        let raw = raw_item(
            "t",
            vec![("depends_on", yaml_seq(vec![yaml_str("a"), yaml_str("b")]))],
        );
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(diagnostics.is_empty());
        assert_eq!(
            fields["depends_on"],
            FieldValue::Links(vec![
                WorkItemId::from("a".to_owned()),
                WorkItemId::from("b".to_owned())
            ])
        );
    }

    #[test]
    fn coerce_links_rejects_string() {
        let s = schema(vec![(
            "depends_on",
            FieldDefinition::new(FieldTypeConfig::Links {
                allow_cycles: None,
                inverse: None,
            }),
        )]);
        let raw = raw_item("t", vec![("depends_on", yaml_str("a"))]);
        let (_, diagnostics) = coerce_fields(&raw, &s);

        assert_field_error(&diagnostics, |e| {
            matches!(e, FieldValueError::TypeMismatch { .. })
        });
    }

    // ── Cross-cutting concerns ───────────────────────────────────────

    #[test]
    fn unknown_field_produces_warning() {
        let s = schema(vec![(
            "title",
            FieldDefinition::new(FieldTypeConfig::String { pattern: None }),
        )]);
        let raw = raw_item(
            "t",
            vec![("title", yaml_str("Hi")), ("bogus", yaml_str("x"))],
        );
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert_eq!(fields.len(), 1);
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Warning
                && matches!(
                    &diagnostic.kind,
                    DiagnosticKind::UnknownField { field, .. } if field == "bogus"
                )
        }));
    }

    #[test]
    fn missing_required_field() {
        let mut def = FieldDefinition::new(FieldTypeConfig::String { pattern: None });
        def.required = true;
        let s = schema(vec![("title", def)]);
        let raw = raw_item("t", vec![]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(fields.is_empty());
        assert!(diagnostics.iter().any(|diagnostic| matches!(
            &diagnostic.kind,
            DiagnosticKind::MissingRequired { field, .. } if field == "title"
        )));
    }

    #[test]
    fn null_value_treated_as_absent() {
        let mut def = FieldDefinition::new(FieldTypeConfig::String { pattern: None });
        def.required = true;
        let s = schema(vec![("title", def)]);
        let raw = raw_item("t", vec![("title", serde_yaml::Value::Null)]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(fields.get("title").is_none());
        assert!(diagnostics.iter().any(|diagnostic| matches!(
            &diagnostic.kind,
            DiagnosticKind::MissingRequired { field, .. } if field == "title"
        )));
    }

    #[test]
    fn id_field_skipped() {
        let s = schema(vec![
            (
                "id",
                FieldDefinition::new(FieldTypeConfig::String { pattern: None }),
            ),
            (
                "title",
                FieldDefinition::new(FieldTypeConfig::String { pattern: None }),
            ),
        ]);
        let raw = raw_item("t", vec![("title", yaml_str("Hi"))]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(diagnostics.is_empty());
        assert!(fields.get("id").is_none());
        assert_eq!(fields["title"], FieldValue::String("Hi".into()));
    }

    #[test]
    fn multiple_errors_collected() {
        let mut title_def = FieldDefinition::new(FieldTypeConfig::String { pattern: None });
        title_def.required = true;
        let status_def = FieldDefinition::new(FieldTypeConfig::Choice {
            values: vec!["open".into()],
        });
        let s = schema(vec![("title", title_def), ("status", status_def)]);

        // title is missing (required), status has wrong value
        let raw = raw_item("t", vec![("status", yaml_str("invalid"))]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(fields.is_empty());
        assert_eq!(diagnostics.len(), 2);
    }
}
