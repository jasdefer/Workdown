//! Metric view extractor.
//!
//! Reduces the filtered item set to a single [`AggregateValue`] via the
//! view's aggregate. `Count` returns the item count directly; sum/avg/
//! min/max read the `value` field and aggregate via the shared helper.
//!
//! `MetricData.value` is `None` when the aggregate drops (avg/min/max
//! with zero valid inputs); renderers treat that as "no data." Items
//! filtered-in but skipped by the aggregate (missing value field) are
//! listed in `unplaced`.

use serde::Serialize;

use crate::model::schema::Schema;
use crate::model::views::{Aggregate, View, ViewKind};
use crate::model::FieldValue;
use crate::store::Store;

use super::aggregate::compute_aggregate;
use super::common::{build_card, AggregateValue, UnplacedCard, UnplacedReason};
use super::filter::filtered_items;

#[derive(Debug, Clone, Serialize)]
pub struct MetricData {
    pub label: Option<String>,
    pub aggregate: Aggregate,
    pub value_field: Option<String>,
    pub value: Option<AggregateValue>,
    pub unplaced: Vec<UnplacedCard>,
}

pub fn extract_metric(view: &View, store: &Store, schema: &Schema) -> MetricData {
    let ViewKind::Metric {
        label,
        value,
        aggregate,
    } = &view.kind
    else {
        panic!("extract_metric called with non-metric view kind");
    };
    let items = filtered_items(view, store, schema);

    let mut unplaced: Vec<UnplacedCard> = Vec::new();

    let metric_value = match aggregate {
        Aggregate::Count => Some(AggregateValue::Number(items.len() as f64)),
        _ => {
            let mut field_values: Vec<&FieldValue> = Vec::new();
            if let Some(value_field) = value {
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
            compute_aggregate(&field_values, *aggregate)
        }
    };

    unplaced.sort_by(|left, right| left.card.id.as_str().cmp(right.card.id.as_str()));

    MetricData {
        label: label.clone(),
        aggregate: *aggregate,
        value_field: value.clone(),
        value: metric_value,
        unplaced,
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

    fn metric_view(label: Option<&str>, value: Option<&str>, aggregate: Aggregate) -> View {
        View {
            id: "my-metric".into(),
            where_clauses: vec![],
            title: None,
            kind: ViewKind::Metric {
                label: label.map(str::to_owned),
                value: value.map(str::to_owned),
                aggregate,
            },
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
        let view = metric_view(None, None, Aggregate::Count);

        let data = extract_metric(&view, &store, &schema);

        assert!(matches!(data.value, Some(AggregateValue::Number(n)) if n == 3.0));
        assert!(data.unplaced.is_empty());
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
        let view = metric_view(None, Some("points"), Aggregate::Sum);

        let data = extract_metric(&view, &store, &schema);

        assert!(matches!(data.value, Some(AggregateValue::Number(n)) if (n - 10.0).abs() < 1e-9));
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
        let view = metric_view(None, Some("deadline"), Aggregate::Avg);

        let data = extract_metric(&view, &store, &schema);

        assert_eq!(data.value, Some(AggregateValue::Date(ymd(2026, 1, 6))));
    }

    #[test]
    fn avg_with_no_matching_values_is_none() {
        let schema = numeric_schema();
        let store = make_store(
            &schema,
            vec![
                make_item("a", vec![], ""),
                make_item("b", vec![], ""),
            ],
        );
        let view = metric_view(None, Some("points"), Aggregate::Avg);

        let data = extract_metric(&view, &store, &schema);

        assert_eq!(data.value, None);
        assert_eq!(data.unplaced.len(), 2);
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
        let view = metric_view(None, Some("points"), Aggregate::Sum);

        let data = extract_metric(&view, &store, &schema);

        assert!(matches!(data.value, Some(AggregateValue::Number(n)) if (n - 3.0).abs() < 1e-9));
        assert_eq!(data.unplaced.len(), 1);
        assert_eq!(data.unplaced[0].card.id.as_str(), "b");
    }

    #[test]
    fn label_passes_through() {
        let schema = numeric_schema();
        let store = make_store(&schema, vec![]);
        let view = metric_view(Some("Total Story Points"), None, Aggregate::Count);

        let data = extract_metric(&view, &store, &schema);

        assert_eq!(data.label.as_deref(), Some("Total Story Points"));
    }
}
