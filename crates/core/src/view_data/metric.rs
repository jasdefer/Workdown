//! Metric view extractor.
//!
//! A metric view contains one or more rows, each rendered as a labelled
//! aggregate. Per-row `where` clauses AND-combine with the view-level
//! filter, so different rows can scope to different item subsets without
//! needing separate views.
//!
//! Each row reduces its filtered item set to a single
//! [`AggregateValue`]. `Count` returns the item count directly;
//! sum/avg/min/max read the row's `value` field and aggregate via the
//! shared helper. `MetricRowData.value` is `None` when the aggregate
//! drops (avg/min/max with zero valid inputs) — renderers treat that
//! as "no data". Items filtered-in but skipped by the aggregate
//! (missing value field) are listed in the row's `unplaced`.
//!
//! Row order matches definition order in `views.yaml`. Labels fall
//! back to a generated form (`"Count"`, `"Sum of <field>"`) when the
//! row leaves `label` unset.

use serde::Serialize;

use crate::model::schema::Schema;
use crate::model::views::{Aggregate, MetricRow, View, ViewKind};
use crate::model::FieldValue;
use crate::store::Store;

use super::aggregate::compute_aggregate;
use super::common::{build_card, AggregateValue, UnplacedCard, UnplacedReason};
use super::filter::filtered_items_with_extras;

#[derive(Debug, Clone, Serialize)]
pub struct MetricData {
    pub rows: Vec<MetricRowData>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricRowData {
    pub label: String,
    pub aggregate: Aggregate,
    pub value_field: Option<String>,
    pub value: Option<AggregateValue>,
    pub unplaced: Vec<UnplacedCard>,
}

pub fn extract_metric(view: &View, store: &Store, schema: &Schema) -> MetricData {
    let ViewKind::Metric { metrics } = &view.kind else {
        panic!("extract_metric called with non-metric view kind");
    };

    let rows = metrics
        .iter()
        .map(|row| extract_row(view, row, store, schema))
        .collect();

    MetricData { rows }
}

fn extract_row(view: &View, row: &MetricRow, store: &Store, schema: &Schema) -> MetricRowData {
    let items = filtered_items_with_extras(view, &row.where_clauses, store, schema);
    let mut unplaced: Vec<UnplacedCard> = Vec::new();

    let value = match row.aggregate {
        Aggregate::Count => Some(AggregateValue::Number(items.len() as f64)),
        _ => {
            let mut field_values: Vec<&FieldValue> = Vec::new();
            if let Some(value_field) = &row.value {
                for item in &items {
                    match item.fields.get(value_field) {
                        Some(field_value) => field_values.push(field_value),
                        None => unplaced.push(UnplacedCard {
                            card: build_card(item, schema, view),
                            reason: UnplacedReason::MissingValue {
                                field: value_field.clone(),
                            },
                        }),
                    }
                }
            }
            compute_aggregate(&field_values, row.aggregate)
        }
    };

    unplaced.sort_by(|left, right| left.card.id.as_str().cmp(right.card.id.as_str()));

    MetricRowData {
        label: resolve_label(row),
        aggregate: row.aggregate,
        value_field: row.value.clone(),
        value,
        unplaced,
    }
}

/// Use the row's explicit label when set; otherwise generate one from
/// the aggregate and value field. `count` has no value field so it
/// becomes plain `"Count"`; the others read as `"Sum of points"`,
/// `"Avg of estimate"`, etc.
fn resolve_label(row: &MetricRow) -> String {
    if let Some(label) = &row.label {
        return label.clone();
    }
    match (row.aggregate, &row.value) {
        (Aggregate::Count, _) => "Count".to_owned(),
        (aggregate, Some(field)) => {
            format!("{} of {field}", capitalize(&aggregate.to_string()))
        }
        (aggregate, None) => capitalize(&aggregate.to_string()),
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    use crate::model::schema::{FieldTypeConfig, Schema};
    use crate::model::views::{MetricRow, View, ViewKind};
    use crate::view_data::test_support::{make_item, make_schema, make_store};

    fn metric_view(rows: Vec<MetricRow>) -> View {
        View {
            id: "my-metric".into(),
            where_clauses: vec![],
            title: None,
            kind: ViewKind::Metric { metrics: rows },
        }
    }

    fn row(
        label: Option<&str>,
        aggregate: Aggregate,
        value: Option<&str>,
        where_clauses: Vec<&str>,
    ) -> MetricRow {
        MetricRow {
            label: label.map(str::to_owned),
            aggregate,
            value: value.map(str::to_owned),
            where_clauses: where_clauses.into_iter().map(str::to_owned).collect(),
        }
    }

    fn numeric_schema() -> Schema {
        make_schema(vec![
            (
                "points",
                FieldTypeConfig::Integer {
                    min: None,
                    max: None,
                },
            ),
            ("deadline", FieldTypeConfig::Date),
        ])
    }

    fn ymd(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    #[test]
    fn empty_metrics_produces_empty_rows() {
        let schema = numeric_schema();
        let store = make_store(&schema, vec![]);
        let view = metric_view(vec![]);

        let data = extract_metric(&view, &store, &schema);

        assert!(data.rows.is_empty());
    }

    #[test]
    fn count_returns_item_count() {
        let schema = numeric_schema();
        let store = make_store(
            &schema,
            vec![
                make_item("a", vec![], ""),
                make_item("b", vec![], ""),
                make_item("c", vec![], ""),
            ],
        );
        let view = metric_view(vec![row(None, Aggregate::Count, None, vec![])]);

        let data = extract_metric(&view, &store, &schema);

        assert_eq!(data.rows.len(), 1);
        assert!(matches!(
            data.rows[0].value,
            Some(AggregateValue::Number(n)) if n == 3.0
        ));
        assert!(data.rows[0].unplaced.is_empty());
    }

    #[test]
    fn sum_over_value_field() {
        let schema = numeric_schema();
        let store = make_store(
            &schema,
            vec![
                make_item("a", vec![("points", FieldValue::Integer(3))], ""),
                make_item("b", vec![("points", FieldValue::Integer(7))], ""),
            ],
        );
        let view = metric_view(vec![row(None, Aggregate::Sum, Some("points"), vec![])]);

        let data = extract_metric(&view, &store, &schema);

        assert!(matches!(
            data.rows[0].value,
            Some(AggregateValue::Number(n)) if (n - 10.0).abs() < 1e-9
        ));
    }

    #[test]
    fn avg_over_date_value_field_returns_date() {
        let schema = numeric_schema();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "a",
                    vec![("deadline", FieldValue::Date(ymd(2026, 1, 1)))],
                    "",
                ),
                make_item(
                    "b",
                    vec![("deadline", FieldValue::Date(ymd(2026, 1, 11)))],
                    "",
                ),
            ],
        );
        let view = metric_view(vec![row(None, Aggregate::Avg, Some("deadline"), vec![])]);

        let data = extract_metric(&view, &store, &schema);

        assert_eq!(
            data.rows[0].value,
            Some(AggregateValue::Date(ymd(2026, 1, 6)))
        );
    }

    #[test]
    fn avg_with_no_matching_values_is_none() {
        let schema = numeric_schema();
        let store = make_store(
            &schema,
            vec![make_item("a", vec![], ""), make_item("b", vec![], "")],
        );
        let view = metric_view(vec![row(None, Aggregate::Avg, Some("points"), vec![])]);

        let data = extract_metric(&view, &store, &schema);

        assert_eq!(data.rows[0].value, None);
        assert_eq!(data.rows[0].unplaced.len(), 2);
    }

    #[test]
    fn missing_value_field_items_listed_in_unplaced() {
        let schema = numeric_schema();
        let store = make_store(
            &schema,
            vec![
                make_item("a", vec![("points", FieldValue::Integer(3))], ""),
                make_item("b", vec![], ""),
            ],
        );
        let view = metric_view(vec![row(None, Aggregate::Sum, Some("points"), vec![])]);

        let data = extract_metric(&view, &store, &schema);

        assert!(matches!(
            data.rows[0].value,
            Some(AggregateValue::Number(n)) if (n - 3.0).abs() < 1e-9
        ));
        assert_eq!(data.rows[0].unplaced.len(), 1);
        assert_eq!(data.rows[0].unplaced[0].card.id.as_str(), "b");
    }

    #[test]
    fn explicit_label_passes_through() {
        let schema = numeric_schema();
        let store = make_store(&schema, vec![]);
        let view = metric_view(vec![row(
            Some("Total Story Points"),
            Aggregate::Count,
            None,
            vec![],
        )]);

        let data = extract_metric(&view, &store, &schema);

        assert_eq!(data.rows[0].label, "Total Story Points");
    }

    #[test]
    fn auto_label_for_count() {
        let schema = numeric_schema();
        let store = make_store(&schema, vec![]);
        let view = metric_view(vec![row(None, Aggregate::Count, None, vec![])]);

        let data = extract_metric(&view, &store, &schema);

        assert_eq!(data.rows[0].label, "Count");
    }

    #[test]
    fn auto_label_for_sum_with_value() {
        let schema = numeric_schema();
        let store = make_store(&schema, vec![]);
        let view = metric_view(vec![row(None, Aggregate::Sum, Some("points"), vec![])]);

        let data = extract_metric(&view, &store, &schema);

        assert_eq!(data.rows[0].label, "Sum of points");
    }

    #[test]
    fn rows_appear_in_definition_order() {
        let schema = numeric_schema();
        let store = make_store(
            &schema,
            vec![
                make_item("a", vec![("points", FieldValue::Integer(3))], ""),
                make_item("b", vec![("points", FieldValue::Integer(7))], ""),
            ],
        );
        let view = metric_view(vec![
            row(Some("Total"), Aggregate::Count, None, vec![]),
            row(Some("Sum"), Aggregate::Sum, Some("points"), vec![]),
        ]);

        let data = extract_metric(&view, &store, &schema);

        assert_eq!(data.rows.len(), 2);
        assert_eq!(data.rows[0].label, "Total");
        assert_eq!(data.rows[1].label, "Sum");
    }

    #[test]
    fn per_row_where_filters_independently() {
        let schema = make_schema(vec![
            (
                "points",
                FieldTypeConfig::Integer {
                    min: None,
                    max: None,
                },
            ),
            (
                "status",
                FieldTypeConfig::Choice {
                    values: vec!["open".into(), "done".into()],
                },
            ),
        ]);
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "a",
                    vec![
                        ("points", FieldValue::Integer(3)),
                        ("status", FieldValue::Choice("open".into())),
                    ],
                    "",
                ),
                make_item(
                    "b",
                    vec![
                        ("points", FieldValue::Integer(7)),
                        ("status", FieldValue::Choice("done".into())),
                    ],
                    "",
                ),
            ],
        );
        let view = metric_view(vec![
            row(Some("Total"), Aggregate::Count, None, vec![]),
            row(Some("Open"), Aggregate::Count, None, vec!["status=open"]),
            row(
                Some("Done points"),
                Aggregate::Sum,
                Some("points"),
                vec!["status=done"],
            ),
        ]);

        let data = extract_metric(&view, &store, &schema);

        assert!(matches!(data.rows[0].value, Some(AggregateValue::Number(n)) if n == 2.0));
        assert!(matches!(data.rows[1].value, Some(AggregateValue::Number(n)) if n == 1.0));
        assert!(
            matches!(data.rows[2].value, Some(AggregateValue::Number(n)) if (n - 7.0).abs() < 1e-9)
        );
    }

    #[test]
    fn duration_sum_returns_duration_aggregate() {
        let schema = make_schema(vec![(
            "estimate",
            FieldTypeConfig::Duration {
                min: None,
                max: None,
            },
        )]);
        let store = make_store(
            &schema,
            vec![
                make_item("a", vec![("estimate", FieldValue::Duration(3600))], ""),
                make_item("b", vec![("estimate", FieldValue::Duration(7200))], ""),
            ],
        );
        let view = metric_view(vec![row(None, Aggregate::Sum, Some("estimate"), vec![])]);

        let data = extract_metric(&view, &store, &schema);

        assert_eq!(data.rows[0].value, Some(AggregateValue::Duration(10800)));
    }
}
