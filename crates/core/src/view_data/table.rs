//! Table view extractor.
//!
//! Produces one row per filtered item, with cells parallel to the view's
//! `columns:` list. The virtual `id` column emits the item id as a string
//! cell; real fields emit their typed [`FieldValue`] when set, `None`
//! when the item doesn't have that field.

use serde::Serialize;

use crate::model::schema::Schema;
use crate::model::views::{View, ViewKind};
use crate::model::{FieldValue, WorkItemId};
use crate::store::Store;

use super::filter::filtered_items;

#[derive(Debug, Clone, Serialize)]
pub struct TableData {
    pub columns: Vec<String>,
    pub rows: Vec<TableRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TableRow {
    pub id: WorkItemId,
    pub cells: Vec<Option<FieldValue>>,
}

pub fn extract_table(view: &View, store: &Store, schema: &Schema) -> TableData {
    let ViewKind::Table { columns } = &view.kind else {
        panic!("extract_table called with non-table view kind");
    };
    let items = filtered_items(view, store, schema);
    let rows = items
        .iter()
        .map(|item| {
            let cells = columns
                .iter()
                .map(|column| {
                    if column == "id" {
                        Some(FieldValue::String(item.id.as_str().to_owned()))
                    } else {
                        item.fields.get(column).cloned()
                    }
                })
                .collect();
            TableRow {
                id: item.id.clone(),
                cells,
            }
        })
        .collect();
    TableData {
        columns: columns.clone(),
        rows,
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::FieldTypeConfig;
    use crate::model::views::{View, ViewKind};
    use crate::view_data::test_support::{make_item, make_schema, make_store};

    fn table_view(columns: Vec<&str>, where_clauses: Vec<&str>) -> View {
        View {
            id: "my-table".to_owned(),
            where_clauses: where_clauses.into_iter().map(str::to_owned).collect(),
            title: None,
            kind: ViewKind::Table {
                columns: columns.into_iter().map(str::to_owned).collect(),
            },
        }
    }

    fn basic_schema() -> crate::model::schema::Schema {
        make_schema(vec![
            ("title", FieldTypeConfig::String { pattern: None }),
            (
                "status",
                FieldTypeConfig::Choice {
                    values: vec!["open".into(), "done".into()],
                },
            ),
            (
                "points",
                FieldTypeConfig::Integer {
                    min: None,
                    max: None,
                },
            ),
        ])
    }

    #[test]
    fn rows_sorted_by_id_ascending() {
        let schema = basic_schema();
        let store = make_store(
            &schema,
            vec![
                make_item("c", vec![("status", FieldValue::Choice("open".into()))], ""),
                make_item("a", vec![("status", FieldValue::Choice("done".into()))], ""),
                make_item("b", vec![("status", FieldValue::Choice("open".into()))], ""),
            ],
        );
        let view = table_view(vec!["id", "status"], vec![]);

        let data = extract_table(&view, &store, &schema);

        let ids: Vec<&str> = data.rows.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    #[test]
    fn id_column_emits_string_cell() {
        let schema = basic_schema();
        let store = make_store(
            &schema,
            vec![make_item(
                "task-a",
                vec![("status", FieldValue::Choice("open".into()))],
                "",
            )],
        );
        let view = table_view(vec!["id"], vec![]);

        let data = extract_table(&view, &store, &schema);

        assert_eq!(data.columns, vec!["id"]);
        assert_eq!(
            data.rows[0].cells,
            vec![Some(FieldValue::String("task-a".into()))]
        );
    }

    #[test]
    fn missing_cell_is_none() {
        let schema = basic_schema();
        let store = make_store(
            &schema,
            vec![make_item(
                "task-a",
                vec![("status", FieldValue::Choice("open".into()))],
                "",
            )],
        );
        let view = table_view(vec!["id", "points", "status"], vec![]);

        let data = extract_table(&view, &store, &schema);

        let cells = &data.rows[0].cells;
        assert_eq!(cells[0], Some(FieldValue::String("task-a".into())));
        assert_eq!(cells[1], None);
        assert_eq!(cells[2], Some(FieldValue::Choice("open".into())));
    }

    #[test]
    fn where_clause_filters_rows() {
        let schema = basic_schema();
        let store = make_store(
            &schema,
            vec![
                make_item("a", vec![("status", FieldValue::Choice("open".into()))], ""),
                make_item("b", vec![("status", FieldValue::Choice("done".into()))], ""),
                make_item("c", vec![("status", FieldValue::Choice("open".into()))], ""),
            ],
        );
        let view = table_view(vec!["id"], vec!["status=open"]);

        let data = extract_table(&view, &store, &schema);

        let ids: Vec<&str> = data.rows.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "c"]);
    }

    #[test]
    fn empty_store_produces_zero_rows() {
        let schema = basic_schema();
        let store = make_store(&schema, vec![]);
        let view = table_view(vec!["id", "status"], vec![]);

        let data = extract_table(&view, &store, &schema);

        assert_eq!(data.columns, vec!["id", "status"]);
        assert!(data.rows.is_empty());
    }

    #[test]
    fn columns_preserved_in_declaration_order() {
        let schema = basic_schema();
        let store = make_store(
            &schema,
            vec![make_item(
                "a",
                vec![
                    ("title", FieldValue::String("Hello".into())),
                    ("status", FieldValue::Choice("open".into())),
                    ("points", FieldValue::Integer(3)),
                ],
                "",
            )],
        );
        let view = table_view(vec!["points", "status", "title"], vec![]);

        let data = extract_table(&view, &store, &schema);

        assert_eq!(data.columns, vec!["points", "status", "title"]);
        assert_eq!(
            data.rows[0].cells,
            vec![
                Some(FieldValue::Integer(3)),
                Some(FieldValue::Choice("open".into())),
                Some(FieldValue::String("Hello".into())),
            ]
        );
    }
}
