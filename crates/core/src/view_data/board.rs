//! Board view extractor.
//!
//! Groups filter-matched items into columns by a choice/multichoice/string
//! field. Choice and multichoice columns follow the schema-declared value
//! order; string columns follow alphabetical order of values discovered
//! on items. A synthetic `value: None` column at the end collects items
//! whose grouping field is missing or empty; multichoice items with
//! multiple values appear in each matching column.

use serde::Serialize;

use crate::model::schema::{FieldTypeConfig, Schema};
use crate::model::views::{View, ViewKind};
use crate::model::{FieldValue, WorkItem};
use crate::store::Store;

use super::common::{build_card, Card};
use super::filter::filtered_items;

#[derive(Debug, Clone, Serialize)]
pub struct BoardData {
    pub field: String,
    pub columns: Vec<BoardColumn>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BoardColumn {
    /// `None` = synthetic bucket for items without a value for the grouping field.
    pub value: Option<String>,
    pub cards: Vec<Card>,
}

pub fn extract_board(view: &View, store: &Store, schema: &Schema) -> BoardData {
    let ViewKind::Board { field } = &view.kind else {
        panic!("extract_board called with non-board view kind");
    };
    let items = filtered_items(view, store, schema);
    let field_def = schema
        .fields
        .get(field)
        .expect("views_check validates field reference");

    let mut columns: Vec<BoardColumn> = column_values(&field_def.type_config, &items, field)
        .into_iter()
        .map(|value| BoardColumn {
            value: Some(value),
            cards: Vec::new(),
        })
        .collect();
    let mut synthetic = BoardColumn {
        value: None,
        cards: Vec::new(),
    };

    for item in &items {
        let card = build_card(item, schema, view);
        let placed = match item.fields.get(field) {
            Some(FieldValue::Choice(value)) => place_in(&mut columns, value, &card),
            Some(FieldValue::String(value)) => place_in(&mut columns, value, &card),
            Some(FieldValue::Multichoice(values)) => {
                let mut any = false;
                for value in values {
                    if place_in(&mut columns, value, &card) {
                        any = true;
                    }
                }
                any
            }
            _ => false,
        };
        if !placed {
            synthetic.cards.push(card);
        }
    }

    columns.push(synthetic);
    BoardData {
        field: field.clone(),
        columns,
    }
}

fn column_values(config: &FieldTypeConfig, items: &[&WorkItem], field: &str) -> Vec<String> {
    match config {
        FieldTypeConfig::Choice { values } | FieldTypeConfig::Multichoice { values } => {
            values.clone()
        }
        FieldTypeConfig::String { .. } => {
            let mut discovered: Vec<String> = items
                .iter()
                .filter_map(|item| match item.fields.get(field) {
                    Some(FieldValue::String(value)) => Some(value.clone()),
                    _ => None,
                })
                .collect();
            discovered.sort();
            discovered.dedup();
            discovered
        }
        _ => panic!("views_check rejects board fields of other types"),
    }
}

fn place_in(columns: &mut [BoardColumn], value: &str, card: &Card) -> bool {
    for column in columns.iter_mut() {
        if column.value.as_deref() == Some(value) {
            column.cards.push(card.clone());
            return true;
        }
    }
    false
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::{FieldTypeConfig, Schema};
    use crate::model::views::{View, ViewKind};
    use crate::view_data::test_support::{make_item, make_schema, make_store};

    fn board_view(field: &str) -> View {
        View {
            id: "my-board".into(),
            where_clauses: vec![],
            title: None,
            kind: ViewKind::Board {
                field: field.to_owned(),
            },
        }
    }

    fn choice_schema() -> Schema {
        make_schema(vec![(
            "status",
            FieldTypeConfig::Choice {
                values: vec!["open".into(), "in_progress".into(), "done".into()],
            },
        )])
    }

    #[test]
    fn columns_follow_schema_value_order_with_synthetic_last() {
        let schema = choice_schema();
        let store = make_store(
            &schema,
            vec![
                make_item("a", vec![("status", FieldValue::Choice("done".into()))], ""),
                make_item("b", vec![("status", FieldValue::Choice("open".into()))], ""),
            ],
        );
        let view = board_view("status");

        let data = extract_board(&view, &store, &schema);

        let values: Vec<Option<&str>> = data.columns.iter().map(|c| c.value.as_deref()).collect();
        assert_eq!(
            values,
            vec![Some("open"), Some("in_progress"), Some("done"), None]
        );
    }

    #[test]
    fn items_placed_in_matching_columns() {
        let schema = choice_schema();
        let store = make_store(
            &schema,
            vec![
                make_item("a", vec![("status", FieldValue::Choice("open".into()))], ""),
                make_item("b", vec![("status", FieldValue::Choice("done".into()))], ""),
                make_item("c", vec![("status", FieldValue::Choice("open".into()))], ""),
            ],
        );
        let view = board_view("status");

        let data = extract_board(&view, &store, &schema);

        let open = data
            .columns
            .iter()
            .find(|c| c.value.as_deref() == Some("open"))
            .unwrap();
        let done = data
            .columns
            .iter()
            .find(|c| c.value.as_deref() == Some("done"))
            .unwrap();
        let ids: Vec<&str> = open.cards.iter().map(|card| card.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "c"]);
        assert_eq!(done.cards.len(), 1);
    }

    #[test]
    fn missing_value_goes_to_synthetic_column() {
        let schema = choice_schema();
        let store = make_store(
            &schema,
            vec![
                make_item("a", vec![], ""),
                make_item("b", vec![("status", FieldValue::Choice("open".into()))], ""),
            ],
        );
        let view = board_view("status");

        let data = extract_board(&view, &store, &schema);

        let synthetic = data.columns.last().unwrap();
        assert_eq!(synthetic.value, None);
        assert_eq!(synthetic.cards.len(), 1);
        assert_eq!(synthetic.cards[0].id.as_str(), "a");
    }

    #[test]
    fn multichoice_card_appears_in_every_matching_column() {
        let schema = make_schema(vec![(
            "tags",
            FieldTypeConfig::Multichoice {
                values: vec!["alpha".into(), "beta".into(), "gamma".into()],
            },
        )]);
        let store = make_store(
            &schema,
            vec![make_item(
                "a",
                vec![(
                    "tags",
                    FieldValue::Multichoice(vec!["alpha".into(), "beta".into()]),
                )],
                "",
            )],
        );
        let view = board_view("tags");

        let data = extract_board(&view, &store, &schema);

        assert_eq!(data.columns[0].cards.len(), 1); // alpha
        assert_eq!(data.columns[1].cards.len(), 1); // beta
        assert_eq!(data.columns[2].cards.len(), 0); // gamma
    }

    #[test]
    fn empty_multichoice_goes_to_synthetic() {
        let schema = make_schema(vec![(
            "tags",
            FieldTypeConfig::Multichoice {
                values: vec!["alpha".into(), "beta".into()],
            },
        )]);
        let store = make_store(
            &schema,
            vec![make_item(
                "a",
                vec![("tags", FieldValue::Multichoice(vec![]))],
                "",
            )],
        );
        let view = board_view("tags");

        let data = extract_board(&view, &store, &schema);

        let synthetic = data.columns.last().unwrap();
        assert_eq!(synthetic.value, None);
        assert_eq!(synthetic.cards.len(), 1);
    }

    #[test]
    fn string_field_columns_discovered_alphabetically() {
        let schema = make_schema(vec![("team", FieldTypeConfig::String { pattern: None })]);
        let store = make_store(
            &schema,
            vec![
                make_item("a", vec![("team", FieldValue::String("ops".into()))], ""),
                make_item("b", vec![("team", FieldValue::String("eng".into()))], ""),
                make_item("c", vec![("team", FieldValue::String("eng".into()))], ""),
                make_item("d", vec![], ""),
            ],
        );
        let view = board_view("team");

        let data = extract_board(&view, &store, &schema);

        let values: Vec<Option<&str>> = data.columns.iter().map(|c| c.value.as_deref()).collect();
        assert_eq!(values, vec![Some("eng"), Some("ops"), None]);
        assert_eq!(data.columns[0].cards.len(), 2);
        assert_eq!(data.columns[1].cards.len(), 1);
        assert_eq!(data.columns[2].cards.len(), 1);
    }

    #[test]
    fn cards_within_column_sorted_by_id() {
        let schema = choice_schema();
        let store = make_store(
            &schema,
            vec![
                make_item("c", vec![("status", FieldValue::Choice("open".into()))], ""),
                make_item("a", vec![("status", FieldValue::Choice("open".into()))], ""),
                make_item("b", vec![("status", FieldValue::Choice("open".into()))], ""),
            ],
        );
        let view = board_view("status");

        let data = extract_board(&view, &store, &schema);

        let open = &data.columns[0];
        let ids: Vec<&str> = open.cards.iter().map(|card| card.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }
}
