//! Formatting helpers for query results.
//!
//! Provides value formatting and JSON output. Table rendering is handled
//! by the command layer using `cli::output::table()` to keep this module
//! free of CLI dependencies.

use crate::model::FieldValue;
use crate::query::types::QueryResult;

// ── Field value formatting ──────────────────────────────────────────

/// Format a field value as a human-readable display string.
pub fn format_field_value(value: &FieldValue) -> String {
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
        FieldValue::Links(ids) => ids
            .iter()
            .map(|id| id.as_str())
            .collect::<Vec<_>>()
            .join(", "),
    }
}

// ── JSON output ─────────────────────────────────────────────────────

/// Render a query result as a JSON string.
///
/// Produces a JSON array of objects, one per matched item. Each object
/// has a key for every column in the result.
pub fn render_json(result: &QueryResult) -> String {
    let items: Vec<serde_json::Value> = result
        .items
        .iter()
        .map(|row| {
            let mut object = serde_json::Map::new();
            for (index, column) in result.columns.iter().enumerate() {
                let value = row
                    .values
                    .get(index)
                    .cloned()
                    .unwrap_or_default();
                object.insert(column.clone(), serde_json::Value::String(value));
            }
            serde_json::Value::Object(object)
        })
        .collect();

    serde_json::to_string_pretty(&items).unwrap_or_else(|_| "[]".to_owned())
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::WorkItemId;
    use crate::query::types::{QueryResult, QueryRow};

    #[test]
    fn format_string_value() {
        assert_eq!(
            format_field_value(&FieldValue::String("hello".into())),
            "hello"
        );
    }

    #[test]
    fn format_integer_value() {
        assert_eq!(format_field_value(&FieldValue::Integer(42)), "42");
    }

    #[test]
    fn format_list_value() {
        assert_eq!(
            format_field_value(&FieldValue::List(vec!["a".into(), "b".into()])),
            "a, b"
        );
    }

    #[test]
    fn format_links_value() {
        assert_eq!(
            format_field_value(&FieldValue::Links(vec![
                WorkItemId::from("x".to_owned()),
                WorkItemId::from("y".to_owned()),
            ])),
            "x, y"
        );
    }

    #[test]
    fn render_json_produces_valid_json() {
        let result = QueryResult {
            columns: vec!["id".into(), "title".into()],
            items: vec![
                QueryRow {
                    id: "task-a".into(),
                    values: vec!["task-a".into(), "Fix Login".into()],
                },
                QueryRow {
                    id: "task-b".into(),
                    values: vec!["task-b".into(), "Add Dashboard".into()],
                },
            ],
        };

        let json = render_json(&result);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed.as_array().unwrap().len(), 2);
        assert_eq!(parsed[0]["id"], "task-a");
        assert_eq!(parsed[0]["title"], "Fix Login");
    }

    #[test]
    fn render_json_empty_result() {
        let result = QueryResult {
            columns: vec!["id".into()],
            items: vec![],
        };
        let json = render_json(&result);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 0);
    }
}
