//! Field coercion: convert raw `serde_yaml::Value` fields into typed [`FieldValue`]s.
//!
//! Operates on a single [`crate::parser::RawWorkItem`] and the project [`Schema`].
//! Produces a map of successfully coerced fields plus a list of
//! [`Diagnostic`]s for fields that failed coercion or violated constraints.

use std::collections::HashMap;

use regex::Regex;

use crate::model::diagnostic::{Diagnostic, DiagnosticKind, FieldValueError};
use crate::model::schema::{FieldDefinition, FieldType, Schema, Severity};
use crate::model::FieldValue;
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
                // Value is absent or null.
                if def.required {
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
    match def.field_type {
        FieldType::String => coerce_string(value, def),
        FieldType::Choice => coerce_choice(value, def),
        FieldType::Multichoice => coerce_multichoice(value, def),
        FieldType::Integer => coerce_integer(value, def),
        FieldType::Float => coerce_float(value, def),
        FieldType::Date => coerce_date(value),
        FieldType::Boolean => coerce_boolean(value),
        FieldType::List => coerce_list(value),
        FieldType::Link => coerce_link(value),
        FieldType::Links => coerce_links(value),
    }
}

// ── Per-type coercion ────────────────────────────────────────────────

fn coerce_string(
    value: &serde_yaml::Value,
    def: &FieldDefinition,
) -> Result<FieldValue, FieldValueError> {
    let s = value
        .as_str()
        .ok_or_else(|| FieldValueError::TypeMismatch {
            expected: FieldType::String,
            got: yaml_type_name(value).into(),
        })?;

    if let Some(pattern) = &def.pattern {
        let re = Regex::new(pattern).map_err(|e| FieldValueError::InvalidPattern {
            pattern: pattern.clone(),
            error: e.to_string(),
        })?;
        if !re.is_match(s) {
            return Err(FieldValueError::PatternMismatch {
                value: s.to_owned(),
                pattern: pattern.clone(),
            });
        }
    }

    Ok(FieldValue::String(s.to_owned()))
}

fn coerce_choice(
    value: &serde_yaml::Value,
    def: &FieldDefinition,
) -> Result<FieldValue, FieldValueError> {
    let s = value
        .as_str()
        .ok_or_else(|| FieldValueError::TypeMismatch {
            expected: FieldType::Choice,
            got: yaml_type_name(value).into(),
        })?;

    if let Some(values) = &def.values {
        if !values.iter().any(|v| v == s) {
            return Err(FieldValueError::InvalidChoice {
                value: s.to_owned(),
                allowed: values.clone(),
            });
        }
    }

    Ok(FieldValue::Choice(s.to_owned()))
}

fn coerce_multichoice(
    value: &serde_yaml::Value,
    def: &FieldDefinition,
) -> Result<FieldValue, FieldValueError> {
    let seq = value
        .as_sequence()
        .ok_or_else(|| FieldValueError::TypeMismatch {
            expected: FieldType::Multichoice,
            got: yaml_type_name(value).into(),
        })?;

    let mut result = Vec::with_capacity(seq.len());
    for item in seq {
        let s = item
            .as_str()
            .ok_or_else(|| FieldValueError::TypeMismatch {
                expected: FieldType::Multichoice,
                got: format!("sequence containing {}", yaml_type_name(item)),
            })?;
        result.push(s.to_owned());
    }

    if let Some(allowed) = &def.values {
        let invalid: Vec<String> = result
            .iter()
            .filter(|v| !allowed.contains(v))
            .cloned()
            .collect();
        if !invalid.is_empty() {
            return Err(FieldValueError::InvalidMultichoice {
                values: invalid,
                allowed: allowed.clone(),
            });
        }
    }

    Ok(FieldValue::Multichoice(result))
}

fn coerce_integer(
    value: &serde_yaml::Value,
    def: &FieldDefinition,
) -> Result<FieldValue, FieldValueError> {
    let n = value
        .as_i64()
        .ok_or_else(|| FieldValueError::TypeMismatch {
            expected: FieldType::Integer,
            got: yaml_type_name(value).into(),
        })?;

    if let Some(min) = def.min {
        if (n as f64) < min {
            return Err(FieldValueError::OutOfRange {
                value: n as f64,
                min: Some(min),
                max: def.max,
            });
        }
    }
    if let Some(max) = def.max {
        if (n as f64) > max {
            return Err(FieldValueError::OutOfRange {
                value: n as f64,
                min: def.min,
                max: Some(max),
            });
        }
    }

    Ok(FieldValue::Integer(n))
}

fn coerce_float(
    value: &serde_yaml::Value,
    def: &FieldDefinition,
) -> Result<FieldValue, FieldValueError> {
    let n = value
        .as_f64()
        .ok_or_else(|| FieldValueError::TypeMismatch {
            expected: FieldType::Float,
            got: yaml_type_name(value).into(),
        })?;

    if let Some(min) = def.min {
        if n < min {
            return Err(FieldValueError::OutOfRange {
                value: n,
                min: Some(min),
                max: def.max,
            });
        }
    }
    if let Some(max) = def.max {
        if n > max {
            return Err(FieldValueError::OutOfRange {
                value: n,
                min: def.min,
                max: Some(max),
            });
        }
    }

    Ok(FieldValue::Float(n))
}

fn coerce_date(value: &serde_yaml::Value) -> Result<FieldValue, FieldValueError> {
    let s = value
        .as_str()
        .ok_or_else(|| FieldValueError::TypeMismatch {
            expected: FieldType::Date,
            got: yaml_type_name(value).into(),
        })?;

    if !is_valid_date(s) {
        return Err(FieldValueError::InvalidDate {
            value: s.to_owned(),
        });
    }

    Ok(FieldValue::Date(s.to_owned()))
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
        let s = item
            .as_str()
            .ok_or_else(|| FieldValueError::TypeMismatch {
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

    Ok(FieldValue::Link(s.to_owned()))
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
        let s = item
            .as_str()
            .ok_or_else(|| FieldValueError::TypeMismatch {
                expected: FieldType::Links,
                got: format!("sequence containing {}", yaml_type_name(item)),
            })?;
        result.push(s.to_owned());
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

/// Validate a date string matches `YYYY-MM-DD` and represents a plausible date.
fn is_valid_date(s: &str) -> bool {
    if s.len() != 10 {
        return false;
    }

    let bytes = s.as_bytes();
    if bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }

    let year: u32 = match s[0..4].parse() {
        Ok(y) => y,
        Err(_) => return false,
    };
    let month: u32 = match s[5..7].parse() {
        Ok(m) => m,
        Err(_) => return false,
    };
    let day: u32 = match s[8..10].parse() {
        Ok(d) => d,
        Err(_) => return false,
    };

    if year == 0 || month == 0 || month > 12 || day == 0 {
        return false;
    }

    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400) {
                29
            } else {
                28
            }
        }
        _ => return false,
    };

    day <= max_day
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::FieldDefinition;
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

    /// Build a minimal FieldDefinition for a given type.
    fn field(field_type: FieldType) -> FieldDefinition {
        FieldDefinition {
            field_type,
            description: None,
            required: false,
            default: None,
            values: None,
            pattern: None,
            min: None,
            max: None,
            allow_cycles: None,
            inverse: None,
            resource: None,
            aggregate: None,
        }
    }

    /// Build a RawWorkItem with the given frontmatter.
    fn raw_item(id: &str, frontmatter: Vec<(&str, serde_yaml::Value)>) -> RawWorkItem {
        RawWorkItem {
            id: id.to_owned(),
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
        let s = schema(vec![("title", field(FieldType::String))]);
        let raw = raw_item("t", vec![("title", yaml_str("Hello"))]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(diagnostics.is_empty());
        assert_eq!(fields["title"], FieldValue::String("Hello".into()));
    }

    #[test]
    fn coerce_string_rejects_number() {
        let s = schema(vec![("title", field(FieldType::String))]);
        let raw = raw_item("t", vec![("title", yaml_int(42))]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(fields.get("title").is_none());
        assert_field_error(&diagnostics, |e| {
            matches!(e, FieldValueError::TypeMismatch { .. })
        });
    }

    #[test]
    fn coerce_string_with_pattern() {
        let mut def = field(FieldType::String);
        def.pattern = Some(r"^[A-Z]{3}-\d+$".to_owned());
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
        let mut def = field(FieldType::Choice);
        def.values = Some(vec!["open".into(), "closed".into()]);
        let s = schema(vec![("status", def)]);
        let raw = raw_item("t", vec![("status", yaml_str("open"))]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(diagnostics.is_empty());
        assert_eq!(fields["status"], FieldValue::Choice("open".into()));
    }

    #[test]
    fn coerce_choice_invalid_value() {
        let mut def = field(FieldType::Choice);
        def.values = Some(vec!["open".into(), "closed".into()]);
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
        let mut def = field(FieldType::Choice);
        def.values = Some(vec!["open".into()]);
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
        let mut def = field(FieldType::Multichoice);
        def.values = Some(vec!["a".into(), "b".into(), "c".into()]);
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
        let mut def = field(FieldType::Multichoice);
        def.values = Some(vec!["a".into(), "b".into()]);
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
        let mut def = field(FieldType::Multichoice);
        def.values = Some(vec!["a".into()]);
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
        let s = schema(vec![("priority", field(FieldType::Integer))]);
        let raw = raw_item("t", vec![("priority", yaml_int(42))]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(diagnostics.is_empty());
        assert_eq!(fields["priority"], FieldValue::Integer(42));
    }

    #[test]
    fn coerce_integer_out_of_range() {
        let mut def = field(FieldType::Integer);
        def.min = Some(1.0);
        def.max = Some(10.0);
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
        let s = schema(vec![("priority", field(FieldType::Integer))]);
        let raw = raw_item("t", vec![("priority", yaml_str("high"))]);
        let (_, diagnostics) = coerce_fields(&raw, &s);

        assert_field_error(&diagnostics, |e| {
            matches!(e, FieldValueError::TypeMismatch { .. })
        });
    }

    // ── Float coercion ───────────────────────────────────────────────

    #[test]
    fn coerce_float_valid() {
        let s = schema(vec![("weight", field(FieldType::Float))]);
        let raw = raw_item("t", vec![("weight", yaml_float(3.14))]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(diagnostics.is_empty());
        assert_eq!(fields["weight"], FieldValue::Float(3.14));
    }

    #[test]
    fn coerce_float_from_integer() {
        let s = schema(vec![("weight", field(FieldType::Float))]);
        let raw = raw_item("t", vec![("weight", yaml_int(5))]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(diagnostics.is_empty());
        assert_eq!(fields["weight"], FieldValue::Float(5.0));
    }

    #[test]
    fn coerce_float_out_of_range() {
        let mut def = field(FieldType::Float);
        def.min = Some(0.0);
        def.max = Some(1.0);
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
        let s = schema(vec![("created", field(FieldType::Date))]);
        let raw = raw_item("t", vec![("created", yaml_str("2026-01-15"))]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(diagnostics.is_empty());
        assert_eq!(fields["created"], FieldValue::Date("2026-01-15".into()));
    }

    #[test]
    fn coerce_date_invalid_format() {
        let s = schema(vec![("created", field(FieldType::Date))]);
        let raw = raw_item("t", vec![("created", yaml_str("01/15/2026"))]);
        let (_, diagnostics) = coerce_fields(&raw, &s);

        assert_field_error(&diagnostics, |e| {
            matches!(e, FieldValueError::InvalidDate { .. })
        });
    }

    #[test]
    fn coerce_date_invalid_day() {
        let s = schema(vec![("created", field(FieldType::Date))]);
        let raw = raw_item("t", vec![("created", yaml_str("2026-02-30"))]);
        let (_, diagnostics) = coerce_fields(&raw, &s);

        assert_field_error(&diagnostics, |e| {
            matches!(e, FieldValueError::InvalidDate { .. })
        });
    }

    #[test]
    fn coerce_date_leap_year() {
        let s = schema(vec![("created", field(FieldType::Date))]);

        let raw = raw_item("t", vec![("created", yaml_str("2024-02-29"))]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);
        assert!(diagnostics.is_empty());
        assert_eq!(fields["created"], FieldValue::Date("2024-02-29".into()));

        let raw = raw_item("t", vec![("created", yaml_str("2023-02-29"))]);
        let (_, diagnostics) = coerce_fields(&raw, &s);
        assert!(!diagnostics.is_empty());
    }

    // ── Boolean coercion ─────────────────────────────────────────────

    #[test]
    fn coerce_boolean_valid() {
        let s = schema(vec![("active", field(FieldType::Boolean))]);
        let raw = raw_item("t", vec![("active", yaml_bool(true))]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(diagnostics.is_empty());
        assert_eq!(fields["active"], FieldValue::Boolean(true));
    }

    #[test]
    fn coerce_boolean_rejects_string() {
        let s = schema(vec![("active", field(FieldType::Boolean))]);
        let raw = raw_item("t", vec![("active", yaml_str("true"))]);
        let (_, diagnostics) = coerce_fields(&raw, &s);

        assert_field_error(&diagnostics, |e| {
            matches!(e, FieldValueError::TypeMismatch { .. })
        });
    }

    // ── List coercion ────────────────────────────────────────────────

    #[test]
    fn coerce_list_valid() {
        let s = schema(vec![("tags", field(FieldType::List))]);
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
        let s = schema(vec![("tags", field(FieldType::List))]);
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
        let s = schema(vec![("parent", field(FieldType::Link))]);
        let raw = raw_item("t", vec![("parent", yaml_str("auth-epic"))]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(diagnostics.is_empty());
        assert_eq!(fields["parent"], FieldValue::Link("auth-epic".into()));
    }

    #[test]
    fn coerce_link_rejects_number() {
        let s = schema(vec![("parent", field(FieldType::Link))]);
        let raw = raw_item("t", vec![("parent", yaml_int(1))]);
        let (_, diagnostics) = coerce_fields(&raw, &s);

        assert_field_error(&diagnostics, |e| {
            matches!(e, FieldValueError::TypeMismatch { .. })
        });
    }

    // ── Links coercion ───────────────────────────────────────────────

    #[test]
    fn coerce_links_valid() {
        let s = schema(vec![("depends_on", field(FieldType::Links))]);
        let raw = raw_item(
            "t",
            vec![("depends_on", yaml_seq(vec![yaml_str("a"), yaml_str("b")]))],
        );
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(diagnostics.is_empty());
        assert_eq!(
            fields["depends_on"],
            FieldValue::Links(vec!["a".into(), "b".into()])
        );
    }

    #[test]
    fn coerce_links_rejects_string() {
        let s = schema(vec![("depends_on", field(FieldType::Links))]);
        let raw = raw_item("t", vec![("depends_on", yaml_str("a"))]);
        let (_, diagnostics) = coerce_fields(&raw, &s);

        assert_field_error(&diagnostics, |e| {
            matches!(e, FieldValueError::TypeMismatch { .. })
        });
    }

    // ── Cross-cutting concerns ───────────────────────────────────────

    #[test]
    fn unknown_field_produces_warning() {
        let s = schema(vec![("title", field(FieldType::String))]);
        let raw = raw_item(
            "t",
            vec![("title", yaml_str("Hi")), ("bogus", yaml_str("x"))],
        );
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert_eq!(fields.len(), 1);
        assert!(diagnostics.iter().any(|d| {
            d.severity == Severity::Warning
                && matches!(
                    &d.kind,
                    DiagnosticKind::UnknownField { field, .. } if field == "bogus"
                )
        }));
    }

    #[test]
    fn missing_required_field() {
        let mut def = field(FieldType::String);
        def.required = true;
        let s = schema(vec![("title", def)]);
        let raw = raw_item("t", vec![]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(fields.is_empty());
        assert!(diagnostics.iter().any(|d| matches!(
            &d.kind,
            DiagnosticKind::MissingRequired { field, .. } if field == "title"
        )));
    }

    #[test]
    fn null_value_treated_as_absent() {
        let mut def = field(FieldType::String);
        def.required = true;
        let s = schema(vec![("title", def)]);
        let raw = raw_item("t", vec![("title", serde_yaml::Value::Null)]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(fields.get("title").is_none());
        assert!(diagnostics.iter().any(|d| matches!(
            &d.kind,
            DiagnosticKind::MissingRequired { field, .. } if field == "title"
        )));
    }

    #[test]
    fn id_field_skipped() {
        let s = schema(vec![
            ("id", field(FieldType::String)),
            ("title", field(FieldType::String)),
        ]);
        let raw = raw_item("t", vec![("title", yaml_str("Hi"))]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(diagnostics.is_empty());
        assert!(fields.get("id").is_none());
        assert_eq!(fields["title"], FieldValue::String("Hi".into()));
    }

    #[test]
    fn multiple_errors_collected() {
        let mut title_def = field(FieldType::String);
        title_def.required = true;
        let mut status_def = field(FieldType::Choice);
        status_def.values = Some(vec!["open".into()]);
        let s = schema(vec![("title", title_def), ("status", status_def)]);

        // title is missing (required), status has wrong value
        let raw = raw_item("t", vec![("status", yaml_str("invalid"))]);
        let (fields, diagnostics) = coerce_fields(&raw, &s);

        assert!(fields.is_empty());
        assert_eq!(diagnostics.len(), 2);
    }

    // ── Date validation helpers ──────────────────────────────────────

    #[test]
    fn date_validation() {
        assert!(is_valid_date("2026-01-01"));
        assert!(is_valid_date("2026-12-31"));
        assert!(is_valid_date("2024-02-29")); // leap year
        assert!(!is_valid_date("2023-02-29")); // not a leap year
        assert!(!is_valid_date("2026-13-01")); // invalid month
        assert!(!is_valid_date("2026-00-01")); // zero month
        assert!(!is_valid_date("2026-01-32")); // invalid day
        assert!(!is_valid_date("2026-1-1")); // wrong format
        assert!(!is_valid_date("not-a-date"));
        assert!(!is_valid_date(""));
    }
}
