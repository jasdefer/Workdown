//! Heatmap view extractor.
//!
//! Builds a grid of cells keyed by stringified `x`/`y` values. Axis fields
//! accept any type: choice/string/link/boolean/integer/float stringify via
//! [`format_field_value`]; multichoice/list/links contribute one label
//! per element (so an item lands in multiple cells); date axes format via
//! the `bucket` slot (day = `YYYY-MM-DD`, week = ISO `YYYY-Www`, month =
//! `YYYY-MM`). All ISO date formats are zero-padded so lex sort matches
//! chronological order.
//!
//! Items missing x or y land in `unplaced`; on non-count aggregates,
//! items missing the value field likewise. Cells are aggregated via the
//! shared helper; avg/min/max over zero valid values drops the cell.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::model::field_value::format_field_value;
use crate::model::schema::Schema;
use crate::model::views::{Aggregate, Bucket, View, ViewKind};
use crate::model::{FieldValue, WorkItem};
use crate::store::Store;

use super::aggregate::compute_aggregate;
use super::common::{build_card, AggregateValue, UnplacedCard, UnplacedReason};
use super::filter::filtered_items;

#[derive(Debug, Clone, Serialize)]
pub struct HeatmapData {
    pub x_field: String,
    pub y_field: String,
    pub value_field: Option<String>,
    pub aggregate: Aggregate,
    pub bucket: Option<Bucket>,
    pub x_labels: Vec<String>,
    pub y_labels: Vec<String>,
    pub cells: Vec<HeatmapCell>,
    pub unplaced: Vec<UnplacedCard>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HeatmapCell {
    pub x: String,
    pub y: String,
    pub value: AggregateValue,
}

pub fn extract_heatmap(view: &View, store: &Store, schema: &Schema) -> HeatmapData {
    let ViewKind::Heatmap {
        x,
        y,
        value,
        aggregate,
        bucket,
    } = &view.kind
    else {
        panic!("extract_heatmap called with non-heatmap view kind");
    };
    let items = filtered_items(view, store, schema);

    let mut cell_items: BTreeMap<(String, String), Vec<&WorkItem>> = BTreeMap::new();
    let mut x_labels: BTreeSet<String> = BTreeSet::new();
    let mut y_labels: BTreeSet<String> = BTreeSet::new();
    let mut unplaced: Vec<UnplacedCard> = Vec::new();

    for item in &items {
        let x_axis = axis_labels_for(item, x, *bucket);
        let y_axis = axis_labels_for(item, y, *bucket);

        if x_axis.is_empty() {
            unplaced.push(UnplacedCard {
                card: build_card(item, schema, view),
                reason: UnplacedReason::MissingValue { field: x.clone() },
            });
            continue;
        }
        if y_axis.is_empty() {
            unplaced.push(UnplacedCard {
                card: build_card(item, schema, view),
                reason: UnplacedReason::MissingValue { field: y.clone() },
            });
            continue;
        }

        if *aggregate != Aggregate::Count {
            if let Some(value_field) = value {
                if !item.fields.contains_key(value_field) {
                    unplaced.push(UnplacedCard {
                        card: build_card(item, schema, view),
                        reason: UnplacedReason::MissingValue {
                            field: value_field.clone(),
                        },
                    });
                    continue;
                }
            }
        }

        for x_label in &x_axis {
            x_labels.insert(x_label.clone());
        }
        for y_label in &y_axis {
            y_labels.insert(y_label.clone());
        }
        for x_label in &x_axis {
            for y_label in &y_axis {
                cell_items
                    .entry((x_label.clone(), y_label.clone()))
                    .or_default()
                    .push(*item);
            }
        }
    }

    let mut cells: Vec<HeatmapCell> = Vec::new();
    for ((x_label, y_label), items_in_cell) in cell_items {
        let result = match aggregate {
            Aggregate::Count => Some(AggregateValue::Number(items_in_cell.len() as f64)),
            _ => {
                let field_values: Vec<&FieldValue> = match value.as_ref() {
                    Some(value_field) => items_in_cell
                        .iter()
                        .filter_map(|item| item.fields.get(value_field))
                        .collect(),
                    None => Vec::new(),
                };
                compute_aggregate(&field_values, *aggregate)
            }
        };
        if let Some(result) = result {
            cells.push(HeatmapCell {
                x: x_label,
                y: y_label,
                value: result,
            });
        }
    }

    unplaced.sort_by(|left, right| left.card.id.as_str().cmp(right.card.id.as_str()));

    HeatmapData {
        x_field: x.clone(),
        y_field: y.clone(),
        value_field: value.clone(),
        aggregate: *aggregate,
        bucket: *bucket,
        x_labels: x_labels.into_iter().collect(),
        y_labels: y_labels.into_iter().collect(),
        cells,
        unplaced,
    }
}

fn axis_labels_for(item: &WorkItem, field: &str, bucket: Option<Bucket>) -> Vec<String> {
    match item.fields.get(field) {
        None => Vec::new(),
        Some(FieldValue::Multichoice(values)) => values.clone(),
        Some(FieldValue::List(values)) => values.clone(),
        Some(FieldValue::Links(ids)) => ids.iter().map(|id| id.as_str().to_owned()).collect(),
        Some(FieldValue::Date(date)) => {
            let formatted = match bucket {
                Some(Bucket::Week) => date.format("%G-W%V").to_string(),
                Some(Bucket::Month) => date.format("%Y-%m").to_string(),
                Some(Bucket::Day) | None => date.format("%Y-%m-%d").to_string(),
            };
            vec![formatted]
        }
        Some(other) => vec![format_field_value(other)],
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    use crate::model::schema::FieldTypeConfig;
    use crate::model::views::{View, ViewKind};
    use crate::view_data::test_support::{make_item, make_schema, make_store};

    fn heatmap_view(
        x: &str,
        y: &str,
        value: Option<&str>,
        aggregate: Aggregate,
        bucket: Option<Bucket>,
    ) -> View {
        View {
            id: "my-heatmap".into(),
            where_clauses: vec![],
            title: None,
            kind: ViewKind::Heatmap {
                x: x.to_owned(),
                y: y.to_owned(),
                value: value.map(str::to_owned),
                aggregate,
                bucket,
            },
        }
    }

    fn ymd(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    #[test]
    fn count_on_two_choice_axes() {
        let schema = make_schema(vec![
            (
                "status",
                FieldTypeConfig::Choice {
                    values: vec!["open".into(), "done".into()],
                },
            ),
            (
                "team",
                FieldTypeConfig::Choice {
                    values: vec!["eng".into(), "ops".into()],
                },
            ),
        ]);
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "a",
                    vec![
                        ("status", FieldValue::Choice("open".into())),
                        ("team", FieldValue::Choice("eng".into())),
                    ],
                    "",
                ),
                make_item(
                    "b",
                    vec![
                        ("status", FieldValue::Choice("open".into())),
                        ("team", FieldValue::Choice("eng".into())),
                    ],
                    "",
                ),
                make_item(
                    "c",
                    vec![
                        ("status", FieldValue::Choice("done".into())),
                        ("team", FieldValue::Choice("ops".into())),
                    ],
                    "",
                ),
            ],
        );
        let view = heatmap_view("status", "team", None, Aggregate::Count, None);

        let data = extract_heatmap(&view, &store, &schema);

        assert_eq!(data.x_labels, vec!["done", "open"]);
        assert_eq!(data.y_labels, vec!["eng", "ops"]);
        let open_eng = data
            .cells
            .iter()
            .find(|c| c.x == "open" && c.y == "eng")
            .unwrap();
        assert!(matches!(open_eng.value, AggregateValue::Number(n) if n == 2.0));
        let done_ops = data
            .cells
            .iter()
            .find(|c| c.x == "done" && c.y == "ops")
            .unwrap();
        assert!(matches!(done_ops.value, AggregateValue::Number(n) if n == 1.0));
    }

    #[test]
    fn date_axis_with_week_bucket_produces_iso_week_labels() {
        let schema = make_schema(vec![
            ("day", FieldTypeConfig::Date),
            (
                "team",
                FieldTypeConfig::Choice {
                    values: vec!["eng".into()],
                },
            ),
        ]);
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "a",
                    vec![
                        ("day", FieldValue::Date(ymd(2026, 1, 5))),
                        ("team", FieldValue::Choice("eng".into())),
                    ],
                    "",
                ),
                make_item(
                    "b",
                    vec![
                        ("day", FieldValue::Date(ymd(2026, 1, 7))),
                        ("team", FieldValue::Choice("eng".into())),
                    ],
                    "",
                ),
            ],
        );
        let view = heatmap_view("day", "team", None, Aggregate::Count, Some(Bucket::Week));

        let data = extract_heatmap(&view, &store, &schema);

        // 2026-01-05 is Monday of ISO week 02; 2026-01-07 is Wednesday of same week.
        assert_eq!(data.x_labels, vec!["2026-W02"]);
        assert_eq!(data.cells.len(), 1);
        assert!(matches!(data.cells[0].value, AggregateValue::Number(n) if n == 2.0));
    }

    #[test]
    fn month_bucket_groups_by_year_month() {
        let schema = make_schema(vec![
            ("day", FieldTypeConfig::Date),
            (
                "team",
                FieldTypeConfig::Choice {
                    values: vec!["eng".into()],
                },
            ),
        ]);
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "a",
                    vec![
                        ("day", FieldValue::Date(ymd(2026, 1, 5))),
                        ("team", FieldValue::Choice("eng".into())),
                    ],
                    "",
                ),
                make_item(
                    "b",
                    vec![
                        ("day", FieldValue::Date(ymd(2026, 2, 28))),
                        ("team", FieldValue::Choice("eng".into())),
                    ],
                    "",
                ),
            ],
        );
        let view = heatmap_view("day", "team", None, Aggregate::Count, Some(Bucket::Month));

        let data = extract_heatmap(&view, &store, &schema);

        assert_eq!(data.x_labels, vec!["2026-01", "2026-02"]);
    }

    #[test]
    fn multichoice_axis_contributes_to_every_matching_cell() {
        let schema = make_schema(vec![
            (
                "tags",
                FieldTypeConfig::Multichoice {
                    values: vec!["alpha".into(), "beta".into()],
                },
            ),
            (
                "team",
                FieldTypeConfig::Choice {
                    values: vec!["eng".into()],
                },
            ),
        ]);
        let store = make_store(
            &schema,
            vec![make_item(
                "a",
                vec![
                    (
                        "tags",
                        FieldValue::Multichoice(vec!["alpha".into(), "beta".into()]),
                    ),
                    ("team", FieldValue::Choice("eng".into())),
                ],
                "",
            )],
        );
        let view = heatmap_view("tags", "team", None, Aggregate::Count, None);

        let data = extract_heatmap(&view, &store, &schema);

        assert_eq!(data.cells.len(), 2);
        for cell in &data.cells {
            assert!(matches!(cell.value, AggregateValue::Number(n) if n == 1.0));
        }
    }

    #[test]
    fn sum_aggregate_over_numeric_value() {
        let schema = make_schema(vec![
            (
                "status",
                FieldTypeConfig::Choice {
                    values: vec!["open".into()],
                },
            ),
            (
                "team",
                FieldTypeConfig::Choice {
                    values: vec!["eng".into()],
                },
            ),
            (
                "points",
                FieldTypeConfig::Integer {
                    min: None,
                    max: None,
                },
            ),
        ]);
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "a",
                    vec![
                        ("status", FieldValue::Choice("open".into())),
                        ("team", FieldValue::Choice("eng".into())),
                        ("points", FieldValue::Integer(3)),
                    ],
                    "",
                ),
                make_item(
                    "b",
                    vec![
                        ("status", FieldValue::Choice("open".into())),
                        ("team", FieldValue::Choice("eng".into())),
                        ("points", FieldValue::Integer(5)),
                    ],
                    "",
                ),
            ],
        );
        let view = heatmap_view("status", "team", Some("points"), Aggregate::Sum, None);

        let data = extract_heatmap(&view, &store, &schema);

        assert_eq!(data.cells.len(), 1);
        assert!(matches!(data.cells[0].value, AggregateValue::Number(n) if (n - 8.0).abs() < 1e-9));
    }

    #[test]
    fn missing_x_is_unplaced() {
        let schema = make_schema(vec![
            (
                "status",
                FieldTypeConfig::Choice {
                    values: vec!["open".into()],
                },
            ),
            (
                "team",
                FieldTypeConfig::Choice {
                    values: vec!["eng".into()],
                },
            ),
        ]);
        let store = make_store(
            &schema,
            vec![make_item(
                "a",
                vec![("team", FieldValue::Choice("eng".into()))],
                "",
            )],
        );
        let view = heatmap_view("status", "team", None, Aggregate::Count, None);

        let data = extract_heatmap(&view, &store, &schema);

        assert!(data.cells.is_empty());
        match &data.unplaced[0].reason {
            UnplacedReason::MissingValue { field } => assert_eq!(field, "status"),
            other => panic!("expected MissingValue, got {other:?}"),
        }
    }

    #[test]
    fn missing_y_is_unplaced() {
        let schema = make_schema(vec![
            (
                "status",
                FieldTypeConfig::Choice {
                    values: vec!["open".into()],
                },
            ),
            (
                "team",
                FieldTypeConfig::Choice {
                    values: vec!["eng".into()],
                },
            ),
        ]);
        let store = make_store(
            &schema,
            vec![make_item(
                "a",
                vec![("status", FieldValue::Choice("open".into()))],
                "",
            )],
        );
        let view = heatmap_view("status", "team", None, Aggregate::Count, None);

        let data = extract_heatmap(&view, &store, &schema);

        match &data.unplaced[0].reason {
            UnplacedReason::MissingValue { field } => assert_eq!(field, "team"),
            other => panic!("expected MissingValue, got {other:?}"),
        }
    }
}
