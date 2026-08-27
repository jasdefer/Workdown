//! Line chart view extractor.
//!
//! One point per filter-matched item with both `x` (numeric, date, or
//! duration) and `y` (numeric or duration) set. Missing either side
//! routes the item to `unplaced` with `MissingValue`. Points sorted by
//! `x` ascending, ties broken by id for determinism.
//!
//! Points are partitioned into [`LineSeries`] here, not in renderers, so
//! every front end draws the same lines in the same order: one series
//! per distinct group value ordered by that value ascending, then a
//! final series with `group: None` collecting items that have no value
//! for the group field — the same synthetic-last convention board uses
//! for columns and gantt for sections. Renderers name that series in
//! their own words and pick their own palette; the partition and the
//! order are settled here.
//!
//! With no `group` slot on the view every point lands in one series
//! whose `group` is `None`; renderers branch on `group_field.is_none()`
//! to skip the legend, exactly as gantt does for its bands. Group field
//! type is validated upstream (`views_check`).

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};

use serde::Serialize;

use crate::model::field_value::format_field_value;
use crate::model::schema::Schema;
use crate::model::views::{View, ViewKind};
use crate::model::WorkItemId;
use crate::store::Store;

use super::common::{
    as_axis, as_size, build_card, resolve_title, sort_unplaced, ChartValue, ItemRef, SizeValue,
    UnplacedCard, UnplacedReason,
};
use super::filter::filtered_items;

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct LineChartData {
    pub x_field: String,
    pub y_field: String,
    /// Field name used to split points into series, when set on the view.
    /// `None` means single-series; renderers skip the legend.
    pub group_field: Option<String>,
    /// Drawable series in draw order. Empty when nothing matched.
    pub series: Vec<LineSeries>,
    /// Resolution map for the ids carried by the series' points. Title
    /// resolves via the view's `title` display role; `None` when unset
    /// or absent on the item — UI falls back to `prettifyId(id)`.
    /// Mirrors the Table pattern, since hover tooltips on points need
    /// item titles and `LinePoint` carries only the raw id.
    pub items: HashMap<WorkItemId, ItemRef>,
    pub unplaced: Vec<UnplacedCard>,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct LineSeries {
    /// Stringified value of the view's `group` field shared by every
    /// point here. `None` marks the synthetic series — items with no
    /// value for the group field, or the single series of an ungrouped
    /// chart. Renderers turn it into a label; the extractor never does.
    pub group: Option<String>,
    /// Points in x-ascending order, ties broken by id.
    pub points: Vec<LinePoint>,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct LinePoint {
    pub id: WorkItemId,
    pub x: ChartValue,
    pub y: SizeValue,
}

pub fn extract_line_chart(view: &View, store: &Store, schema: &Schema) -> LineChartData {
    let ViewKind::LineChart { x, y, group } = &view.kind else {
        panic!("extract_line_chart called with non-line-chart view kind");
    };
    let items = filtered_items(view, store, schema);

    let mut collected: Vec<(Option<String>, LinePoint)> = Vec::new();
    let mut items_sidecar: HashMap<WorkItemId, ItemRef> = HashMap::new();
    let mut unplaced: Vec<UnplacedCard> = Vec::new();

    for item in &items {
        let x_value = as_axis(item.fields.get(x));
        let y_value = as_size(item.fields.get(y));

        match (x_value, y_value) {
            (Some(x_value), Some(y_value)) => {
                let group_value = group
                    .as_deref()
                    .and_then(|name| item.fields.get(name).map(format_field_value));
                items_sidecar.insert(
                    item.id.clone(),
                    ItemRef {
                        title: resolve_title(item, view),
                    },
                );
                collected.push((
                    group_value,
                    LinePoint {
                        id: item.id.clone(),
                        x: x_value,
                        y: y_value,
                    },
                ));
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

    // Sort once across all points, then partition: each series inherits
    // the global x-ascending order without a second sort per group.
    collected.sort_by(|(_, left), (_, right)| {
        compare_axis(&left.x, &right.x).then_with(|| left.id.as_str().cmp(right.id.as_str()))
    });
    sort_unplaced(&mut unplaced);

    LineChartData {
        x_field: x.clone(),
        y_field: y.clone(),
        group_field: group.clone(),
        series: build_series(collected),
        items: items_sidecar,
        unplaced,
    }
}

/// Partition x-sorted points into series: one per distinct group value
/// in ascending order, then the no-value series last.
fn build_series(collected: Vec<(Option<String>, LinePoint)>) -> Vec<LineSeries> {
    let mut by_group: BTreeMap<String, Vec<LinePoint>> = BTreeMap::new();
    let mut no_value: Vec<LinePoint> = Vec::new();

    for (group_value, point) in collected {
        match group_value {
            Some(label) => by_group.entry(label).or_default().push(point),
            None => no_value.push(point),
        }
    }

    let mut series: Vec<LineSeries> = by_group
        .into_iter()
        .map(|(label, points)| LineSeries {
            group: Some(label),
            points,
        })
        .collect();
    if !no_value.is_empty() {
        series.push(LineSeries {
            group: None,
            points: no_value,
        });
    }
    series
}

fn compare_axis(left: &ChartValue, right: &ChartValue) -> Ordering {
    match (left, right) {
        (ChartValue::Number(left), ChartValue::Number(right)) => {
            left.partial_cmp(right).unwrap_or(Ordering::Equal)
        }
        (ChartValue::Date(left), ChartValue::Date(right)) => left.cmp(right),
        (ChartValue::Duration(left), ChartValue::Duration(right)) => left.cmp(right),
        // Same-field items are always the same variant; mixed types shouldn't
        // happen in practice but keep ordering total for determinism.
        (ChartValue::Number(_), _) => Ordering::Less,
        (_, ChartValue::Number(_)) => Ordering::Greater,
        (ChartValue::Duration(_), ChartValue::Date(_)) => Ordering::Less,
        (ChartValue::Date(_), ChartValue::Duration(_)) => Ordering::Greater,
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    use crate::model::schema::{FieldTypeConfig, Schema};
    use crate::model::views::{DisplayConfig, View, ViewKind};
    use crate::model::{FieldValue, WorkItem};
    use crate::view_data::test_support::{make_item, make_schema, make_store};

    fn line_chart_view(x: &str, y: &str) -> View {
        line_chart_view_with_group(x, y, None)
    }

    fn line_chart_view_with_group(x: &str, y: &str, group: Option<&str>) -> View {
        View {
            id: "my-line".into(),
            where_clauses: vec![],
            display: DisplayConfig::default(),
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

    /// Flatten the series back into one point list, for assertions that
    /// care about the global x-sort rather than the partition.
    fn all_points(data: &LineChartData) -> Vec<&LinePoint> {
        data.series
            .iter()
            .flat_map(|series| series.points.iter())
            .collect()
    }

    /// The `group` value of each series, in draw order.
    fn series_groups(data: &LineChartData) -> Vec<Option<&str>> {
        data.series
            .iter()
            .map(|series| series.group.as_deref())
            .collect()
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

        let xs: Vec<f64> = all_points(&data)
            .into_iter()
            .map(|point| match point.x {
                ChartValue::Number(number) => number,
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

        let dates: Vec<NaiveDate> = all_points(&data)
            .into_iter()
            .map(|point| match point.x {
                ChartValue::Date(date) => date,
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

        let ids: Vec<&str> = all_points(&data)
            .into_iter()
            .map(|point| point.id.as_str())
            .collect();
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

        assert!(data.series.is_empty());
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

        assert!(data.series.is_empty());
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

    fn point(id: &str, progress: i64, score: f64, team: Option<&str>) -> WorkItem {
        let mut fields = vec![
            ("progress", FieldValue::Integer(progress)),
            ("score", FieldValue::Float(score)),
        ];
        if let Some(team) = team {
            fields.push(("team", FieldValue::Choice(team.into())));
        }
        make_item(id, fields, "")
    }

    #[test]
    fn ungrouped_view_yields_one_series_with_no_group() {
        let schema = grouped_schema();
        let store = make_store(
            &schema,
            vec![
                point("a", 1, 1.0, Some("eng")),
                point("b", 2, 2.0, Some("ops")),
            ],
        );
        let view = line_chart_view("progress", "score");

        let data = extract_line_chart(&view, &store, &schema);

        // No group slot: the team values are ignored and everything lands
        // in the single no-group series. Renderers read `group_field` to
        // decide whether to draw a legend at all.
        assert_eq!(data.group_field, None);
        assert_eq!(series_groups(&data), vec![None]);
        assert_eq!(data.series[0].points.len(), 2);
    }

    #[test]
    fn grouped_view_splits_into_one_series_per_value_ascending() {
        let schema = grouped_schema();
        let store = make_store(
            &schema,
            vec![
                point("c", 3, 3.0, Some("ops")),
                point("a", 1, 1.0, Some("ops")),
                point("b", 2, 2.0, Some("eng")),
            ],
        );
        let view = line_chart_view_with_group("progress", "score", Some("team"));

        let data = extract_line_chart(&view, &store, &schema);

        assert_eq!(data.group_field.as_deref(), Some("team"));
        assert_eq!(series_groups(&data), vec![Some("eng"), Some("ops")]);
        let ops_ids: Vec<&str> = data.series[1]
            .points
            .iter()
            .map(|point| point.id.as_str())
            .collect();
        // Points keep the global x-ascending order inside their series.
        assert_eq!(ops_ids, vec!["a", "c"]);
    }

    #[test]
    fn items_without_a_group_value_land_in_a_final_no_group_series() {
        let schema = grouped_schema();
        let store = make_store(
            &schema,
            vec![
                point("a", 1, 1.0, Some("ops")),
                point("b", 2, 2.0, None),
                point("c", 3, 3.0, Some("eng")),
            ],
        );
        let view = line_chart_view_with_group("progress", "score", Some("team"));

        let data = extract_line_chart(&view, &store, &schema);

        // Synthetic series sorts last regardless of where its label would
        // fall alphabetically — the convention board and gantt also use.
        // It stays a structural `None`; naming it is the renderer's job.
        assert_eq!(series_groups(&data), vec![Some("eng"), Some("ops"), None]);
        let no_group_ids: Vec<&str> = data.series[2]
            .points
            .iter()
            .map(|point| point.id.as_str())
            .collect();
        assert_eq!(no_group_ids, vec!["b"]);
    }

    #[test]
    fn no_group_series_is_absent_when_every_item_has_a_value() {
        let schema = grouped_schema();
        let store = make_store(
            &schema,
            vec![
                point("a", 1, 1.0, Some("eng")),
                point("b", 2, 2.0, Some("ops")),
            ],
        );
        let view = line_chart_view_with_group("progress", "score", Some("team"));

        let data = extract_line_chart(&view, &store, &schema);

        assert_eq!(series_groups(&data), vec![Some("eng"), Some("ops")]);
    }

    #[test]
    fn points_stay_x_sorted_inside_each_series() {
        let schema = grouped_schema();
        let store = make_store(
            &schema,
            vec![
                point("c", 3, 3.0, Some("eng")),
                point("a", 1, 1.0, Some("ops")),
                point("b", 2, 2.0, Some("eng")),
            ],
        );
        let view = line_chart_view_with_group("progress", "score", Some("team"));

        let data = extract_line_chart(&view, &store, &schema);

        // The partition reorders series, never points within a series:
        // eng gets b(x=2) then c(x=3), not file order c then b.
        let eng_ids: Vec<&str> = data.series[0]
            .points
            .iter()
            .map(|point| point.id.as_str())
            .collect();
        assert_eq!(series_groups(&data), vec![Some("eng"), Some("ops")]);
        assert_eq!(eng_ids, vec!["b", "c"]);
    }
}
