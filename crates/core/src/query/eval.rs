//! Predicate evaluation: check whether a work item matches a predicate.
//!
//! The evaluator is type-aware — it uses the schema to determine how to
//! compare field values (numeric for integers, lexicographic for strings, etc.).

use crate::model::duration::{format_duration_seconds, parse_duration};
use crate::model::schema::{FieldType, Schema};
use crate::model::{FieldValue, WorkItem};
use crate::query::types::{Comparison, FieldReference, Operator, Predicate};
use crate::resolve::{resolve_field_ref, ResolvedValues};
use crate::store::Store;

// ── Error ───────────────────────────────────────────────────────────

/// Errors produced during predicate evaluation.
#[derive(Debug, thiserror::Error)]
pub enum QueryEvalError {
    #[error("invalid regex: {0}")]
    InvalidRegex(String),

    #[error("'{relation}' is not a relation field (type {actual_type:?}); dot notation requires a link or links field or an inverse relation")]
    NotARelation {
        relation: String,
        actual_type: FieldType,
    },

    #[error("'{relation}' is not a defined field or inverse relation")]
    UnknownRelation { relation: String },
}

// ── Public API ──────────────────────────────────────────────────────

/// Evaluate a predicate against a work item.
///
/// Returns `true` if the item matches the predicate.
pub fn matches_predicate(
    item: &WorkItem,
    predicate: &Predicate,
    schema: &Schema,
    store: &Store,
) -> Result<bool, QueryEvalError> {
    match predicate {
        Predicate::Comparison(comparison) => eval_comparison(item, comparison, schema, store),
        Predicate::And(predicates) => {
            for predicate in predicates {
                if !matches_predicate(item, predicate, schema, store)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Predicate::Or(predicates) => {
            for predicate in predicates {
                if matches_predicate(item, predicate, schema, store)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Predicate::Not(inner) => Ok(!matches_predicate(item, inner, schema, store)?),
    }
}

// ── Comparison evaluation ───────────────────────────────────────────

fn eval_comparison(
    item: &WorkItem,
    comparison: &Comparison,
    schema: &Schema,
    store: &Store,
) -> Result<bool, QueryEvalError> {
    match &comparison.field {
        FieldReference::Local(name) => {
            let field_value = item.fields.get(name);
            let field_type = schema
                .fields
                .get(name)
                .map(|definition| definition.field_type());
            eval_single(field_value, field_type, comparison)
        }
        FieldReference::Related { relation, field } => {
            validate_relation(relation, schema)?;

            let reference = format!("{relation}.{field}");
            let resolved = resolve_field_ref(item, &reference, schema, store);

            // Use the target field's schema type for type-aware comparison.
            let field_type = schema
                .fields
                .get(field.as_str())
                .map(|definition| definition.field_type());

            match resolved {
                ResolvedValues::Single(field_value) => {
                    eval_single(field_value, field_type, comparison)
                }
                ResolvedValues::Many(values) => {
                    // "Any" semantics: matches if at least one resolved
                    // value satisfies the predicate.
                    for value in values.iter() {
                        if eval_single(*value, field_type, comparison)? {
                            return Ok(true);
                        }
                    }
                    Ok(false)
                }
            }
        }
    }
}

/// Validate that a relation segment (the part before the dot) is a link,
/// links, or defined inverse. Returns an error otherwise.
fn validate_relation(relation: &str, schema: &Schema) -> Result<(), QueryEvalError> {
    if let Some(field_def) = schema.fields.get(relation) {
        let field_type = field_def.field_type();
        return match field_type {
            FieldType::Link | FieldType::Links => Ok(()),
            _ => Err(QueryEvalError::NotARelation {
                relation: relation.to_owned(),
                actual_type: field_type,
            }),
        };
    }
    if schema.inverse_table.contains_key(relation) {
        return Ok(());
    }
    Err(QueryEvalError::UnknownRelation {
        relation: relation.to_owned(),
    })
}

/// Evaluate a comparison against a single resolved field value. Used for
/// both local fields and each value produced by a related-field lookup.
fn eval_single(
    field_value: Option<&FieldValue>,
    field_type: Option<FieldType>,
    comparison: &Comparison,
) -> Result<bool, QueryEvalError> {
    // IsSet / IsNotSet don't need a value.
    match comparison.operator {
        Operator::IsSet => return Ok(field_value.is_some()),
        Operator::IsNotSet => return Ok(field_value.is_none()),
        _ => {}
    }

    let field_value = match field_value {
        Some(value) => value,
        None => return Ok(false),
    };

    match field_type {
        Some(FieldType::Integer) => eval_integer(field_value, comparison),
        Some(FieldType::Float) => eval_float(field_value, comparison),
        Some(FieldType::Boolean) => eval_boolean(field_value, comparison),
        Some(FieldType::Duration) => eval_duration(field_value, comparison),
        Some(FieldType::Multichoice) | Some(FieldType::List) => eval_list(field_value, comparison),
        Some(FieldType::Links) => eval_links(field_value, comparison),
        // String, Choice, Date, Link, and unknown fields all use string comparison.
        _ => eval_string(field_value, comparison),
    }
}

// ── Type-specific evaluation ────────────────────────────────────────

/// String-like comparison: String, Choice, Date, Link, and unknown fields.
fn eval_string(field_value: &FieldValue, comparison: &Comparison) -> Result<bool, QueryEvalError> {
    let actual = extract_string(field_value);
    let expected = &comparison.value;

    match comparison.operator {
        Operator::Equal => Ok(actual == *expected),
        Operator::NotEqual => Ok(actual != *expected),
        Operator::GreaterThan => Ok(actual.as_str() > expected.as_str()),
        Operator::LessThan => Ok(actual.as_str() < expected.as_str()),
        Operator::GreaterOrEqual => Ok(actual.as_str() >= expected.as_str()),
        Operator::LessOrEqual => Ok(actual.as_str() <= expected.as_str()),
        Operator::Contains => Ok(actual.contains(expected.as_str())),
        Operator::Matches => eval_regex(&actual, expected),
        Operator::IsSet | Operator::IsNotSet => unreachable!("handled above"),
    }
}

/// Integer comparison.
fn eval_integer(field_value: &FieldValue, comparison: &Comparison) -> Result<bool, QueryEvalError> {
    let actual = match field_value {
        FieldValue::Integer(number) => *number,
        _ => return Ok(false),
    };
    let expected = match comparison.value.parse::<i64>() {
        Ok(number) => number,
        Err(_) => return Ok(false),
    };

    Ok(match comparison.operator {
        Operator::Equal => actual == expected,
        Operator::NotEqual => actual != expected,
        Operator::GreaterThan => actual > expected,
        Operator::LessThan => actual < expected,
        Operator::GreaterOrEqual => actual >= expected,
        Operator::LessOrEqual => actual <= expected,
        Operator::Contains | Operator::Matches => false,
        Operator::IsSet | Operator::IsNotSet => unreachable!("handled above"),
    })
}

/// Float comparison.
fn eval_float(field_value: &FieldValue, comparison: &Comparison) -> Result<bool, QueryEvalError> {
    let actual = match field_value {
        FieldValue::Float(number) => *number,
        _ => return Ok(false),
    };
    let expected = match comparison.value.parse::<f64>() {
        Ok(number) => number,
        Err(_) => return Ok(false),
    };

    Ok(match comparison.operator {
        Operator::Equal => actual == expected,
        Operator::NotEqual => actual != expected,
        Operator::GreaterThan => actual > expected,
        Operator::LessThan => actual < expected,
        Operator::GreaterOrEqual => actual >= expected,
        Operator::LessOrEqual => actual <= expected,
        Operator::Contains | Operator::Matches => false,
        Operator::IsSet | Operator::IsNotSet => unreachable!("handled above"),
    })
}

/// Duration comparison. Mirrors `eval_integer`: the value is compared
/// as canonical i64 seconds. The RHS string is parsed via the same
/// suffix-shorthand grammar used everywhere else (`5d`, `1w 2d`, etc.).
fn eval_duration(
    field_value: &FieldValue,
    comparison: &Comparison,
) -> Result<bool, QueryEvalError> {
    let actual = match field_value {
        FieldValue::Duration(seconds) => *seconds,
        _ => return Ok(false),
    };
    let expected = match parse_duration(&comparison.value) {
        Ok(seconds) => seconds,
        Err(_) => return Ok(false),
    };

    Ok(match comparison.operator {
        Operator::Equal => actual == expected,
        Operator::NotEqual => actual != expected,
        Operator::GreaterThan => actual > expected,
        Operator::LessThan => actual < expected,
        Operator::GreaterOrEqual => actual >= expected,
        Operator::LessOrEqual => actual <= expected,
        Operator::Contains | Operator::Matches => false,
        Operator::IsSet | Operator::IsNotSet => unreachable!("handled above"),
    })
}

/// Boolean comparison.
fn eval_boolean(field_value: &FieldValue, comparison: &Comparison) -> Result<bool, QueryEvalError> {
    let actual = match field_value {
        FieldValue::Boolean(flag) => *flag,
        _ => return Ok(false),
    };
    let expected = match comparison.value.as_str() {
        "true" => true,
        "false" => false,
        _ => return Ok(false),
    };

    Ok(match comparison.operator {
        Operator::Equal => actual == expected,
        Operator::NotEqual => actual != expected,
        // Ordering/contains/regex don't make sense for booleans.
        _ => false,
    })
}

/// List-like comparison: Multichoice and List fields.
/// Equal checks membership (any element equals the value).
fn eval_list(field_value: &FieldValue, comparison: &Comparison) -> Result<bool, QueryEvalError> {
    let elements: Vec<&str> = match field_value {
        FieldValue::Multichoice(values) => values.iter().map(|string| string.as_str()).collect(),
        FieldValue::List(values) => values.iter().map(|string| string.as_str()).collect(),
        _ => return Ok(false),
    };
    let expected = &comparison.value;

    match comparison.operator {
        Operator::Equal => Ok(elements.contains(&expected.as_str())),
        Operator::NotEqual => Ok(!elements.contains(&expected.as_str())),
        Operator::Contains => Ok(elements
            .iter()
            .any(|element| element.contains(expected.as_str()))),
        Operator::Matches => {
            let compiled_regex = compile_regex(expected)?;
            Ok(elements
                .iter()
                .any(|element| compiled_regex.is_match(element)))
        }
        // Ordering doesn't make sense for lists.
        _ => Ok(false),
    }
}

/// Links comparison: same as list but extracts strings from WorkItemIds.
fn eval_links(field_value: &FieldValue, comparison: &Comparison) -> Result<bool, QueryEvalError> {
    let elements: Vec<&str> = match field_value {
        FieldValue::Links(ids) => ids.iter().map(|id| id.as_str()).collect(),
        _ => return Ok(false),
    };
    let expected = &comparison.value;

    match comparison.operator {
        Operator::Equal => Ok(elements.contains(&expected.as_str())),
        Operator::NotEqual => Ok(!elements.contains(&expected.as_str())),
        Operator::Contains => Ok(elements
            .iter()
            .any(|element| element.contains(expected.as_str()))),
        Operator::Matches => {
            let compiled_regex = compile_regex(expected)?;
            Ok(elements
                .iter()
                .any(|element| compiled_regex.is_match(element)))
        }
        _ => Ok(false),
    }
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Extract the string content from any string-like FieldValue.
fn extract_string(value: &FieldValue) -> String {
    match value {
        FieldValue::String(string) => string.clone(),
        FieldValue::Choice(string) => string.clone(),
        FieldValue::Date(date) => date.format("%Y-%m-%d").to_string(),
        FieldValue::Duration(seconds) => format_duration_seconds(*seconds),
        FieldValue::Link(id) => id.as_str().to_owned(),
        // For non-string types, fall back to a reasonable string representation.
        FieldValue::Integer(number) => number.to_string(),
        FieldValue::Float(number) => number.to_string(),
        FieldValue::Boolean(flag) => flag.to_string(),
        FieldValue::Multichoice(values) => values.join(", "),
        FieldValue::List(values) => values.join(", "),
        FieldValue::Links(ids) => ids
            .iter()
            .map(|id| id.as_str())
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// Evaluate a regex match. The value is stored as `/pattern/flags`.
fn eval_regex(haystack: &str, regex_value: &str) -> Result<bool, QueryEvalError> {
    let compiled_regex = compile_regex(regex_value)?;
    Ok(compiled_regex.is_match(haystack))
}

/// Compile a regex from the stored `/pattern/flags` format.
fn compile_regex(regex_value: &str) -> Result<regex::Regex, QueryEvalError> {
    let (pattern, flags) = parse_regex_value(regex_value);
    let full_pattern = if flags.contains('i') {
        format!("(?i){pattern}")
    } else {
        pattern.to_owned()
    };
    regex::Regex::new(&full_pattern)
        .map_err(|error| QueryEvalError::InvalidRegex(error.to_string()))
}

/// Parse `/pattern/flags` into (pattern, flags). If the format doesn't
/// match, treat the whole string as a pattern with no flags.
fn parse_regex_value(value: &str) -> (&str, &str) {
    if let Some(inner) = value.strip_prefix('/') {
        if let Some(closing) = inner.rfind('/') {
            let pattern = &inner[..closing];
            let flags = &inner[closing + 1..];
            return (pattern, flags);
        }
    }
    (value, "")
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::{FieldDefinition, FieldTypeConfig};
    use crate::model::WorkItemId;
    use crate::query::types::FieldReference;
    use indexmap::IndexMap;
    use std::path::PathBuf;

    /// Build an empty store for tests that only use local fields.
    fn empty_store(schema: &Schema) -> Store {
        let dir = tempfile::tempdir().unwrap();
        Store::load(dir.path(), schema).unwrap()
    }

    /// Wrapper: evaluates a predicate using an empty store. Use for tests
    /// that only exercise local-field predicates.
    fn check(
        item: &WorkItem,
        predicate: &Predicate,
        schema: &Schema,
    ) -> Result<bool, QueryEvalError> {
        let store = empty_store(schema);
        matches_predicate(item, predicate, schema, &store)
    }

    /// Build a test schema with common field types.
    fn test_schema() -> Schema {
        let mut fields = IndexMap::new();
        fields.insert(
            "title".to_owned(),
            FieldDefinition::new(FieldTypeConfig::String { pattern: None }),
        );
        let mut status = FieldDefinition::new(FieldTypeConfig::Choice {
            values: vec!["open".into(), "in_progress".into(), "done".into()],
        });
        status.required = true;
        fields.insert("status".to_owned(), status);
        fields.insert(
            "points".to_owned(),
            FieldDefinition::new(FieldTypeConfig::Integer {
                min: None,
                max: None,
            }),
        );
        fields.insert(
            "weight".to_owned(),
            FieldDefinition::new(FieldTypeConfig::Float {
                min: None,
                max: None,
            }),
        );
        fields.insert(
            "active".to_owned(),
            FieldDefinition::new(FieldTypeConfig::Boolean),
        );
        fields.insert(
            "tags".to_owned(),
            FieldDefinition::new(FieldTypeConfig::List),
        );
        fields.insert(
            "labels".to_owned(),
            FieldDefinition::new(FieldTypeConfig::Multichoice {
                values: vec!["backend".into(), "frontend".into(), "devops".into()],
            }),
        );
        fields.insert(
            "parent".to_owned(),
            FieldDefinition::new(FieldTypeConfig::Link {
                allow_cycles: Some(false),
                inverse: Some("children".into()),
            }),
        );
        fields.insert(
            "depends_on".to_owned(),
            FieldDefinition::new(FieldTypeConfig::Links {
                allow_cycles: Some(false),
                inverse: Some("dependents".into()),
            }),
        );
        fields.insert(
            "due_date".to_owned(),
            FieldDefinition::new(FieldTypeConfig::Date),
        );
        fields.insert(
            "estimate".to_owned(),
            FieldDefinition::new(FieldTypeConfig::Duration {
                min: None,
                max: None,
            }),
        );

        let inverse_table = Schema::build_inverse_table(&fields);
        Schema {
            fields,
            rules: vec![],
            inverse_table,
        }
    }

    /// Build a work item with the given fields.
    fn make_item(id: &str, fields: Vec<(&str, FieldValue)>) -> WorkItem {
        WorkItem {
            id: WorkItemId::from(id.to_owned()),
            fields: fields
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
            body: String::new(),
            source_path: PathBuf::from(format!("{id}.md")),
        }
    }

    fn comparison(field: &str, operator: Operator, value: &str) -> Predicate {
        Predicate::Comparison(Comparison {
            field: FieldReference::Local(field.to_owned()),
            operator,
            value: value.to_owned(),
        })
    }

    // ── String / Choice equality ────────────────────────────────

    #[test]
    fn string_equal_match() {
        let schema = test_schema();
        let item = make_item("t1", vec![("status", FieldValue::Choice("open".into()))]);
        let predicate = comparison("status", Operator::Equal, "open");
        assert!(check(&item, &predicate, &schema).unwrap());
    }

    #[test]
    fn string_equal_no_match() {
        let schema = test_schema();
        let item = make_item("t1", vec![("status", FieldValue::Choice("done".into()))]);
        let predicate = comparison("status", Operator::Equal, "open");
        assert!(!check(&item, &predicate, &schema).unwrap());
    }

    #[test]
    fn string_not_equal() {
        let schema = test_schema();
        let item = make_item("t1", vec![("status", FieldValue::Choice("open".into()))]);
        let predicate = comparison("status", Operator::NotEqual, "done");
        assert!(check(&item, &predicate, &schema).unwrap());
    }

    // ── Integer comparison ──────────────────────────────────────

    #[test]
    fn integer_greater_than_match() {
        let schema = test_schema();
        let item = make_item("t1", vec![("points", FieldValue::Integer(5))]);
        let predicate = comparison("points", Operator::GreaterThan, "3");
        assert!(check(&item, &predicate, &schema).unwrap());
    }

    #[test]
    fn integer_greater_than_no_match() {
        let schema = test_schema();
        let item = make_item("t1", vec![("points", FieldValue::Integer(2))]);
        let predicate = comparison("points", Operator::GreaterThan, "3");
        assert!(!check(&item, &predicate, &schema).unwrap());
    }

    #[test]
    fn integer_equal() {
        let schema = test_schema();
        let item = make_item("t1", vec![("points", FieldValue::Integer(5))]);
        let predicate = comparison("points", Operator::Equal, "5");
        assert!(check(&item, &predicate, &schema).unwrap());
    }

    #[test]
    fn integer_less_or_equal() {
        let schema = test_schema();
        let item = make_item("t1", vec![("points", FieldValue::Integer(3))]);
        let predicate = comparison("points", Operator::LessOrEqual, "3");
        assert!(check(&item, &predicate, &schema).unwrap());
    }

    // ── Float comparison ────────────────────────────────────────

    #[test]
    fn float_greater_or_equal() {
        let schema = test_schema();
        let item = make_item("t1", vec![("weight", FieldValue::Float(1.5))]);
        let predicate = comparison("weight", Operator::GreaterOrEqual, "1.5");
        assert!(check(&item, &predicate, &schema).unwrap());
    }

    // ── Boolean comparison ──────────────────────────────────────

    #[test]
    fn boolean_equal_true() {
        let schema = test_schema();
        let item = make_item("t1", vec![("active", FieldValue::Boolean(true))]);
        let predicate = comparison("active", Operator::Equal, "true");
        assert!(check(&item, &predicate, &schema).unwrap());
    }

    #[test]
    fn boolean_equal_false() {
        let schema = test_schema();
        let item = make_item("t1", vec![("active", FieldValue::Boolean(true))]);
        let predicate = comparison("active", Operator::Equal, "false");
        assert!(!check(&item, &predicate, &schema).unwrap());
    }

    // ── Contains ────────────────────────────────────────────────

    #[test]
    fn string_contains() {
        let schema = test_schema();
        let item = make_item(
            "t1",
            vec![("title", FieldValue::String("Fix login bug".into()))],
        );
        let predicate = comparison("title", Operator::Contains, "login");
        assert!(check(&item, &predicate, &schema).unwrap());
    }

    #[test]
    fn list_contains_membership() {
        let schema = test_schema();
        let item = make_item(
            "t1",
            vec![(
                "tags",
                FieldValue::List(vec!["auth".into(), "backend".into()]),
            )],
        );
        let predicate = comparison("tags", Operator::Equal, "auth");
        assert!(check(&item, &predicate, &schema).unwrap());
    }

    #[test]
    fn multichoice_membership() {
        let schema = test_schema();
        let item = make_item(
            "t1",
            vec![(
                "labels",
                FieldValue::Multichoice(vec!["backend".into(), "frontend".into()]),
            )],
        );
        let predicate = comparison("labels", Operator::Equal, "backend");
        assert!(check(&item, &predicate, &schema).unwrap());
    }

    // ── Regex ───────────────────────────────────────────────────

    #[test]
    fn regex_match() {
        let schema = test_schema();
        let item = make_item(
            "t1",
            vec![("title", FieldValue::String("Fix-login-bug".into()))],
        );
        let predicate = comparison("title", Operator::Matches, "/^Fix-.*/");
        assert!(check(&item, &predicate, &schema).unwrap());
    }

    #[test]
    fn regex_case_insensitive() {
        let schema = test_schema();
        let item = make_item(
            "t1",
            vec![("title", FieldValue::String("fix-login-bug".into()))],
        );
        let predicate = comparison("title", Operator::Matches, "/^Fix-.*/i");
        assert!(check(&item, &predicate, &schema).unwrap());
    }

    // ── IsSet / IsNotSet ────────────────────────────────────────

    #[test]
    fn is_set_with_value() {
        let schema = test_schema();
        let item = make_item(
            "t1",
            vec![("title", FieldValue::String("Something".into()))],
        );
        let predicate = comparison("title", Operator::IsSet, "");
        assert!(check(&item, &predicate, &schema).unwrap());
    }

    #[test]
    fn is_set_without_value() {
        let schema = test_schema();
        let item = make_item("t1", vec![]);
        let predicate = comparison("title", Operator::IsSet, "");
        assert!(!check(&item, &predicate, &schema).unwrap());
    }

    #[test]
    fn is_not_set() {
        let schema = test_schema();
        let item = make_item("t1", vec![]);
        let predicate = comparison("title", Operator::IsNotSet, "");
        assert!(check(&item, &predicate, &schema).unwrap());
    }

    // ── Missing field ───────────────────────────────────────────

    #[test]
    fn missing_field_no_match() {
        let schema = test_schema();
        let item = make_item("t1", vec![]);
        let predicate = comparison("points", Operator::GreaterThan, "3");
        assert!(!check(&item, &predicate, &schema).unwrap());
    }

    // ── And / Or / Not composition ──────────────────────────────

    #[test]
    fn and_both_match() {
        let schema = test_schema();
        let item = make_item(
            "t1",
            vec![
                ("status", FieldValue::Choice("open".into())),
                ("points", FieldValue::Integer(5)),
            ],
        );
        let predicate = Predicate::And(vec![
            comparison("status", Operator::Equal, "open"),
            comparison("points", Operator::GreaterThan, "3"),
        ]);
        assert!(check(&item, &predicate, &schema).unwrap());
    }

    #[test]
    fn and_one_fails() {
        let schema = test_schema();
        let item = make_item(
            "t1",
            vec![
                ("status", FieldValue::Choice("open".into())),
                ("points", FieldValue::Integer(2)),
            ],
        );
        let predicate = Predicate::And(vec![
            comparison("status", Operator::Equal, "open"),
            comparison("points", Operator::GreaterThan, "3"),
        ]);
        assert!(!check(&item, &predicate, &schema).unwrap());
    }

    #[test]
    fn or_one_matches() {
        let schema = test_schema();
        let item = make_item("t1", vec![("status", FieldValue::Choice("open".into()))]);
        let predicate = Predicate::Or(vec![
            comparison("status", Operator::Equal, "open"),
            comparison("status", Operator::Equal, "done"),
        ]);
        assert!(check(&item, &predicate, &schema).unwrap());
    }

    #[test]
    fn not_negates() {
        let schema = test_schema();
        let item = make_item("t1", vec![("status", FieldValue::Choice("open".into()))]);
        let predicate = Predicate::Not(Box::new(comparison("status", Operator::Equal, "done")));
        assert!(check(&item, &predicate, &schema).unwrap());
    }

    // ── Duration comparison (numeric on canonical seconds) ──────

    #[test]
    fn duration_greater_than_match() {
        let schema = test_schema();
        // 5d > 1h
        let item = make_item("t1", vec![("estimate", FieldValue::Duration(432_000))]);
        let predicate = comparison("estimate", Operator::GreaterThan, "1h");
        assert!(check(&item, &predicate, &schema).unwrap());
    }

    #[test]
    fn duration_greater_than_no_match() {
        let schema = test_schema();
        // 30min < 1h
        let item = make_item("t1", vec![("estimate", FieldValue::Duration(1_800))]);
        let predicate = comparison("estimate", Operator::GreaterThan, "1h");
        assert!(!check(&item, &predicate, &schema).unwrap());
    }

    #[test]
    fn duration_compound_rhs_parses() {
        let schema = test_schema();
        // estimate = 1w 2d (= 9 days = 777_600s); compare > "1w 1d" (= 8 days)
        let item = make_item("t1", vec![("estimate", FieldValue::Duration(777_600))]);
        let predicate = comparison("estimate", Operator::GreaterThan, "1w 1d");
        assert!(check(&item, &predicate, &schema).unwrap());
    }

    #[test]
    fn duration_negative_works() {
        let schema = test_schema();
        // -2d < 0s
        let item = make_item("t1", vec![("estimate", FieldValue::Duration(-172_800))]);
        let predicate = comparison("estimate", Operator::LessThan, "0s");
        assert!(check(&item, &predicate, &schema).unwrap());
    }

    #[test]
    fn duration_invalid_rhs_returns_false() {
        let schema = test_schema();
        let item = make_item("t1", vec![("estimate", FieldValue::Duration(432_000))]);
        let predicate = comparison("estimate", Operator::GreaterThan, "garbage");
        assert!(!check(&item, &predicate, &schema).unwrap());
    }

    // ── Date comparison (lexicographic) ─────────────────────────

    #[test]
    fn date_greater_than() {
        let schema = test_schema();
        let item = make_item(
            "t1",
            vec![(
                "due_date",
                FieldValue::Date(chrono::NaiveDate::from_ymd_opt(2026, 3, 15).unwrap()),
            )],
        );
        let predicate = comparison("due_date", Operator::GreaterThan, "2026-03-01");
        assert!(check(&item, &predicate, &schema).unwrap());
    }

    // ── Link comparison ─────────────────────────────────────────

    #[test]
    fn link_equal() {
        let schema = test_schema();
        let item = make_item(
            "t1",
            vec![(
                "parent",
                FieldValue::Link(WorkItemId::from("epic-1".to_owned())),
            )],
        );
        let predicate = comparison("parent", Operator::Equal, "epic-1");
        assert!(check(&item, &predicate, &schema).unwrap());
    }

    // ── Links membership ────────────────────────────────────────

    #[test]
    fn links_membership() {
        let schema = test_schema();
        let item = make_item(
            "t1",
            vec![(
                "depends_on",
                FieldValue::Links(vec![
                    WorkItemId::from("task-a".to_owned()),
                    WorkItemId::from("task-b".to_owned()),
                ]),
            )],
        );
        let predicate = comparison("depends_on", Operator::Equal, "task-a");
        assert!(check(&item, &predicate, &schema).unwrap());
    }

    // ── Cross-item (related-field) predicates ───────────────────

    /// Load a store from a set of in-memory markdown files.
    fn store_from_files(schema: &Schema, files: Vec<(&str, &str)>) -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        for (name, content) in files {
            std::fs::write(dir.path().join(name), content).unwrap();
        }
        let store = Store::load(dir.path(), schema).unwrap();
        (dir, store)
    }

    fn related_comparison(
        relation: &str,
        field: &str,
        operator: Operator,
        value: &str,
    ) -> Predicate {
        Predicate::Comparison(Comparison {
            field: FieldReference::Related {
                relation: relation.to_owned(),
                field: field.to_owned(),
            },
            operator,
            value: value.to_owned(),
        })
    }

    #[test]
    fn related_forward_link_match() {
        let schema = test_schema();
        let (_dir, store) = store_from_files(
            &schema,
            vec![
                ("epic.md", "---\nstatus: open\n---\n"),
                ("task-a.md", "---\nstatus: done\nparent: epic\n---\n"),
            ],
        );
        let item = store.get("task-a").unwrap();
        let predicate = related_comparison("parent", "status", Operator::Equal, "open");
        assert!(matches_predicate(item, &predicate, &schema, &store).unwrap());
    }

    #[test]
    fn related_forward_link_no_match() {
        let schema = test_schema();
        let (_dir, store) = store_from_files(
            &schema,
            vec![
                ("epic.md", "---\nstatus: done\n---\n"),
                ("task-a.md", "---\nstatus: done\nparent: epic\n---\n"),
            ],
        );
        let item = store.get("task-a").unwrap();
        let predicate = related_comparison("parent", "status", Operator::Equal, "open");
        assert!(!matches_predicate(item, &predicate, &schema, &store).unwrap());
    }

    #[test]
    fn related_forward_link_missing_target() {
        // task has parent: missing but target doesn't exist in store.
        let schema = test_schema();
        let (_dir, store) = store_from_files(
            &schema,
            vec![("task-a.md", "---\nstatus: done\nparent: missing\n---\n")],
        );
        let item = store.get("task-a").unwrap();
        let predicate = related_comparison("parent", "status", Operator::Equal, "open");
        assert!(!matches_predicate(item, &predicate, &schema, &store).unwrap());
    }

    #[test]
    fn related_forward_link_unset_relation() {
        // Task with no parent at all.
        let schema = test_schema();
        let (_dir, store) =
            store_from_files(&schema, vec![("task-a.md", "---\nstatus: open\n---\n")]);
        let item = store.get("task-a").unwrap();
        let predicate = related_comparison("parent", "status", Operator::Equal, "open");
        assert!(!matches_predicate(item, &predicate, &schema, &store).unwrap());
    }

    #[test]
    fn related_forward_links_any_matches() {
        // depends_on (links) — "any" semantics: true if any dep has open status.
        let schema = test_schema();
        let (_dir, store) = store_from_files(
            &schema,
            vec![
                ("dep-a.md", "---\nstatus: done\n---\n"),
                ("dep-b.md", "---\nstatus: open\n---\n"),
                (
                    "task.md",
                    "---\nstatus: open\ndepends_on: [dep-a, dep-b]\n---\n",
                ),
            ],
        );
        let item = store.get("task").unwrap();
        let predicate = related_comparison("depends_on", "status", Operator::Equal, "open");
        assert!(matches_predicate(item, &predicate, &schema, &store).unwrap());
    }

    #[test]
    fn related_forward_links_none_match() {
        let schema = test_schema();
        let (_dir, store) = store_from_files(
            &schema,
            vec![
                ("dep-a.md", "---\nstatus: done\n---\n"),
                ("dep-b.md", "---\nstatus: done\n---\n"),
                (
                    "task.md",
                    "---\nstatus: open\ndepends_on: [dep-a, dep-b]\n---\n",
                ),
            ],
        );
        let item = store.get("task").unwrap();
        let predicate = related_comparison("depends_on", "status", Operator::Equal, "open");
        assert!(!matches_predicate(item, &predicate, &schema, &store).unwrap());
    }

    #[test]
    fn related_inverse_match() {
        // children.status — inverse of parent.
        let schema = test_schema();
        let (_dir, store) = store_from_files(
            &schema,
            vec![
                ("epic.md", "---\nstatus: open\n---\n"),
                ("child-a.md", "---\nstatus: done\nparent: epic\n---\n"),
                ("child-b.md", "---\nstatus: open\nparent: epic\n---\n"),
            ],
        );
        let item = store.get("epic").unwrap();
        let predicate = related_comparison("children", "status", Operator::Equal, "open");
        assert!(matches_predicate(item, &predicate, &schema, &store).unwrap());
    }

    #[test]
    fn related_inverse_no_children() {
        let schema = test_schema();
        let (_dir, store) =
            store_from_files(&schema, vec![("leaf.md", "---\nstatus: open\n---\n")]);
        let item = store.get("leaf").unwrap();
        let predicate = related_comparison("children", "status", Operator::Equal, "open");
        assert!(!matches_predicate(item, &predicate, &schema, &store).unwrap());
    }

    #[test]
    fn related_is_set_on_related_field() {
        let schema = test_schema();
        let (_dir, store) = store_from_files(
            &schema,
            vec![
                ("epic.md", "---\nstatus: open\n---\n"),
                ("task-a.md", "---\nstatus: done\nparent: epic\n---\n"),
            ],
        );
        let item = store.get("task-a").unwrap();
        let predicate = related_comparison("parent", "status", Operator::IsSet, "");
        assert!(matches_predicate(item, &predicate, &schema, &store).unwrap());
    }

    #[test]
    fn related_is_set_unset_relation() {
        // No parent link → is_set should be false.
        let schema = test_schema();
        let (_dir, store) =
            store_from_files(&schema, vec![("task-a.md", "---\nstatus: open\n---\n")]);
        let item = store.get("task-a").unwrap();
        let predicate = related_comparison("parent", "status", Operator::IsSet, "");
        assert!(!matches_predicate(item, &predicate, &schema, &store).unwrap());
    }

    #[test]
    fn related_contains_on_traversal() {
        let schema = test_schema();
        let (_dir, store) = store_from_files(
            &schema,
            vec![
                ("epic.md", "---\ntitle: Fix login bug\nstatus: open\n---\n"),
                ("task-a.md", "---\nstatus: done\nparent: epic\n---\n"),
            ],
        );
        let item = store.get("task-a").unwrap();
        let predicate = related_comparison("parent", "title", Operator::Contains, "login");
        assert!(matches_predicate(item, &predicate, &schema, &store).unwrap());
    }

    #[test]
    fn related_combined_with_and() {
        let schema = test_schema();
        let (_dir, store) = store_from_files(
            &schema,
            vec![
                ("epic.md", "---\nstatus: open\n---\n"),
                ("task-a.md", "---\nstatus: done\nparent: epic\n---\n"),
            ],
        );
        let item = store.get("task-a").unwrap();
        let predicate = Predicate::And(vec![
            comparison("status", Operator::Equal, "done"),
            related_comparison("parent", "status", Operator::Equal, "open"),
        ]);
        assert!(matches_predicate(item, &predicate, &schema, &store).unwrap());
    }

    #[test]
    fn related_not_a_relation_errors() {
        // `title` is a string field — cannot traverse.
        let schema = test_schema();
        let (_dir, store) = store_from_files(
            &schema,
            vec![("task-a.md", "---\ntitle: A\nstatus: open\n---\n")],
        );
        let item = store.get("task-a").unwrap();
        let predicate = related_comparison("title", "whatever", Operator::Equal, "x");
        let result = matches_predicate(item, &predicate, &schema, &store);
        assert!(matches!(result, Err(QueryEvalError::NotARelation { .. })));
    }

    #[test]
    fn related_unknown_relation_errors() {
        let schema = test_schema();
        let (_dir, store) =
            store_from_files(&schema, vec![("task-a.md", "---\nstatus: open\n---\n")]);
        let item = store.get("task-a").unwrap();
        let predicate = related_comparison("nonexistent", "status", Operator::Equal, "x");
        let result = matches_predicate(item, &predicate, &schema, &store);
        assert!(matches!(
            result,
            Err(QueryEvalError::UnknownRelation { .. })
        ));
    }
}
