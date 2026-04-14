//! Type-aware sorting for query results.
//!
//! Sorts a slice of work item references by one or more fields,
//! using the schema to determine the comparison strategy per field type.
//! Missing values always sort last, regardless of direction.

use std::cmp::Ordering;

use crate::model::schema::{FieldType, Schema};
use crate::model::{FieldValue, WorkItem};
use crate::query::types::{SortDirection, SortSpec};

// ── Public API ──────────────────────────────────────────────────────

/// Sort work items in-place by the given sort specifications.
///
/// Applies specs left-to-right: the first spec is the primary sort key,
/// the second breaks ties from the first, and so on. When all specs
/// produce equal orderings, items are ordered by ID for determinism.
pub fn sort_items(items: &mut [&WorkItem], specs: &[SortSpec], schema: &Schema) {
    if specs.is_empty() {
        return;
    }

    items.sort_by(|item_a, item_b| {
        for spec in specs {
            let field_type = schema
                .fields
                .get(&spec.field)
                .map(|definition| definition.field_type());

            let value_a = item_a.fields.get(&spec.field);
            let value_b = item_b.fields.get(&spec.field);

            let ordering = compare_field_values(value_a, value_b, field_type);

            // Reverse for descending, but missing-last is already handled
            // inside compare_field_values and should not be reversed.
            let ordering = match spec.direction {
                SortDirection::Ascending => ordering,
                SortDirection::Descending => match (value_a, value_b) {
                    // Both present: reverse the ordering.
                    (Some(_), Some(_)) => ordering.reverse(),
                    // Missing values stay last regardless of direction.
                    _ => ordering,
                },
            };

            if ordering != Ordering::Equal {
                return ordering;
            }
        }

        // Tie-breaker: order by ID for determinism.
        item_a.id.as_str().cmp(item_b.id.as_str())
    });
}

// ── Field value comparison ──────────────────────────────────────────

/// Compare two field values, using the field type for type-aware ordering.
///
/// Missing values sort last: `(None, Some(_))` → `Greater`,
/// `(Some(_), None)` → `Less`.
fn compare_field_values(
    value_a: Option<&FieldValue>,
    value_b: Option<&FieldValue>,
    field_type: Option<FieldType>,
) -> Ordering {
    match (value_a, value_b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater, // missing sorts last
        (Some(_), None) => Ordering::Less,    // missing sorts last
        (Some(a), Some(b)) => compare_present_values(a, b, field_type),
    }
}

/// Compare two present (non-missing) field values.
fn compare_present_values(
    value_a: &FieldValue,
    value_b: &FieldValue,
    field_type: Option<FieldType>,
) -> Ordering {
    match field_type {
        Some(FieldType::Integer) => compare_integers(value_a, value_b),
        Some(FieldType::Float) => compare_floats(value_a, value_b),
        Some(FieldType::Boolean) => compare_booleans(value_a, value_b),
        Some(FieldType::Multichoice) | Some(FieldType::List) => {
            compare_string_lists(value_a, value_b)
        }
        Some(FieldType::Links) => compare_id_lists(value_a, value_b),
        // String, Choice, Date, Link, unknown: lexicographic.
        _ => compare_as_strings(value_a, value_b),
    }
}

// ── Type-specific comparisons ───────────────────────────────────────

fn compare_integers(value_a: &FieldValue, value_b: &FieldValue) -> Ordering {
    match (value_a, value_b) {
        (FieldValue::Integer(a), FieldValue::Integer(b)) => a.cmp(b),
        _ => Ordering::Equal,
    }
}

fn compare_floats(value_a: &FieldValue, value_b: &FieldValue) -> Ordering {
    match (value_a, value_b) {
        (FieldValue::Float(a), FieldValue::Float(b)) => {
            a.partial_cmp(b).unwrap_or(Ordering::Equal)
        }
        _ => Ordering::Equal,
    }
}

fn compare_booleans(value_a: &FieldValue, value_b: &FieldValue) -> Ordering {
    match (value_a, value_b) {
        // false < true
        (FieldValue::Boolean(a), FieldValue::Boolean(b)) => a.cmp(b),
        _ => Ordering::Equal,
    }
}

fn compare_string_lists(value_a: &FieldValue, value_b: &FieldValue) -> Ordering {
    let list_a = match value_a {
        FieldValue::Multichoice(values) => values.as_slice(),
        FieldValue::List(values) => values.as_slice(),
        _ => return Ordering::Equal,
    };
    let list_b = match value_b {
        FieldValue::Multichoice(values) => values.as_slice(),
        FieldValue::List(values) => values.as_slice(),
        _ => return Ordering::Equal,
    };
    list_a.cmp(list_b)
}

fn compare_id_lists(value_a: &FieldValue, value_b: &FieldValue) -> Ordering {
    match (value_a, value_b) {
        (FieldValue::Links(a), FieldValue::Links(b)) => {
            let strings_a: Vec<&str> = a.iter().map(|id| id.as_str()).collect();
            let strings_b: Vec<&str> = b.iter().map(|id| id.as_str()).collect();
            strings_a.cmp(&strings_b)
        }
        _ => Ordering::Equal,
    }
}

/// Compare two values as strings (lexicographic).
fn compare_as_strings(value_a: &FieldValue, value_b: &FieldValue) -> Ordering {
    let string_a = extract_sort_string(value_a);
    let string_b = extract_sort_string(value_b);
    string_a.cmp(&string_b)
}

/// Extract a string for sorting purposes.
fn extract_sort_string(value: &FieldValue) -> String {
    match value {
        FieldValue::String(string) => string.clone(),
        FieldValue::Choice(string) => string.clone(),
        FieldValue::Date(string) => string.clone(),
        FieldValue::Link(id) => id.as_str().to_owned(),
        FieldValue::Integer(number) => number.to_string(),
        FieldValue::Float(number) => number.to_string(),
        FieldValue::Boolean(flag) => flag.to_string(),
        FieldValue::Multichoice(values) => values.join(", "),
        FieldValue::List(values) => values.join(", "),
        FieldValue::Links(ids) => ids.iter().map(|id| id.as_str()).collect::<Vec<_>>().join(", "),
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::{FieldDefinition, FieldTypeConfig};
    use crate::model::WorkItemId;
    use indexmap::IndexMap;
    use std::path::PathBuf;

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
            "active".to_owned(),
            FieldDefinition::new(FieldTypeConfig::Boolean),
        );
        let inverse_table = Schema::build_inverse_table(&fields);
        Schema {
            fields,
            rules: vec![],
            inverse_table,
        }
    }

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

    // ── Sort by string field ────────────────────────────────────

    #[test]
    fn sort_by_string_ascending() {
        let schema = test_schema();
        let item_b = make_item("b", vec![("title", FieldValue::String("Banana".into()))]);
        let item_a = make_item("a", vec![("title", FieldValue::String("Apple".into()))]);
        let item_c = make_item("c", vec![("title", FieldValue::String("Cherry".into()))]);

        let mut items: Vec<&WorkItem> = vec![&item_b, &item_a, &item_c];
        sort_items(
            &mut items,
            &[SortSpec {
                field: "title".to_owned(),
                direction: SortDirection::Ascending,
            }],
            &schema,
        );

        let ids: Vec<&str> = items.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    // ── Sort by integer field ───────────────────────────────────

    #[test]
    fn sort_by_integer_ascending() {
        let schema = test_schema();
        let item_a = make_item("a", vec![("points", FieldValue::Integer(5))]);
        let item_b = make_item("b", vec![("points", FieldValue::Integer(2))]);
        let item_c = make_item("c", vec![("points", FieldValue::Integer(8))]);

        let mut items: Vec<&WorkItem> = vec![&item_a, &item_b, &item_c];
        sort_items(
            &mut items,
            &[SortSpec {
                field: "points".to_owned(),
                direction: SortDirection::Ascending,
            }],
            &schema,
        );

        let ids: Vec<&str> = items.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(ids, vec!["b", "a", "c"]);
    }

    #[test]
    fn sort_by_integer_descending() {
        let schema = test_schema();
        let item_a = make_item("a", vec![("points", FieldValue::Integer(5))]);
        let item_b = make_item("b", vec![("points", FieldValue::Integer(2))]);
        let item_c = make_item("c", vec![("points", FieldValue::Integer(8))]);

        let mut items: Vec<&WorkItem> = vec![&item_a, &item_b, &item_c];
        sort_items(
            &mut items,
            &[SortSpec {
                field: "points".to_owned(),
                direction: SortDirection::Descending,
            }],
            &schema,
        );

        let ids: Vec<&str> = items.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(ids, vec!["c", "a", "b"]);
    }

    // ── Missing values sort last ────────────────────────────────

    #[test]
    fn missing_values_sort_last_ascending() {
        let schema = test_schema();
        let item_a = make_item("a", vec![("points", FieldValue::Integer(5))]);
        let item_b = make_item("b", vec![]); // no points
        let item_c = make_item("c", vec![("points", FieldValue::Integer(2))]);

        let mut items: Vec<&WorkItem> = vec![&item_a, &item_b, &item_c];
        sort_items(
            &mut items,
            &[SortSpec {
                field: "points".to_owned(),
                direction: SortDirection::Ascending,
            }],
            &schema,
        );

        let ids: Vec<&str> = items.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(ids, vec!["c", "a", "b"]); // b (missing) is last
    }

    #[test]
    fn missing_values_sort_last_descending() {
        let schema = test_schema();
        let item_a = make_item("a", vec![("points", FieldValue::Integer(5))]);
        let item_b = make_item("b", vec![]); // no points
        let item_c = make_item("c", vec![("points", FieldValue::Integer(2))]);

        let mut items: Vec<&WorkItem> = vec![&item_a, &item_b, &item_c];
        sort_items(
            &mut items,
            &[SortSpec {
                field: "points".to_owned(),
                direction: SortDirection::Descending,
            }],
            &schema,
        );

        let ids: Vec<&str> = items.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "c", "b"]); // b (missing) still last
    }

    // ── Multi-field sort ────────────────────────────────────────

    #[test]
    fn multi_field_sort() {
        let schema = test_schema();
        let item_a = make_item(
            "a",
            vec![
                ("status", FieldValue::Choice("open".into())),
                ("points", FieldValue::Integer(5)),
            ],
        );
        let item_b = make_item(
            "b",
            vec![
                ("status", FieldValue::Choice("open".into())),
                ("points", FieldValue::Integer(2)),
            ],
        );
        let item_c = make_item(
            "c",
            vec![
                ("status", FieldValue::Choice("done".into())),
                ("points", FieldValue::Integer(8)),
            ],
        );

        let mut items: Vec<&WorkItem> = vec![&item_a, &item_b, &item_c];
        sort_items(
            &mut items,
            &[
                SortSpec {
                    field: "status".to_owned(),
                    direction: SortDirection::Ascending,
                },
                SortSpec {
                    field: "points".to_owned(),
                    direction: SortDirection::Ascending,
                },
            ],
            &schema,
        );

        let ids: Vec<&str> = items.iter().map(|item| item.id.as_str()).collect();
        // done < open (lexicographic), then by points within same status
        assert_eq!(ids, vec!["c", "b", "a"]);
    }

    // ── Boolean sort ────────────────────────────────────────────

    #[test]
    fn sort_by_boolean() {
        let schema = test_schema();
        let item_a = make_item("a", vec![("active", FieldValue::Boolean(true))]);
        let item_b = make_item("b", vec![("active", FieldValue::Boolean(false))]);

        let mut items: Vec<&WorkItem> = vec![&item_a, &item_b];
        sort_items(
            &mut items,
            &[SortSpec {
                field: "active".to_owned(),
                direction: SortDirection::Ascending,
            }],
            &schema,
        );

        let ids: Vec<&str> = items.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(ids, vec!["b", "a"]); // false < true
    }

    // ── Deterministic tie-breaker ───────────────────────────────

    #[test]
    fn equal_values_ordered_by_id() {
        let schema = test_schema();
        let item_c = make_item("c", vec![("points", FieldValue::Integer(5))]);
        let item_a = make_item("a", vec![("points", FieldValue::Integer(5))]);
        let item_b = make_item("b", vec![("points", FieldValue::Integer(5))]);

        let mut items: Vec<&WorkItem> = vec![&item_c, &item_a, &item_b];
        sort_items(
            &mut items,
            &[SortSpec {
                field: "points".to_owned(),
                direction: SortDirection::Ascending,
            }],
            &schema,
        );

        let ids: Vec<&str> = items.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }
}
