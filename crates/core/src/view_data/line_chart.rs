//! Line chart view extractor.
//!
//! One point per filter-matched item with both `x` (numeric or date)
//! and `y` (numeric) set. Missing either side routes the item to
//! `unplaced` with `MissingValue`. Points sorted by `x` ascending,
//! ties broken by id for determinism.

use std::cmp::Ordering;

use serde::Serialize;

use crate::model::schema::Schema;
use crate::model::views::{View, ViewKind};
use crate::model::WorkItemId;
use crate::store::Store;

use super::common::{as_axis, as_number, build_card, AxisValue, UnplacedCard, UnplacedReason};
use super::filter::filtered_items;

#[derive(Debug, Clone, Serialize)]
pub struct LineChartData {
    pub x_field: String,
    pub y_field: String,
    pub points: Vec<LinePoint>,
    pub unplaced: Vec<UnplacedCard>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LinePoint {
    pub id: WorkItemId,
    pub x: AxisValue,
    pub y: f64,
}

pub fn extract_line_chart(view: &View, store: &Store, schema: &Schema) -> LineChartData {
    let ViewKind::LineChart { x, y } = &view.kind else {
        panic!("extract_line_chart called with non-line-chart view kind");
    };
    let items = filtered_items(view, store, schema);

    let mut points: Vec<LinePoint> = Vec::new();
    let mut unplaced: Vec<UnplacedCard> = Vec::new();

    for item in &items {
        let x_value = as_axis(item.fields.get(x));
        let y_value = as_number(item.fields.get(y));

        match (x_value, y_value) {
            (Some(x_value), Some(y_value)) => points.push(LinePoint {
                id: item.id.clone(),
                x: x_value,
                y: y_value,
            }),
            (None, _) => unplaced.push(UnplacedCard {
                card: build_card(item, schema, view),
                reason: UnplacedReason::MissingValue { field: x.clone() },
            }),
            (_, None) => unplaced.push(UnplacedCard {
                card: build_card(item, schema, view),
                reason: UnplacedReason::MissingValue { field: y.clone() },
            }),
        }
    }

    points.sort_by(|left, right| {
        compare_axis(&left.x, &right.x).then_with(|| left.id.as_str().cmp(right.id.as_str()))
    });
    unplaced.sort_by(|left, right| left.card.id.as_str().cmp(right.card.id.as_str()));

    LineChartData {
        x_field: x.clone(),
        y_field: y.clone(),
        points,
        unplaced,
    }
}

fn compare_axis(left: &AxisValue, right: &AxisValue) -> Ordering {
    match (left, right) {
        (AxisValue::Number(left), AxisValue::Number(right)) => {
            left.partial_cmp(right).unwrap_or(Ordering::Equal)
        }
        (AxisValue::Date(left), AxisValue::Date(right)) => left.cmp(right),
        // Same-field items are always the same variant; mixed types shouldn't
        // happen in practice but keep ordering total for determinism.
        (AxisValue::Number(_), AxisValue::Date(_)) => Ordering::Less,
        (AxisValue::Date(_), AxisValue::Number(_)) => Ordering::Greater,
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    use crate::model::schema::{FieldTypeConfig, Schema};
    use crate::model::views::{View, ViewKind};
    use crate::model::FieldValue;
    use crate::view_data::test_support::{make_item, make_schema, make_store};

    fn line_chart_view(x: &str, y: &str) -> View {
        View {
            id: "my-line".into(),
            where_clauses: vec![],
            title: None,
            kind: ViewKind::LineChart {
                x: x.to_owned(),
                y: y.to_owned(),
            },
        }
    }

    fn numeric_schema() -> Schema {
        make_schema(vec![
            (
                "progress",
                FieldTypeConfig::Integer {
                    min: None,
                    max: None,
                },
            ),
            (
                "score",
                FieldTypeConfig::Float {
                    min: None,
                    max: None,
                },
            ),
            ("day", FieldTypeConfig::Date),
        ])
    }

    fn ymd(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    #[test]
    fn numeric_x_and_y_produces_points_sorted_by_x() {
        let schema = numeric_schema();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "a",
                    vec![
                        ("progress", FieldValue::Integer(5)),
                        ("score", FieldValue::Float(2.0)),
                    ],
                    "",
                ),
                make_item(
                    "b",
                    vec![
                        ("progress", FieldValue::Integer(1)),
                        ("score", FieldValue::Float(1.0)),
                    ],
                    "",
                ),
                make_item(
                    "c",
                    vec![
                        ("progress", FieldValue::Integer(3)),
                        ("score", FieldValue::Float(3.0)),
                    ],
                    "",
                ),
            ],
        );
        let view = line_chart_view("progress", "score");

        let data = extract_line_chart(&view, &store, &schema);

        let xs: Vec<f64> = data
            .points
            .iter()
            .map(|p| match p.x {
                AxisValue::Number(n) => n,
                _ => panic!("expected numeric axis"),
            })
            .collect();
        assert_eq!(xs, vec![1.0, 3.0, 5.0]);
    }

    #[test]
    fn date_x_with_numeric_y() {
        let schema = numeric_schema();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "a",
                    vec![
                        ("day", FieldValue::Date(ymd(2026, 2, 1))),
                        ("score", FieldValue::Float(10.0)),
                    ],
                    "",
                ),
                make_item(
                    "b",
                    vec![
                        ("day", FieldValue::Date(ymd(2026, 1, 1))),
                        ("score", FieldValue::Float(5.0)),
                    ],
                    "",
                ),
            ],
        );
        let view = line_chart_view("day", "score");

        let data = extract_line_chart(&view, &store, &schema);

        let dates: Vec<NaiveDate> = data
            .points
            .iter()
            .map(|p| match p.x {
                AxisValue::Date(d) => d,
                _ => panic!("expected date axis"),
            })
            .collect();
        assert_eq!(dates, vec![ymd(2026, 1, 1), ymd(2026, 2, 1)]);
    }

    #[test]
    fn ties_on_x_broken_by_id() {
        let schema = numeric_schema();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "c",
                    vec![
                        ("progress", FieldValue::Integer(1)),
                        ("score", FieldValue::Float(3.0)),
                    ],
                    "",
                ),
                make_item(
                    "a",
                    vec![
                        ("progress", FieldValue::Integer(1)),
                        ("score", FieldValue::Float(1.0)),
                    ],
                    "",
                ),
                make_item(
                    "b",
                    vec![
                        ("progress", FieldValue::Integer(1)),
                        ("score", FieldValue::Float(2.0)),
                    ],
                    "",
                ),
            ],
        );
        let view = line_chart_view("progress", "score");

        let data = extract_line_chart(&view, &store, &schema);

        let ids: Vec<&str> = data.points.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    #[test]
    fn missing_x_is_unplaced() {
        let schema = numeric_schema();
        let store = make_store(
            &schema,
            vec![make_item(
                "a",
                vec![("score", FieldValue::Float(1.0))],
                "",
            )],
        );
        let view = line_chart_view("progress", "score");

        let data = extract_line_chart(&view, &store, &schema);

        assert!(data.points.is_empty());
        assert_eq!(data.unplaced.len(), 1);
        match &data.unplaced[0].reason {
            UnplacedReason::MissingValue { field } => assert_eq!(field, "progress"),
            other => panic!("expected MissingValue, got {other:?}"),
        }
    }

    #[test]
    fn missing_y_is_unplaced() {
        let schema = numeric_schema();
        let store = make_store(
            &schema,
            vec![make_item(
                "a",
                vec![("progress", FieldValue::Integer(1))],
                "",
            )],
        );
        let view = line_chart_view("progress", "score");

        let data = extract_line_chart(&view, &store, &schema);

        assert!(data.points.is_empty());
        match &data.unplaced[0].reason {
            UnplacedReason::MissingValue { field } => assert_eq!(field, "score"),
            other => panic!("expected MissingValue, got {other:?}"),
        }
    }
}
