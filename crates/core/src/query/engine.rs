//! Query engine facade: the main entry point for executing queries.
//!
//! Ties together filtering, sorting, column selection, and row
//! formatting into a single `execute` call. This is the API that
//! other commands (board, tree, graph) will use programmatically.

use crate::model::field_value::format_field_value;
use crate::model::schema::Schema;
use crate::model::WorkItem;
use crate::query::eval::{matches_predicate, QueryEvalError};
use crate::query::sort::sort_items;
use crate::query::types::{QueryRequest, QueryResult, QueryRow};
use crate::store::Store;

// ── Public API ──────────────────────────────────────────────────────

/// Execute a query against the store and return formatted results.
///
/// Runs [`filter_and_sort`] then formats each matched item's field values
/// into display strings for table/JSON output.
pub fn execute(
    request: &QueryRequest,
    store: &Store,
    schema: &Schema,
) -> Result<QueryResult, QueryEvalError> {
    let (columns, matched_items) = filter_and_sort(request, store, schema)?;
    let items = matched_items
        .iter()
        .map(|item| build_row(item, &columns))
        .collect();
    Ok(QueryResult { columns, items })
}

/// Run the filter, sort, and column-selection stages of a query.
///
/// Returns the chosen column names and the matched items in sorted order,
/// without formatting any field values. Callers that need raw typed values
/// (e.g. CSV/TSV export with a custom list separator) use this directly
/// so they can format differently than the default table/JSON path.
pub fn filter_and_sort<'a>(
    request: &QueryRequest,
    store: &'a Store,
    schema: &Schema,
) -> Result<(Vec<String>, Vec<&'a WorkItem>), QueryEvalError> {
    let mut matched_items: Vec<&'a WorkItem> = Vec::new();
    for item in store.all_items() {
        let matches = match &request.predicate {
            Some(predicate) => matches_predicate(item, predicate, schema, store)?,
            None => true,
        };
        if matches {
            matched_items.push(item);
        }
    }

    sort_items(&mut matched_items, &request.sort, schema);

    let columns = if request.fields.is_empty() {
        default_columns(schema)
    } else {
        request.fields.clone()
    };

    Ok((columns, matched_items))
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Build the default column list: `id` followed by all required fields
/// in schema definition order.
fn default_columns(schema: &Schema) -> Vec<String> {
    let mut columns = vec!["id".to_owned()];
    for (field_name, definition) in &schema.fields {
        if definition.required {
            columns.push(field_name.clone());
        }
    }
    columns
}

/// Build a result row for a single work item.
fn build_row(item: &WorkItem, columns: &[String]) -> QueryRow {
    let values = columns
        .iter()
        .map(|column| {
            if column == "id" {
                item.id.as_str().to_owned()
            } else {
                match item.fields.get(column) {
                    Some(value) => format_field_value(value),
                    None => String::new(),
                }
            }
        })
        .collect();

    QueryRow {
        id: item.id.as_str().to_owned(),
        values,
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::{FieldDefinition, FieldTypeConfig};
    use crate::model::{FieldValue, WorkItemId};
    use crate::query::types::{
        Comparison, FieldReference, Operator, Predicate, SortDirection, SortSpec,
    };
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
        let inverse_table = Schema::build_inverse_table(&fields);
        Schema {
            fields,
            rules: vec![],
            inverse_table,
        }
    }

    fn make_store(items: Vec<WorkItem>) -> Store {
        // Build store by loading from a temp directory with the test items.
        // For unit tests we use the insert method to avoid file I/O.
        let empty_schema = test_schema();
        let temp_dir = tempfile::tempdir().unwrap();
        let store_result = Store::load(temp_dir.path(), &empty_schema);
        let mut store = store_result.unwrap();
        for item in items {
            store.insert(item);
        }
        store
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

    #[test]
    fn execute_no_predicate_returns_all() {
        let schema = test_schema();
        let store = make_store(vec![
            make_item("a", vec![("status", FieldValue::Choice("open".into()))]),
            make_item("b", vec![("status", FieldValue::Choice("done".into()))]),
        ]);

        let request = QueryRequest {
            predicate: None,
            sort: vec![],
            fields: vec![],
        };
        let result = execute(&request, &store, &schema).unwrap();
        assert_eq!(result.items.len(), 2);
    }

    #[test]
    fn execute_with_predicate_filters() {
        let schema = test_schema();
        let store = make_store(vec![
            make_item("a", vec![("status", FieldValue::Choice("open".into()))]),
            make_item("b", vec![("status", FieldValue::Choice("done".into()))]),
        ]);

        let request = QueryRequest {
            predicate: Some(Predicate::Comparison(Comparison {
                field: FieldReference::Local("status".to_owned()),
                operator: Operator::Equal,
                value: "open".to_owned(),
            })),
            sort: vec![],
            fields: vec![],
        };
        let result = execute(&request, &store, &schema).unwrap();
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].id, "a");
    }

    #[test]
    fn execute_default_columns() {
        let schema = test_schema();
        let store = make_store(vec![make_item(
            "a",
            vec![("status", FieldValue::Choice("open".into()))],
        )]);

        let request = QueryRequest {
            predicate: None,
            sort: vec![],
            fields: vec![],
        };
        let result = execute(&request, &store, &schema).unwrap();
        // Default columns: id + required fields (status is required).
        assert_eq!(result.columns, vec!["id", "status"]);
    }

    #[test]
    fn execute_custom_columns() {
        let schema = test_schema();
        let store = make_store(vec![make_item(
            "a",
            vec![
                ("title", FieldValue::String("Hello".into())),
                ("status", FieldValue::Choice("open".into())),
                ("points", FieldValue::Integer(5)),
            ],
        )]);

        let request = QueryRequest {
            predicate: None,
            sort: vec![],
            fields: vec!["id".into(), "title".into(), "points".into()],
        };
        let result = execute(&request, &store, &schema).unwrap();
        assert_eq!(result.columns, vec!["id", "title", "points"]);
        assert_eq!(result.items[0].values, vec!["a", "Hello", "5"]);
    }

    #[test]
    fn execute_with_sort() {
        let schema = test_schema();
        let store = make_store(vec![
            make_item("a", vec![("points", FieldValue::Integer(5))]),
            make_item("b", vec![("points", FieldValue::Integer(2))]),
            make_item("c", vec![("points", FieldValue::Integer(8))]),
        ]);

        let request = QueryRequest {
            predicate: None,
            sort: vec![SortSpec {
                field: "points".to_owned(),
                direction: SortDirection::Ascending,
            }],
            fields: vec!["id".into(), "points".into()],
        };
        let result = execute(&request, &store, &schema).unwrap();
        let ids: Vec<&str> = result.items.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(ids, vec!["b", "a", "c"]);
    }

    #[test]
    fn execute_empty_store() {
        let schema = test_schema();
        let store = make_store(vec![]);

        let request = QueryRequest {
            predicate: None,
            sort: vec![],
            fields: vec![],
        };
        let result = execute(&request, &store, &schema).unwrap();
        assert!(result.items.is_empty());
    }
}
