//! Predicate evaluation: check whether a work item matches a predicate.
//!
//! The evaluator is type-aware — it uses the schema to determine how to
//! compare field values (numeric for integers, lexicographic for strings, etc.).

use crate::model::duration::parse_duration;
use crate::model::field_value::format_field_value;
use crate::model::schema::{FieldType, Schema};
use crate::model::{FieldValue, WorkItem};
use crate::query::types::{Comparison, FieldReference, Operator, Predicate};
use crate::resolve::{resolve_field_ref, ResolvedValues};
use crate::store::Store;

// ── Error ───────────────────────────────────────────────────────────

/// Errors produced during predicate evaluation.
#[derive(Debug, thiserror::Error)]
pub enum QueryEvalError {
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
            Ok(eval_single(field_value, field_type, comparison))
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
                    Ok(eval_single(field_value, field_type, comparison))
                }
                ResolvedValues::Many(values) => {
                    // "Any" semantics: matches if at least one resolved
                    // value satisfies the predicate.
                    Ok(values
                        .iter()
                        .any(|value| eval_single(*value, field_type, comparison)))
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
) -> bool {
    // IsSet / IsNotSet don't need a value.
    match comparison.operator {
        Operator::IsSet => return field_value.is_some(),
        Operator::IsNotSet => return field_value.is_none(),
        _ => {}
    }

    // A field with no value satisfies the negative comparisons and fails
    // every positive one. `status != done` and `status not in done,removed`
    // therefore both admit an item carrying no status, which is what makes
    // the two ways of writing a negation agree — see `Operator::is_negative`.
    //
    // The stricter reading (an absent field matches nothing either way) stays
    // available by adding the presence check as a second clause:
    // `status != removed` + `status?`.
    let field_value = match field_value {
        Some(value) => value,
        None => return comparison.operator.is_negative(),
    };

    match field_type {
        Some(FieldType::Integer) => eval_integer(field_value, comparison),
        Some(FieldType::Float) => eval_float(field_value, comparison),
        Some(FieldType::Boolean) => eval_boolean(field_value, comparison),
        Some(FieldType::Duration) => eval_duration(field_value, comparison),
        Some(FieldType::Color) => eval_color(field_value, comparison),
        Some(FieldType::Multichoice) | Some(FieldType::List) => eval_list(field_value, comparison),
        Some(FieldType::Links) => eval_links(field_value, comparison),
        // String, Choice, Date, Link, and unknown fields all use string comparison.
        _ => eval_string(field_value, comparison),
    }
}

// ── Type-specific evaluation ────────────────────────────────────────

/// The ordered-scalar operator table, shared by the integer, float, and
/// duration evaluators — the one place a comparison operator is mapped onto
/// `PartialOrd`. `None` on either side (a wrong-variant field value, an
/// unparseable right-hand side) matches nothing, whatever the operator.
fn eval_ordered<T: PartialOrd>(actual: Option<T>, expected: Option<T>, operator: Operator) -> bool {
    let (Some(actual), Some(expected)) = (actual, expected) else {
        return false;
    };
    match operator {
        Operator::Equal => actual == expected,
        Operator::NotEqual => actual != expected,
        Operator::GreaterThan => actual > expected,
        Operator::LessThan => actual < expected,
        Operator::GreaterOrEqual => actual >= expected,
        Operator::LessOrEqual => actual <= expected,
        Operator::Contains | Operator::Matches => false,
        Operator::IsSet | Operator::IsNotSet => unreachable!("handled above"),
        Operator::In | Operator::NotIn => {
            unreachable!("desugared into Or/And by query::parse")
        }
    }
}

/// String-like comparison: String, Choice, Date, Link, and unknown fields.
/// The field value is coerced to text via [`format_field_value`], so a
/// filter compares exactly the text a view displays.
fn eval_string(field_value: &FieldValue, comparison: &Comparison) -> bool {
    let actual = format_field_value(field_value);
    let expected = comparison.operand.text();

    match comparison.operator {
        Operator::Contains => actual.contains(expected),
        Operator::Matches => eval_regex(comparison, &actual),
        operator => eval_ordered(Some(actual.as_str()), Some(expected), operator),
    }
}

/// Integer comparison.
fn eval_integer(field_value: &FieldValue, comparison: &Comparison) -> bool {
    let actual = match field_value {
        FieldValue::Integer(number) => Some(*number),
        _ => None,
    };
    let expected = comparison.operand.text().parse::<i64>().ok();
    eval_ordered(actual, expected, comparison.operator)
}

/// Float comparison.
fn eval_float(field_value: &FieldValue, comparison: &Comparison) -> bool {
    let actual = match field_value {
        FieldValue::Float(number) => Some(*number),
        _ => None,
    };
    let expected = comparison.operand.text().parse::<f64>().ok();
    eval_ordered(actual, expected, comparison.operator)
}

/// Duration comparison: canonical i64 seconds. The RHS string is parsed via
/// the same suffix-shorthand grammar used everywhere else (`5d`, `1w 2d`, …).
fn eval_duration(field_value: &FieldValue, comparison: &Comparison) -> bool {
    let actual = match field_value {
        FieldValue::Duration(seconds) => Some(*seconds),
        _ => None,
    };
    let expected = parse_duration(comparison.operand.text()).ok();
    eval_ordered(actual, expected, comparison.operator)
}

/// Color comparison. Both sides are resolved to hex before comparing,
/// so `color == red` matches an item that stores red's pinned hex and
/// vice versa. A RHS that isn't a valid color never matches, mirroring
/// how unparseable numbers behave for numeric fields.
fn eval_color(field_value: &FieldValue, comparison: &Comparison) -> bool {
    use crate::model::color::{parse_color, resolve_color_to_hex};

    let actual = match field_value {
        FieldValue::Color(canonical) => resolve_color_to_hex(canonical),
        _ => return false,
    };
    let Some(actual) = actual else {
        return false;
    };
    let expected = match parse_color(comparison.operand.text()) {
        Ok(canonical) => resolve_color_to_hex(&canonical),
        Err(_) => return false,
    };
    let Some(expected) = expected else {
        return false;
    };

    match comparison.operator {
        Operator::Equal => actual == expected,
        Operator::NotEqual => actual != expected,
        // Ordering/contains/regex don't make sense for colors.
        _ => false,
    }
}

/// Boolean comparison.
fn eval_boolean(field_value: &FieldValue, comparison: &Comparison) -> bool {
    let actual = match field_value {
        FieldValue::Boolean(flag) => *flag,
        _ => return false,
    };
    let expected = match comparison.operand.text() {
        "true" => true,
        "false" => false,
        _ => return false,
    };

    match comparison.operator {
        Operator::Equal => actual == expected,
        Operator::NotEqual => actual != expected,
        // Ordering/contains/regex don't make sense for booleans.
        _ => false,
    }
}

/// The collection operator table, shared by the list-like and links
/// evaluators. `Equal`/`NotEqual` test membership (any element equals the
/// value); `Contains`/`Matches` test each element; ordering never matches.
fn eval_collection(elements: &[&str], comparison: &Comparison) -> bool {
    let expected = comparison.operand.text();
    match comparison.operator {
        Operator::Equal => elements.contains(&expected),
        Operator::NotEqual => !elements.contains(&expected),
        Operator::Contains => elements.iter().any(|element| element.contains(expected)),
        Operator::Matches => elements
            .iter()
            .any(|element| eval_regex(comparison, element)),
        Operator::GreaterThan
        | Operator::LessThan
        | Operator::GreaterOrEqual
        | Operator::LessOrEqual => false,
        Operator::IsSet | Operator::IsNotSet => unreachable!("handled above"),
        Operator::In | Operator::NotIn => {
            unreachable!("desugared into Or/And by query::parse")
        }
    }
}

/// List-like comparison: Multichoice and List fields.
fn eval_list(field_value: &FieldValue, comparison: &Comparison) -> bool {
    let elements: Vec<&str> = match field_value {
        FieldValue::Multichoice(values) | FieldValue::List(values) => {
            values.iter().map(String::as_str).collect()
        }
        _ => return false,
    };
    eval_collection(&elements, comparison)
}

/// Links comparison: the collection table over the target ids.
fn eval_links(field_value: &FieldValue, comparison: &Comparison) -> bool {
    let elements: Vec<&str> = match field_value {
        FieldValue::Links(ids) => ids.iter().map(|id| id.as_str()).collect(),
        _ => return false,
    };
    eval_collection(&elements, comparison)
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Evaluate the comparison's regex operand against one haystack. A literal
/// operand under `Matches` — impossible via the parser, possible in a
/// hand-built predicate — matches nothing, mirroring how an unparseable
/// number behaves for numeric fields.
fn eval_regex(comparison: &Comparison, haystack: &str) -> bool {
    match comparison.operand.regex() {
        Some(regex) => regex.is_match(haystack),
        None => false,
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::{FieldDefinition, FieldTypeConfig};
    use crate::model::WorkItemId;
    use crate::query::parse::parse_where;
    use crate::query::types::{FieldReference, Operand, QueryRegex};
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
        fields.insert(
            "background".to_owned(),
            FieldDefinition::new(FieldTypeConfig::Color),
        );

        Schema::new(fields, vec![])
    }

    /// Build a work item with the given fields, including the `id`
    /// projection that `coerce_fields` adds to every loaded item.
    fn make_item(id: &str, fields: Vec<(&str, FieldValue)>) -> WorkItem {
        let mut map: std::collections::HashMap<String, FieldValue> = fields
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect();
        map.insert("id".to_owned(), FieldValue::String(id.to_owned()));
        WorkItem {
            id: WorkItemId::from(id.to_owned()),
            fields: map,
            body: String::new(),
            source_path: PathBuf::from(format!("{id}.md")),
        }
    }

    fn comparison(field: &str, operator: Operator, value: &str) -> Predicate {
        Predicate::Comparison(Comparison {
            field: FieldReference::Local(field.to_owned()),
            operator,
            operand: Operand::Value(value.to_owned()),
        })
    }

    /// A `Matches` clause carrying the compiled regex the parser would
    /// hand the evaluator, built from its parts — splitting the
    /// `/pattern/flags` clause form stays the parser's job alone.
    fn regex_comparison(field: &str, pattern: &str, flags: &str) -> Predicate {
        Predicate::Comparison(Comparison {
            field: FieldReference::Local(field.to_owned()),
            operator: Operator::Matches,
            operand: Operand::Regex(QueryRegex::new(pattern, flags).unwrap()),
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

    // ── Color comparison ────────────────────────────────────────

    #[test]
    fn color_name_matches_its_pinned_hex() {
        // The item stores red's pinned hex literally; filtering on the
        // name must match — both sides resolve before comparing.
        let schema = test_schema();
        let item = make_item(
            "t1",
            vec![("background", FieldValue::Color("#ef4444".into()))],
        );
        let predicate = comparison("background", Operator::Equal, "red");
        assert!(check(&item, &predicate, &schema).unwrap());
    }

    #[test]
    fn color_hex_matches_stored_name() {
        let schema = test_schema();
        let item = make_item("t1", vec![("background", FieldValue::Color("red".into()))]);
        let predicate = comparison("background", Operator::Equal, "#EF4444");
        assert!(check(&item, &predicate, &schema).unwrap());
    }

    #[test]
    fn color_not_equal_on_resolved_hex() {
        let schema = test_schema();
        let item = make_item("t1", vec![("background", FieldValue::Color("red".into()))]);
        let predicate = comparison("background", Operator::NotEqual, "blue");
        assert!(check(&item, &predicate, &schema).unwrap());
    }

    #[test]
    fn color_invalid_comparison_value_never_matches() {
        let schema = test_schema();
        let item = make_item("t1", vec![("background", FieldValue::Color("red".into()))]);
        let equal_predicate = comparison("background", Operator::Equal, "teal");
        assert!(!check(&item, &equal_predicate, &schema).unwrap());
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
        let predicate = regex_comparison("title", "^Fix-.*", "");
        assert!(check(&item, &predicate, &schema).unwrap());
    }

    #[test]
    fn regex_case_insensitive() {
        let schema = test_schema();
        let item = make_item(
            "t1",
            vec![("title", FieldValue::String("fix-login-bug".into()))],
        );
        let predicate = regex_comparison("title", "^Fix-.*", "i");
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

    /// An absent field fails every positive comparison.
    #[test]
    fn missing_field_fails_positive_operators() {
        let schema = test_schema();
        let item = make_item("t1", vec![]);
        for (field, operator, value) in [
            ("status", Operator::Equal, "done"),
            ("points", Operator::GreaterThan, "3"),
            ("points", Operator::LessOrEqual, "3"),
            ("title", Operator::Contains, "fix"),
            ("title", Operator::Matches, "/^fix/"),
            ("labels", Operator::Equal, "backend"),
        ] {
            let predicate = comparison(field, operator, value);
            assert!(
                !check(&item, &predicate, &schema).unwrap(),
                "{field} {operator:?} {value} should not match an absent field"
            );
        }
    }

    /// …and satisfies the negative ones, so an item that never set a status
    /// still counts as "not done".
    #[test]
    fn missing_field_satisfies_negative_operators() {
        let schema = test_schema();
        let item = make_item("t1", vec![]);
        for (field, value) in [("status", "done"), ("points", "3"), ("labels", "backend")] {
            let predicate = comparison(field, Operator::NotEqual, value);
            assert!(
                check(&item, &predicate, &schema).unwrap(),
                "{field} != {value} should match an absent field"
            );
        }
    }

    /// The contract `not in` relies on: it agrees with `!=` for every item,
    /// including one carrying no value for the field.
    #[test]
    fn not_in_agrees_with_not_equal_on_absent_field() {
        let schema = test_schema();
        let absent = make_item("t1", vec![]);
        let present = make_item("t2", vec![("status", FieldValue::String("done".into()))]);

        for item in [&absent, &present] {
            let single = parse_where("status!=done").unwrap();
            let membership = parse_where("status not in done").unwrap();
            assert_eq!(
                check(item, &single, &schema).unwrap(),
                check(item, &membership, &schema).unwrap(),
                "'status!=done' and 'status not in done' disagree on {}",
                item.id
            );
        }
    }

    /// The stricter reading — exclude items with no value — stays reachable by
    /// AND-ing the presence check, which is how a `where:` list combines.
    #[test]
    fn presence_check_restores_strict_exclusion() {
        let schema = test_schema();
        let absent = make_item("t1", vec![]);
        let predicate = Predicate::And(vec![
            parse_where("status!=done").unwrap(),
            parse_where("status?").unwrap(),
        ]);
        assert!(!check(&absent, &predicate, &schema).unwrap());
    }

    #[test]
    fn membership_matches_listed_values_only() {
        let schema = test_schema();
        let predicate = parse_where("status in open,in_progress").unwrap();
        for (value, expected) in [("open", true), ("in_progress", true), ("done", false)] {
            let item = make_item("t1", vec![("status", FieldValue::String(value.into()))]);
            assert_eq!(
                check(&item, &predicate, &schema).unwrap(),
                expected,
                "{value}"
            );
        }
    }

    #[test]
    fn negated_membership_excludes_listed_values_only() {
        let schema = test_schema();
        let predicate = parse_where("status not in done,open").unwrap();
        for (value, expected) in [("open", false), ("done", false), ("in_progress", true)] {
            let item = make_item("t1", vec![("status", FieldValue::String(value.into()))]);
            assert_eq!(
                check(&item, &predicate, &schema).unwrap(),
                expected,
                "{value}"
            );
        }
    }

    /// On a collection field, `equal` means membership, so `in` asks whether
    /// the collection holds any of the listed members.
    #[test]
    fn membership_on_a_collection_field() {
        let schema = test_schema();
        let item = make_item(
            "t1",
            vec![(
                "labels",
                FieldValue::Multichoice(vec!["backend".into(), "devops".into()]),
            )],
        );
        assert!(check(
            &item,
            &parse_where("labels in backend,frontend").unwrap(),
            &schema
        )
        .unwrap());
        assert!(!check(&item, &parse_where("labels in frontend").unwrap(), &schema).unwrap());
        assert!(!check(
            &item,
            &parse_where("labels not in devops,frontend").unwrap(),
            &schema
        )
        .unwrap());
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

    // ── Filtering on the id ─────────────────────────────────────

    /// The reported bug: `id=alpha` matched nothing at all.
    #[test]
    fn id_equality_matches() {
        let schema = test_schema();
        let item = make_item("alpha", vec![]);
        assert!(check(&item, &comparison("id", Operator::Equal, "alpha"), &schema).unwrap());
        assert!(!check(&item, &comparison("id", Operator::Equal, "beta"), &schema).unwrap());
    }

    /// The id is a string by construction, so the full string operator set
    /// applies — and it is never absent, so `id?` always holds.
    #[test]
    fn id_supports_string_operators() {
        let schema = test_schema();
        let item = make_item("auth-login", vec![]);
        for (operator, value, expected) in [
            (Operator::NotEqual, "other", true),
            (Operator::Contains, "login", true),
            (Operator::Contains, "logout", false),
            (Operator::IsSet, "", true),
            (Operator::IsNotSet, "", false),
        ] {
            assert_eq!(
                check(&item, &comparison("id", operator, value), &schema).unwrap(),
                expected,
                "id {operator:?} {value}"
            );
        }
        for (pattern, expected) in [("^auth-", true), ("^billing-", false)] {
            assert_eq!(
                check(&item, &regex_comparison("id", pattern, ""), &schema).unwrap(),
                expected,
                "id Matches /{pattern}/"
            );
        }
    }

    #[test]
    fn id_membership() {
        let schema = test_schema();
        let item = make_item("alpha", vec![]);
        assert!(check(&item, &parse_where("id in alpha,beta").unwrap(), &schema).unwrap());
        assert!(!check(&item, &parse_where("id in beta,gamma").unwrap(), &schema).unwrap());
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
            operand: Operand::Value(value.to_owned()),
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

    /// A regex on a related field parses and evaluates like every other
    /// operator on a related field — pinned end-to-end because a stale
    /// parser comment used to claim the combination was rejected.
    #[test]
    fn related_regex_matches_target_value() {
        let schema = test_schema();
        let (_dir, store) = store_from_files(
            &schema,
            vec![
                ("epic.md", "---\ntitle: Fix login flow\nstatus: open\n---\n"),
                ("task-a.md", "---\nstatus: done\nparent: epic\n---\n"),
            ],
        );
        let item = store.get("task-a").unwrap();
        let matching = parse_where("parent.title/^Fix/").unwrap();
        assert!(matches_predicate(item, &matching, &schema, &store).unwrap());
        let case_insensitive = parse_where("parent.title/^fix/i").unwrap();
        assert!(matches_predicate(item, &case_insensitive, &schema, &store).unwrap());
        let not_matching = parse_where("parent.title/^Add/").unwrap();
        assert!(!matches_predicate(item, &not_matching, &schema, &store).unwrap());
    }

    /// A regex reaching a collection-valued target field tests each element.
    #[test]
    fn related_regex_on_collection_target() {
        let schema = test_schema();
        let (_dir, store) = store_from_files(
            &schema,
            vec![
                (
                    "dep-a.md",
                    "---\nstatus: open\ntags: [auth, backend]\n---\n",
                ),
                ("task.md", "---\nstatus: open\ndepends_on: [dep-a]\n---\n"),
            ],
        );
        let item = store.get("task").unwrap();
        let matching = parse_where("depends_on.tags/^back/").unwrap();
        assert!(matches_predicate(item, &matching, &schema, &store).unwrap());
        let not_matching = parse_where("depends_on.tags/^front/").unwrap();
        assert!(!matches_predicate(item, &not_matching, &schema, &store).unwrap());
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

    /// `parent.id` resolves through the projection on the *target* item, so
    /// it works without the traversal knowing anything about ids. These run
    /// against a loaded store, exercising the real coercion path.
    #[test]
    fn related_forward_link_on_id() {
        let schema = test_schema();
        let (_dir, store) = store_from_files(
            &schema,
            vec![
                ("epic.md", "---\nstatus: open\n---\n"),
                ("task-a.md", "---\nstatus: done\nparent: epic\n---\n"),
            ],
        );
        let item = store.get("task-a").unwrap();
        assert!(matches_predicate(
            item,
            &related_comparison("parent", "id", Operator::Equal, "epic"),
            &schema,
            &store
        )
        .unwrap());
        assert!(!matches_predicate(
            item,
            &related_comparison("parent", "id", Operator::Equal, "other"),
            &schema,
            &store
        )
        .unwrap());
    }

    /// The inverse direction, which had no way to be expressed before:
    /// "items that have a child called `child-b`".
    #[test]
    fn related_inverse_on_id() {
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
        assert!(matches_predicate(
            item,
            &related_comparison("children", "id", Operator::Equal, "child-b"),
            &schema,
            &store
        )
        .unwrap());
        assert!(!matches_predicate(
            item,
            &related_comparison("children", "id", Operator::Equal, "child-z"),
            &schema,
            &store
        )
        .unwrap());
    }

    /// An item whose id comes from an explicit frontmatter key, rather than
    /// its filename, is filterable on exactly that id — the parser resolves
    /// both sources into the same place before the projection is built.
    #[test]
    fn id_from_frontmatter_key_is_filterable() {
        let schema = test_schema();
        let (_dir, store) = store_from_files(
            &schema,
            vec![("some-file.md", "---\nid: real-id\nstatus: open\n---\n")],
        );
        let item = store.get("real-id").unwrap();
        assert!(matches_predicate(
            item,
            &comparison("id", Operator::Equal, "real-id"),
            &schema,
            &store
        )
        .unwrap());
        assert!(!matches_predicate(
            item,
            &comparison("id", Operator::Equal, "some-file"),
            &schema,
            &store
        )
        .unwrap());
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
