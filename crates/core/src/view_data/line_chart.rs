//! Line chart view extractor.
//!
//! One point per filter-matched item with both `x` (numeric, date, or
//! duration) and `y` (numeric or duration) set. Missing either side
//! routes the item to `unplaced` with `MissingValue`. Points sorted by
//! `x` ascending, ties broken by id for determinism.
//!
//! Optional `group` slot splits points into named series. Items missing
//! the group value still emit a point, with `group: None` — the renderer
//! gathers those into a synthetic `(no <field>)` series, parallel to
//! gantt's section handling. Group field type is validated upstream
//! (`views_check`).

use std::cmp::Ordering;

use serde::Serialize;

use crate::model::schema::Schema;
use crate::model::views::{View, ViewKind};
use crate::model::WorkItemId;
use crate::query::format::format_field_value;
use crate::store::Store;

use super::common::{
    as_axis, as_size, build_card, AxisValue, SizeValue, UnplacedCard, UnplacedReason,
};
use super::filter::filtered_items;

#[derive(Debug, Clone, Serialize)]
pub struct LineChartData {
    pub x_field: String,
    pub y_field: String,
    /// Field name used to split points into series, when set on the view.
    /// `None` means single-series; the renderer skips the legend.
    pub group_field: Option<String>,
    pub points: Vec<LinePoint>,
    pub unplaced: Vec<UnplacedCard>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LinePoint {
    pub id: WorkItemId,
    pub x: AxisValue,
    pub y: SizeValue,
    /// Stringified value of the view's `group` field for this item.
    /// `None` when no group field is configured, or when the item has no
    /// value for it — both routed through the same renderer code path.
    pub group: Option<String>,
}

pub fn extract_line_chart(view: &View, store: &Store, schema: &Schema) -> LineChartData {
    let ViewKind::LineChart { x, y, group } = &view.kind else {
        panic!("extract_line_chart called with non-line-chart view kind");
    };
    let items = filtered_items(view, store, schema);

    let mut points: Vec<LinePoint> = Vec::new();
    let mut unplaced: Vec<UnplacedCard> = Vec::new();

    for item in &items {
        let x_value = as_axis(item.fields.get(x));
        let y_value = as_size(item.fields.get(y));

        match (x_value, y_value) {
            (Some(x_value), Some(y_value)) => {
                let group_value = group
                    .as_deref()
                    .and_then(|name| item.fields.get(name).map(format_field_value));
                points.push(LinePoint {
                    id: item.id.clone(),
                    x: x_value,
                    y: y_value,
                    group: group_value,
                });
            }
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
        group_field: group.clone(),
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
        (AxisValue::Duration(left), AxisValue::Duration(right)) => left.cmp(right),
        // Same-field items are always the same variant; mixed types shouldn't
        // happen in practice but keep ordering total for determinism.
        (AxisValue::Number(_), _) => Ordering::Less,
        (_, AxisValue::Number(_)) => Ordering::Greater,
        (AxisValue::Duration(_), AxisValue::Date(_)) => Ordering::Less,
        (AxisValue::Date(_), AxisValue::Duration(_)) => Ordering::Greater,
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
        line_chart_view_with_group(x, y, None)
    }

    fn line_chart_view_with_group(x: &str, y: &str, group: Option<&str>) -> View {
        View {
            id: "my-line".into(),
            where_clauses: vec![],
            title: None,
            kind: ViewKind::LineChart {
                x: x.to_owned(),
                y: y.to_owned(),
                group: group.map(str::to_owned),
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
            vec![make_item("a", vec![("score", FieldValue::Float(1.0))], "")],
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

    // ── Grouping ────────────────────────────────────────────────────

    fn grouped_schema() -> Schema {
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
            (
                "team",
                FieldTypeConfig::Choice {
                    values: vec!["eng".into(), "ops".into()],
                },
            ),
        ])
    }

    #[test]
    fn group_field_unset_yields_none_on_every_point() {
        let schema = grouped_schema();
        let store = make_store(
            &schema,
            vec![make_item(
                "a",
                vec![
                    ("progress", FieldValue::Integer(1)),
                    ("score", FieldValue::Float(1.0)),
                    ("team", FieldValue::Choice("eng".into())),
                ],
                "",
            )],
        );
        let view = line_chart_view("progress", "score");

        let data = extract_line_chart(&view, &store, &schema);

        assert_eq!(data.group_field, None);
        assert_eq!(data.points[0].group, None);
    }

    #[test]
    fn group_field_set_carries_value_through_to_point() {
        let schema = grouped_schema();
        let store = make_store(
            &schema,
            vec![make_item(
                "a",
                vec![
                    ("progress", FieldValue::Integer(1)),
                    ("score", FieldValue::Float(1.0)),
                    ("team", FieldValue::Choice("eng".into())),
                ],
                "",
            )],
        );
        let view = line_chart_view_with_group("progress", "score", Some("team"));

        let data = extract_line_chart(&view, &store, &schema);

        assert_eq!(data.group_field.as_deref(), Some("team"));
        assert_eq!(data.points[0].group.as_deref(), Some("eng"));
    }

    #[test]
    fn group_field_set_but_value_missing_yields_none() {
        let schema = grouped_schema();
        let store = make_store(
            &schema,
            vec![make_item(
                "a",
                vec![
                    ("progress", FieldValue::Integer(1)),
                    ("score", FieldValue::Float(1.0)),
                ],
                "",
            )],
        );
        let view = line_chart_view_with_group("progress", "score", Some("team"));

        let data = extract_line_chart(&view, &store, &schema);

        assert_eq!(data.group_field.as_deref(), Some("team"));
        assert_eq!(data.points[0].group, None);
    }

    #[test]
    fn group_field_does_not_affect_x_sort() {
        // Sort is still by x then id — group is metadata for the renderer.
        let schema = grouped_schema();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "c",
                    vec![
                        ("progress", FieldValue::Integer(3)),
                        ("score", FieldValue::Float(3.0)),
                        ("team", FieldValue::Choice("eng".into())),
                    ],
                    "",
                ),
                make_item(
                    "a",
                    vec![
                        ("progress", FieldValue::Integer(1)),
                        ("score", FieldValue::Float(1.0)),
                        ("team", FieldValue::Choice("ops".into())),
                    ],
                    "",
                ),
                make_item(
                    "b",
                    vec![
                        ("progress", FieldValue::Integer(2)),
                        ("score", FieldValue::Float(2.0)),
                        ("team", FieldValue::Choice("eng".into())),
                    ],
                    "",
                ),
            ],
        );
        let view = line_chart_view_with_group("progress", "score", Some("team"));

        let data = extract_line_chart(&view, &store, &schema);

        let ids: Vec<&str> = data.points.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }
}
