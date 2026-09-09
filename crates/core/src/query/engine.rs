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
