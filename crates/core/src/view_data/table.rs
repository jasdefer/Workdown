//! Table view extractor.
//!
//! Produces one row per filtered item, with cells parallel to the view's
//! `columns:` list. The virtual `id` column emits the item id as a string
//! cell; real fields emit their typed [`FieldValue`] when set, `None`
//! when the item doesn't have that field.
//!
//! Each column carries its [`FieldType`] so the UI can render and align
//! cells correctly even when every cell in a column is `None`. Link and
//! Links cells reference items by id; an `items` sidecar map resolves
//! those ids to display titles (via the view's `title:` slot, same
//! mechanism as [`Card`](super::common::Card)). Ids that don't resolve
//! to any item in the store are absent from the map — the UI treats
//! absence as "broken link, show the raw id".

use std::collections::HashMap;

use serde::Serialize;

use crate::model::schema::{FieldType, Schema};
use crate::model::views::{View, ViewKind};
use crate::model::{FieldValue, WorkItemId};
use crate::store::Store;

use super::common::resolve_title;
use super::filter::filtered_items;

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct TableData {
    pub columns: Vec<TableColumn>,
    pub rows: Vec<TableRow>,
    /// Resolution map for ids referenced by Link/Links cells. Absent ids
    /// are broken links — UI falls back to rendering the raw id.
    pub items: HashMap<WorkItemId, ItemRef>,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct TableColumn {
    pub name: String,
    pub field_type: FieldType,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct TableRow {
    pub id: WorkItemId,
    pub cells: Vec<Option<FieldValue>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
pub struct ItemRef {
    /// Resolved via the view's `title:` slot. `None` when the view has
    /// no title slot configured or the linked item lacks that field —
    /// the UI falls back to `prettifyId(id)` in that case.
    pub title: Option<String>,
}

pub fn extract_table(view: &View, store: &Store, schema: &Schema) -> TableData {
    let ViewKind::Table { columns } = &view.kind else {
        panic!("extract_table called with non-table view kind");
    };
    let items = filtered_items(view, store, schema);

    let table_columns: Vec<TableColumn> = columns
        .iter()
        .map(|column_name| TableColumn {
            name: column_name.clone(),
            field_type: column_field_type(column_name, schema),
        })
        .collect();

    let rows: Vec<TableRow> = items
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

    let items_sidecar = resolve_link_targets(&rows, view, store);

    TableData {
        columns: table_columns,
        rows,
        items: items_sidecar,
    }
}

/// Look up a column's field type. The virtual `id` column has no schema
/// definition; it always emits a String cell.
fn column_field_type(column_name: &str, schema: &Schema) -> FieldType {
    if column_name == "id" {
        return FieldType::String;
    }
    schema
        .fields
        .get(column_name)
        .expect("views_check validates column references")
        .field_type()
}

/// Walk every Link / Links cell, resolve each referenced id against the
/// store, and collect ids that resolve into the items sidecar. Broken
/// ids (no matching item) are intentionally omitted.
fn resolve_link_targets(
    rows: &[TableRow],
    view: &View,
    store: &Store,
) -> HashMap<WorkItemId, ItemRef> {
    let mut resolved: HashMap<WorkItemId, ItemRef> = HashMap::new();
    for row in rows {
        for cell in &row.cells {
            match cell {
                Some(FieldValue::Link(id)) => insert_ref(&mut resolved, id, view, store),
                Some(FieldValue::Links(ids)) => {
                    for id in ids {
                        insert_ref(&mut resolved, id, view, store);
                    }
                }
                _ => {}
            }
        }
    }
    resolved
}

fn insert_ref(
    resolved: &mut HashMap<WorkItemId, ItemRef>,
    id: &WorkItemId,
    view: &View,
    store: &Store,
) {
    if resolved.contains_key(id) {
        return;
    }
    let Some(item) = store.get(id.as_str()) else {
        return;
    };
    resolved.insert(
        id.clone(),
        ItemRef {
            title: resolve_title(item, view),
        },
    );
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::FieldTypeConfig;
    use crate::model::views::{View, ViewKind};
    use crate::view_data::test_support::{make_item, make_schema, make_store};

    fn table_view(columns: Vec<&str>, where_clauses: Vec<&str>) -> View {
        table_view_with_title(columns, where_clauses, None)
    }

    fn table_view_with_title(
        columns: Vec<&str>,
        where_clauses: Vec<&str>,
        title: Option<&str>,
    ) -> View {
        View {
            id: "my-table".to_owned(),
            where_clauses: where_clauses.into_iter().map(str::to_owned).collect(),
            title: title.map(str::to_owned),
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

    fn link_schema() -> crate::model::schema::Schema {
        make_schema(vec![
            ("title", FieldTypeConfig::String { pattern: None }),
            (
                "parent",
                FieldTypeConfig::Link {
                    allow_cycles: Some(false),
                    inverse: None,
                },
            ),
            (
                "depends_on",
                FieldTypeConfig::Links {
                    allow_cycles: Some(false),
                    inverse: None,
                },
            ),
        ])
    }

    fn column_names(data: &TableData) -> Vec<&str> {
        data.columns.iter().map(|column| column.name.as_str()).collect()
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

        assert_eq!(column_names(&data), vec!["id"]);
        assert_eq!(data.columns[0].field_type, FieldType::String);
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

        assert_eq!(column_names(&data), vec!["id", "status"]);
        assert!(data.rows.is_empty());
        assert!(data.items.is_empty());
    }

    #[test]
    fn columns_carry_field_types_from_schema() {
        let schema = basic_schema();
        let store = make_store(&schema, vec![]);
        let view = table_view(vec!["id", "title", "status", "points"], vec![]);

        let data = extract_table(&view, &store, &schema);

        let types: Vec<FieldType> = data
            .columns
            .iter()
            .map(|column| column.field_type)
            .collect();
        assert_eq!(
            types,
            vec![
                FieldType::String,
                FieldType::String,
                FieldType::Choice,
                FieldType::Integer,
            ]
        );
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

        assert_eq!(column_names(&data), vec!["points", "status", "title"]);
        assert_eq!(
            data.rows[0].cells,
            vec![
                Some(FieldValue::Integer(3)),
                Some(FieldValue::Choice("open".into())),
                Some(FieldValue::String("Hello".into())),
            ]
        );
    }

    #[test]
    fn link_cells_resolve_titles_when_target_exists() {
        let schema = link_schema();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "epic-1",
                    vec![("title", FieldValue::String("Auth epic".into()))],
                    "",
                ),
                make_item(
                    "task-a",
                    vec![(
                        "parent",
                        FieldValue::Link(WorkItemId::from("epic-1".to_owned())),
                    )],
                    "",
                ),
            ],
        );
        let view = table_view_with_title(vec!["id", "parent"], vec![], Some("title"));

        let data = extract_table(&view, &store, &schema);

        let epic_id = WorkItemId::from("epic-1".to_owned());
        assert_eq!(
            data.items.get(&epic_id),
            Some(&ItemRef {
                title: Some("Auth epic".into())
            })
        );
    }

    #[test]
    fn broken_link_cells_are_absent_from_items_map() {
        let schema = link_schema();
        let store = make_store(
            &schema,
            vec![make_item(
                "task-a",
                vec![(
                    "parent",
                    FieldValue::Link(WorkItemId::from("missing-epic".to_owned())),
                )],
                "",
            )],
        );
        let view = table_view_with_title(vec!["id", "parent"], vec![], Some("title"));

        let data = extract_table(&view, &store, &schema);

        assert!(data.items.is_empty());
    }

    #[test]
    fn links_field_resolves_every_target() {
        let schema = link_schema();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "task-a",
                    vec![("title", FieldValue::String("First".into()))],
                    "",
                ),
                make_item(
                    "task-b",
                    vec![("title", FieldValue::String("Second".into()))],
                    "",
                ),
                make_item(
                    "task-c",
                    vec![(
                        "depends_on",
                        FieldValue::Links(vec![
                            WorkItemId::from("task-a".to_owned()),
                            WorkItemId::from("task-b".to_owned()),
                            WorkItemId::from("ghost".to_owned()),
                        ]),
                    )],
                    "",
                ),
            ],
        );
        let view =
            table_view_with_title(vec!["id", "depends_on"], vec![], Some("title"));

        let data = extract_table(&view, &store, &schema);

        let task_a = WorkItemId::from("task-a".to_owned());
        let task_b = WorkItemId::from("task-b".to_owned());
        assert_eq!(
            data.items.get(&task_a).and_then(|r| r.title.as_deref()),
            Some("First")
        );
        assert_eq!(
            data.items.get(&task_b).and_then(|r| r.title.as_deref()),
            Some("Second")
        );
        assert!(!data
            .items
            .contains_key(&WorkItemId::from("ghost".to_owned())));
    }

    #[test]
    fn link_resolution_without_title_slot_carries_none_title() {
        let schema = link_schema();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "epic-1",
                    vec![("title", FieldValue::String("Auth epic".into()))],
                    "",
                ),
                make_item(
                    "task-a",
                    vec![(
                        "parent",
                        FieldValue::Link(WorkItemId::from("epic-1".to_owned())),
                    )],
                    "",
                ),
            ],
        );
        let view = table_view(vec!["id", "parent"], vec![]);

        let data = extract_table(&view, &store, &schema);

        let epic_id = WorkItemId::from("epic-1".to_owned());
        assert_eq!(
            data.items.get(&epic_id),
            Some(&ItemRef { title: None })
        );
    }
}
