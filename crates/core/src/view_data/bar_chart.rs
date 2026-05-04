//! Bar chart view extractor.
//!
//! Buckets filter-matched items by the stringified value of `group_by`
//! (multichoice/list/links items contribute to multiple groups) and
//! reduces each bucket via `aggregate`. Items with no value for the
//! grouping field, or — on non-count aggregates — no value for the
//! `value` field, land in `unplaced` as `MissingValue`.
//!
//! Bars are sorted by group key ascending (BTreeMap gives that for free).
//! avg/min/max over zero valid inputs produces no bar (dropped); sum and
//! count always produce a bar even if zero.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::model::schema::Schema;
use crate::model::views::{Aggregate, View, ViewKind};
use crate::model::{FieldValue, WorkItem};
use crate::query::format::format_field_value;
use crate::store::Store;

use super::aggregate::compute_aggregate;
use super::common::{build_card, AggregateValue, UnplacedCard, UnplacedReason};
use super::filter::filtered_items;

#[derive(Debug, Clone, Serialize)]
pub struct BarChartData {
    pub group_by: String,
    pub value_field: Option<String>,
    pub aggregate: Aggregate,
    pub bars: Vec<BarChartBar>,
    pub unplaced: Vec<UnplacedCard>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BarChartBar {
    pub group: String,
    pub value: AggregateValue,
}

pub fn extract_bar_chart(view: &View, store: &Store, schema: &Schema) -> BarChartData {
    let ViewKind::BarChart {
        group_by,
        value,
        aggregate,
    } = &view.kind
    else {
        panic!("extract_bar_chart called with non-bar-chart view kind");
    };
    let items = filtered_items(view, store, schema);

    let mut groups: BTreeMap<String, Vec<&WorkItem>> = BTreeMap::new();
    let mut unplaced: Vec<UnplacedCard> = Vec::new();

    for item in &items {
        let keys = keys_for_group(item, group_by);
        if keys.is_empty() {
            unplaced.push(UnplacedCard {
                card: build_card(item, schema, view),
                reason: UnplacedReason::MissingValue {
                    field: group_by.clone(),
                },
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

        for key in keys {
            groups.entry(key).or_default().push(*item);
        }
    }

    let mut bars: Vec<BarChartBar> = Vec::new();
    for (group, group_items) in groups {
        let result = match aggregate {
            Aggregate::Count => Some(AggregateValue::Number(group_items.len() as f64)),
            _ => {
                let field_values: Vec<&FieldValue> = match value.as_ref() {
                    Some(value_field) => group_items
                        .iter()
                        .filter_map(|item| item.fields.get(value_field))
                        .collect(),
                    None => Vec::new(),
                };
                compute_aggregate(&field_values, *aggregate)
            }
        };
        if let Some(result) = result {
            bars.push(BarChartBar {
                group,
                value: result,
            });
        }
    }

    unplaced.sort_by(|left, right| left.card.id.as_str().cmp(right.card.id.as_str()));

    BarChartData {
        group_by: group_by.clone(),
        value_field: value.clone(),
        aggregate: *aggregate,
        bars,
        unplaced,
    }
}

fn keys_for_group(item: &WorkItem, field: &str) -> Vec<String> {
    match item.fields.get(field) {
        None => Vec::new(),
        Some(FieldValue::Multichoice(values)) => values.clone(),
        Some(FieldValue::List(values)) => values.clone(),
        Some(FieldValue::Links(ids)) => ids.iter().map(|id| id.as_str().to_owned()).collect(),
        Some(other) => vec![format_field_value(other)],
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    use crate::model::schema::{FieldTypeConfig, Schema};
    use crate::model::views::{View, ViewKind};
    use crate::view_data::test_support::{make_item, make_schema, make_store};

    fn bar_chart_view(group_by: &str, value: Option<&str>, aggregate: Aggregate) -> View {
        View {
            id: "my-bar".into(),
            where_clauses: vec![],
            title: None,
            kind: ViewKind::BarChart {
                group_by: group_by.to_owned(),
                value: value.map(str::to_owned),
                aggregate,
            },
        }
    }

    fn status_schema() -> Schema {
        make_schema(vec![
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
            ("deadline", FieldTypeConfig::Date),
            (
                "tags",
                FieldTypeConfig::Multichoice {
                    values: vec!["alpha".into(), "beta".into()],
                },
            ),
        ])
    }

    fn ymd(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    #[test]
    fn count_buckets_items_per_group() {
        let schema = status_schema();
        let store = make_store(
            &schema,
            vec![
                make_item("a", vec![("status", FieldValue::Choice("open".into()))], ""),
                make_item("b", vec![("status", FieldValue::Choice("open".into()))], ""),
                make_item("c", vec![("status", FieldValue::Choice("done".into()))], ""),
            ],
        );
        let view = bar_chart_view("status", None, Aggregate::Count);

        let data = extract_bar_chart(&view, &store, &schema);

        assert_eq!(data.bars.len(), 2);
        let done = data.bars.iter().find(|b| b.group == "done").unwrap();
        let open = data.bars.iter().find(|b| b.group == "open").unwrap();
        assert!(matches!(done.value, AggregateValue::Number(n) if n == 1.0));
        assert!(matches!(open.value, AggregateValue::Number(n) if n == 2.0));
    }

    #[test]
    fn sum_aggregates_numeric_value_field() {
        let schema = status_schema();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "a",
                    vec![
                        ("status", FieldValue::Choice("open".into())),
                        ("points", FieldValue::Integer(3)),
                    ],
                    "",
                ),
                make_item(
                    "b",
                    vec![
                        ("status", FieldValue::Choice("open".into())),
                        ("points", FieldValue::Integer(5)),
                    ],
                    "",
                ),
            ],
        );
        let view = bar_chart_view("status", Some("points"), Aggregate::Sum);

        let data = extract_bar_chart(&view, &store, &schema);

        let open = data.bars.iter().find(|b| b.group == "open").unwrap();
        assert!(matches!(open.value, AggregateValue::Number(n) if (n - 8.0).abs() < 1e-9));
    }

    #[test]
    fn avg_over_date_field_produces_midpoint() {
        let schema = status_schema();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "a",
                    vec![
                        ("status", FieldValue::Choice("open".into())),
                        ("deadline", FieldValue::Date(ymd(2026, 1, 1))),
                    ],
                    "",
                ),
                make_item(
                    "b",
                    vec![
                        ("status", FieldValue::Choice("open".into())),
                        ("deadline", FieldValue::Date(ymd(2026, 1, 5))),
                    ],
                    "",
                ),
            ],
        );
        let view = bar_chart_view("status", Some("deadline"), Aggregate::Avg);

        let data = extract_bar_chart(&view, &store, &schema);

        let open = data.bars.iter().find(|b| b.group == "open").unwrap();
        assert_eq!(open.value, AggregateValue::Date(ymd(2026, 1, 3)));
    }

    #[test]
    fn multichoice_places_item_in_each_matching_group() {
        let schema = status_schema();
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
        let view = bar_chart_view("tags", None, Aggregate::Count);

        let data = extract_bar_chart(&view, &store, &schema);

        assert_eq!(data.bars.len(), 2);
        for bar in &data.bars {
            assert!(matches!(bar.value, AggregateValue::Number(n) if n == 1.0));
        }
    }

    #[test]
    fn missing_group_by_value_is_unplaced() {
        let schema = status_schema();
        let store = make_store(&schema, vec![make_item("a", vec![], "")]);
        let view = bar_chart_view("status", None, Aggregate::Count);

        let data = extract_bar_chart(&view, &store, &schema);

        assert!(data.bars.is_empty());
        assert_eq!(data.unplaced.len(), 1);
        match &data.unplaced[0].reason {
            UnplacedReason::MissingValue { field } => assert_eq!(field, "status"),
            other => panic!("expected MissingValue, got {other:?}"),
        }
    }

    #[test]
    fn missing_value_field_on_sum_is_unplaced() {
        let schema = status_schema();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "a",
                    vec![
                        ("status", FieldValue::Choice("open".into())),
                        ("points", FieldValue::Integer(3)),
                    ],
                    "",
                ),
                make_item("b", vec![("status", FieldValue::Choice("open".into()))], ""),
            ],
        );
        let view = bar_chart_view("status", Some("points"), Aggregate::Sum);

        let data = extract_bar_chart(&view, &store, &schema);

        let open = data.bars.iter().find(|b| b.group == "open").unwrap();
        assert!(matches!(open.value, AggregateValue::Number(n) if (n - 3.0).abs() < 1e-9));
        assert_eq!(data.unplaced.len(), 1);
        assert_eq!(data.unplaced[0].card.id.as_str(), "b");
    }

    #[test]
    fn avg_with_no_matching_values_drops_bar() {
        let schema = status_schema();
        // Two items in "open", neither has a points value; avg → None → drop bar.
        let store = make_store(
            &schema,
            vec![
                make_item("a", vec![("status", FieldValue::Choice("open".into()))], ""),
                make_item("b", vec![("status", FieldValue::Choice("open".into()))], ""),
            ],
        );
        let view = bar_chart_view("status", Some("points"), Aggregate::Avg);

        let data = extract_bar_chart(&view, &store, &schema);

        // Both items unplaced (missing points), group never has enough to aggregate.
        assert!(data.bars.is_empty());
        assert_eq!(data.unplaced.len(), 2);
    }

    #[test]
    fn bars_sorted_by_group_key_ascending() {
        let schema = status_schema();
        let store = make_store(
            &schema,
            vec![
                make_item("a", vec![("status", FieldValue::Choice("done".into()))], ""),
                make_item("b", vec![("status", FieldValue::Choice("open".into()))], ""),
            ],
        );
        let view = bar_chart_view("status", None, Aggregate::Count);

        let data = extract_bar_chart(&view, &store, &schema);

        let groups: Vec<&str> = data.bars.iter().map(|b| b.group.as_str()).collect();
        assert_eq!(groups, vec!["done", "open"]);
    }
}
