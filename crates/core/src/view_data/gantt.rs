//! Gantt view extractor.
//!
//! Emits one bar per filter-matched item with a valid `[start..=end]`
//! window. Bars always carry resolved `(start, end)` `NaiveDate`s
//! regardless of how the view declared them — the view configures one
//! of two input recipes:
//!
//! - **`start + end`**: read both dates directly.
//! - **`start + duration`**: read `start`, read the duration field's
//!   canonical seconds, ceil to whole days, set `end = start + (days - 1)`.
//!   The view is guaranteed to set exactly one of `end` / `duration` by
//!   `views_check`.
//!
//! Inclusive `[start, end]` convention: `start == end` is a 1-day bar,
//! matching Mermaid's two-date task syntax. So `duration = "1d"` yields
//! `end = start` (1-day bar), `"5d"` yields `end = start + 4` (5-day bar).
//!
//! Sub-day durations ceil up: `4h` becomes a 1-day bar. Day-grid Mermaid
//! can't represent sub-day ranges meaningfully; ceil keeps every hour of
//! work visible on the chart.
//!
//! Non-positive durations (`0s`, `-2d`) naturally fall into `InvalidRange`:
//! `ceil_days = 0` makes `end = start - 1`, which the existing
//! `start > end` check catches without special-casing.
//!
//! Items missing a required value land in `unplaced` with a structured
//! reason. The optional `group` slot resolves to a stringified field
//! value per bar; missing `group` values surface as `None`.

use chrono::NaiveDate;
use serde::Serialize;

use crate::model::schema::{FieldTypeConfig, Schema};
use crate::model::views::{View, ViewKind};
use crate::query::format::format_field_value;
use crate::store::Store;

use super::common::{as_date, as_duration_seconds, build_card, Card, UnplacedCard, UnplacedReason};
use super::filter::filtered_items;

/// Seconds per day for ceil-to-days conversion.
const SECONDS_PER_DAY: i64 = 86_400;

#[derive(Debug, Clone, Serialize)]
pub struct GanttData {
    pub group_field: Option<String>,
    pub bars: Vec<GanttBar>,
    pub unplaced: Vec<UnplacedCard>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GanttBar {
    pub card: Card,
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub group: Option<String>,
}

pub fn extract_gantt(view: &View, store: &Store, schema: &Schema) -> GanttData {
    let ViewKind::Gantt {
        start,
        end,
        duration,
        group,
    } = &view.kind
    else {
        panic!("extract_gantt called with non-gantt view kind");
    };
    let items = filtered_items(view, store, schema);

    let mut bars: Vec<GanttBar> = Vec::new();
    let mut unplaced: Vec<UnplacedCard> = Vec::new();

    for item in &items {
        let card = build_card(item, schema, view);
        let Some(start_date) = as_date(item.fields.get(start)) else {
            unplaced.push(UnplacedCard {
                card,
                reason: UnplacedReason::MissingValue {
                    field: start.clone(),
                },
            });
            continue;
        };

        let resolved_end = match (end, duration) {
            (Some(end_field), None) => match as_date(item.fields.get(end_field)) {
                Some(end_date) => Ok(end_date),
                None => Err(UnplacedReason::MissingValue {
                    field: end_field.clone(),
                }),
            },
            (None, Some(duration_field)) => {
                match as_duration_seconds(item.fields.get(duration_field)) {
                    Some(seconds) => match end_from_duration(start_date, seconds) {
                        Some(end_date) => Ok(end_date),
                        None => Err(UnplacedReason::InvalidRange {
                            start_field: start.clone(),
                            end_field: duration_field.clone(),
                        }),
                    },
                    None => Err(UnplacedReason::MissingValue {
                        field: duration_field.clone(),
                    }),
                }
            }
            // views_check guarantees exactly one of (end, duration).
            _ => unreachable!("views_check ensures exactly one of end / duration"),
        };

        match resolved_end {
            Ok(end_date) if start_date > end_date => {
                let end_field_name = end.as_ref().or(duration.as_ref()).cloned().unwrap();
                unplaced.push(UnplacedCard {
                    card,
                    reason: UnplacedReason::InvalidRange {
                        start_field: start.clone(),
                        end_field: end_field_name,
                    },
                });
            }
            Ok(end_date) => {
                let group_value = group
                    .as_deref()
                    .and_then(|name| item.fields.get(name).map(format_field_value));
                bars.push(GanttBar {
                    card,
                    start: start_date,
                    end: end_date,
                    group: group_value,
                });
            }
            Err(reason) => unplaced.push(UnplacedCard { card, reason }),
        }
    }

    let section_order = section_order(group.as_deref(), schema, &bars);
    bars.sort_by(|left, right| {
        let li = section_index(&left.group, &section_order);
        let ri = section_index(&right.group, &section_order);
        (li, left.start, left.card.id.as_str()).cmp(&(ri, right.start, right.card.id.as_str()))
    });
    unplaced.sort_by(|left, right| left.card.id.as_str().cmp(right.card.id.as_str()));

    GanttData {
        group_field: group.clone(),
        bars,
        unplaced,
    }
}

/// Compute the inclusive end date from a start date and a duration in
/// canonical seconds. Returns `None` for non-positive durations or
/// chrono date overflow — both routed to `InvalidRange` by the caller.
///
/// Sub-day durations ceil up: `4h` → 1 day → `end = start`. Whole-day
/// durations: `1d` → 1 day → `end = start`; `5d` → 5 days → `end = start + 4`.
fn end_from_duration(start: NaiveDate, seconds: i64) -> Option<NaiveDate> {
    if seconds <= 0 {
        return None;
    }
    // Ceil division: (s + 86399) / 86400. Pre-checked positive, so no
    // sign edge cases and the worst case `i64::MAX + 86399` overflows
    // i64 — guard with i128 arithmetic.
    let ceil_days = ((seconds as i128 + (SECONDS_PER_DAY as i128 - 1)) / SECONDS_PER_DAY as i128)
        .min(u64::MAX as i128) as u64;
    // Inclusive end: a 1-day bar has end == start, so we add ceil_days - 1.
    start.checked_add_days(chrono::Days::new(ceil_days.saturating_sub(1)))
}

/// Determine the ordered list of section labels for the bars.
///
/// `Choice` fields use their schema-declared `values:` list — preserving
/// the order users intend to read columns in. Every other accepted group
/// type (`String`, `List`, `Multichoice`, `Link`, `Links`) falls back to
/// alphabetical order of the distinct group strings actually present on
/// the bars; the schema doesn't carry a meaningful declared order for
/// those. Bars whose group value is missing get `usize::MAX` and end up
/// in the synthetic last section regardless of this list.
fn section_order(group: Option<&str>, schema: &Schema, bars: &[GanttBar]) -> Vec<String> {
    let field_def = group.and_then(|name| schema.fields.get(name));
    if let Some(FieldTypeConfig::Choice { values }) = field_def.map(|d| &d.type_config) {
        return values.clone();
    }
    let mut distinct: Vec<String> = bars.iter().filter_map(|b| b.group.clone()).collect();
    distinct.sort();
    distinct.dedup();
    distinct
}

fn section_index(group_value: &Option<String>, section_order: &[String]) -> usize {
    match group_value {
        Some(value) => section_order
            .iter()
            .position(|s| s == value)
            .unwrap_or(usize::MAX),
        None => usize::MAX,
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

    fn gantt_view(start: &str, end: &str, group: Option<&str>) -> View {
        View {
            id: "my-gantt".into(),
            where_clauses: vec![],
            title: None,
            kind: ViewKind::Gantt {
                start: start.to_owned(),
                end: Some(end.to_owned()),
                duration: None,
                group: group.map(str::to_owned),
            },
        }
    }

    fn gantt_view_duration(start: &str, duration: &str, group: Option<&str>) -> View {
        View {
            id: "my-gantt".into(),
            where_clauses: vec![],
            title: None,
            kind: ViewKind::Gantt {
                start: start.to_owned(),
                end: None,
                duration: Some(duration.to_owned()),
                group: group.map(str::to_owned),
            },
        }
    }

    fn date_schema() -> Schema {
        make_schema(vec![
            ("start", FieldTypeConfig::Date),
            ("end", FieldTypeConfig::Date),
            (
                "team",
                FieldTypeConfig::Choice {
                    values: vec!["eng".into(), "ops".into()],
                },
            ),
        ])
    }

    fn ymd(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    #[test]
    fn valid_bar_placed_with_typed_dates() {
        let schema = date_schema();
        let store = make_store(
            &schema,
            vec![make_item(
                "a",
                vec![
                    ("start", FieldValue::Date(ymd(2026, 1, 1))),
                    ("end", FieldValue::Date(ymd(2026, 1, 5))),
                ],
                "",
            )],
        );
        let view = gantt_view("start", "end", None);

        let data = extract_gantt(&view, &store, &schema);

        assert_eq!(data.bars.len(), 1);
        assert_eq!(data.bars[0].card.id.as_str(), "a");
        assert_eq!(data.bars[0].start, ymd(2026, 1, 1));
        assert_eq!(data.bars[0].end, ymd(2026, 1, 5));
        assert!(data.unplaced.is_empty());
    }

    #[test]
    fn start_equals_end_emits_bar() {
        let schema = date_schema();
        let day = ymd(2026, 3, 15);
        let store = make_store(
            &schema,
            vec![make_item(
                "a",
                vec![
                    ("start", FieldValue::Date(day)),
                    ("end", FieldValue::Date(day)),
                ],
                "",
            )],
        );
        let view = gantt_view("start", "end", None);

        let data = extract_gantt(&view, &store, &schema);

        assert_eq!(data.bars.len(), 1);
        assert_eq!(data.bars[0].start, day);
        assert_eq!(data.bars[0].end, day);
    }

    #[test]
    fn missing_start_is_unplaced_missing_value() {
        let schema = date_schema();
        let store = make_store(
            &schema,
            vec![make_item(
                "a",
                vec![("end", FieldValue::Date(ymd(2026, 1, 5)))],
                "",
            )],
        );
        let view = gantt_view("start", "end", None);

        let data = extract_gantt(&view, &store, &schema);

        assert!(data.bars.is_empty());
        assert_eq!(data.unplaced.len(), 1);
        match &data.unplaced[0].reason {
            UnplacedReason::MissingValue { field } => assert_eq!(field, "start"),
            other => panic!("expected MissingValue, got {other:?}"),
        }
    }

    #[test]
    fn missing_end_is_unplaced_missing_value() {
        let schema = date_schema();
        let store = make_store(
            &schema,
            vec![make_item(
                "a",
                vec![("start", FieldValue::Date(ymd(2026, 1, 1)))],
                "",
            )],
        );
        let view = gantt_view("start", "end", None);

        let data = extract_gantt(&view, &store, &schema);

        assert_eq!(data.unplaced.len(), 1);
        match &data.unplaced[0].reason {
            UnplacedReason::MissingValue { field } => assert_eq!(field, "end"),
            other => panic!("expected MissingValue, got {other:?}"),
        }
    }

    #[test]
    fn start_after_end_is_unplaced_invalid_range() {
        let schema = date_schema();
        let store = make_store(
            &schema,
            vec![make_item(
                "a",
                vec![
                    ("start", FieldValue::Date(ymd(2026, 1, 10))),
                    ("end", FieldValue::Date(ymd(2026, 1, 5))),
                ],
                "",
            )],
        );
        let view = gantt_view("start", "end", None);

        let data = extract_gantt(&view, &store, &schema);

        assert!(data.bars.is_empty());
        match &data.unplaced[0].reason {
            UnplacedReason::InvalidRange {
                start_field,
                end_field,
            } => {
                assert_eq!(start_field, "start");
                assert_eq!(end_field, "end");
            }
            other => panic!("expected InvalidRange, got {other:?}"),
        }
    }

    #[test]
    fn group_slot_stringifies_field_value() {
        let schema = date_schema();
        let store = make_store(
            &schema,
            vec![make_item(
                "a",
                vec![
                    ("start", FieldValue::Date(ymd(2026, 1, 1))),
                    ("end", FieldValue::Date(ymd(2026, 1, 5))),
                    ("team", FieldValue::Choice("eng".into())),
                ],
                "",
            )],
        );
        let view = gantt_view("start", "end", Some("team"));

        let data = extract_gantt(&view, &store, &schema);

        assert_eq!(data.bars[0].group.as_deref(), Some("eng"));
        assert_eq!(data.group_field.as_deref(), Some("team"));
    }

    #[test]
    fn group_slot_missing_value_produces_none() {
        let schema = date_schema();
        let store = make_store(
            &schema,
            vec![make_item(
                "a",
                vec![
                    ("start", FieldValue::Date(ymd(2026, 1, 1))),
                    ("end", FieldValue::Date(ymd(2026, 1, 5))),
                ],
                "",
            )],
        );
        let view = gantt_view("start", "end", Some("team"));

        let data = extract_gantt(&view, &store, &schema);

        assert_eq!(data.bars[0].group, None);
    }

    #[test]
    fn bars_sorted_by_start_then_id_when_no_group() {
        let schema = date_schema();
        let early = ymd(2026, 1, 1);
        let late = ymd(2026, 1, 10);
        let end = ymd(2026, 2, 1);
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "c",
                    vec![
                        ("start", FieldValue::Date(late)),
                        ("end", FieldValue::Date(end)),
                    ],
                    "",
                ),
                make_item(
                    "a",
                    vec![
                        ("start", FieldValue::Date(early)),
                        ("end", FieldValue::Date(end)),
                    ],
                    "",
                ),
                make_item(
                    "b",
                    vec![
                        ("start", FieldValue::Date(early)),
                        ("end", FieldValue::Date(end)),
                    ],
                    "",
                ),
            ],
        );
        let view = gantt_view("start", "end", None);

        let data = extract_gantt(&view, &store, &schema);

        let bar_ids: Vec<&str> = data.bars.iter().map(|b| b.card.id.as_str()).collect();
        // a/b share the early start so id breaks the tie; c is later.
        assert_eq!(bar_ids, vec!["a", "b", "c"]);
    }

    #[test]
    fn unplaced_sorted_by_id() {
        let schema = date_schema();
        let store = make_store(
            &schema,
            vec![make_item("z", vec![], ""), make_item("m", vec![], "")],
        );
        let view = gantt_view("start", "end", None);

        let data = extract_gantt(&view, &store, &schema);

        let unplaced_ids: Vec<&str> = data.unplaced.iter().map(|u| u.card.id.as_str()).collect();
        assert_eq!(unplaced_ids, vec!["m", "z"]);
    }

    #[test]
    fn sections_follow_schema_declared_order_for_choice() {
        let schema = make_schema(vec![
            ("start", FieldTypeConfig::Date),
            ("end", FieldTypeConfig::Date),
            (
                "team",
                FieldTypeConfig::Choice {
                    values: vec!["ops".into(), "eng".into(), "design".into()],
                },
            ),
        ]);
        let d1 = ymd(2026, 1, 1);
        let d2 = ymd(2026, 1, 5);
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "a",
                    vec![
                        ("start", FieldValue::Date(d1)),
                        ("end", FieldValue::Date(d2)),
                        ("team", FieldValue::Choice("eng".into())),
                    ],
                    "",
                ),
                make_item(
                    "b",
                    vec![
                        ("start", FieldValue::Date(d1)),
                        ("end", FieldValue::Date(d2)),
                        ("team", FieldValue::Choice("design".into())),
                    ],
                    "",
                ),
                make_item(
                    "c",
                    vec![
                        ("start", FieldValue::Date(d1)),
                        ("end", FieldValue::Date(d2)),
                        ("team", FieldValue::Choice("ops".into())),
                    ],
                    "",
                ),
            ],
        );
        let view = gantt_view("start", "end", Some("team"));

        let data = extract_gantt(&view, &store, &schema);

        let groups: Vec<&str> = data
            .bars
            .iter()
            .map(|b| b.group.as_deref().unwrap())
            .collect();
        assert_eq!(groups, vec!["ops", "eng", "design"]);
    }

    #[test]
    fn sections_alphabetical_for_string_group() {
        let schema = make_schema(vec![
            ("start", FieldTypeConfig::Date),
            ("end", FieldTypeConfig::Date),
            ("squad", FieldTypeConfig::String { pattern: None }),
        ]);
        let d1 = ymd(2026, 1, 1);
        let d2 = ymd(2026, 1, 5);
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "a",
                    vec![
                        ("start", FieldValue::Date(d1)),
                        ("end", FieldValue::Date(d2)),
                        ("squad", FieldValue::String("zeta".into())),
                    ],
                    "",
                ),
                make_item(
                    "b",
                    vec![
                        ("start", FieldValue::Date(d1)),
                        ("end", FieldValue::Date(d2)),
                        ("squad", FieldValue::String("alpha".into())),
                    ],
                    "",
                ),
            ],
        );
        let view = gantt_view("start", "end", Some("squad"));

        let data = extract_gantt(&view, &store, &schema);

        let groups: Vec<&str> = data
            .bars
            .iter()
            .map(|b| b.group.as_deref().unwrap())
            .collect();
        assert_eq!(groups, vec!["alpha", "zeta"]);
    }

    #[test]
    fn link_group_value_is_target_id() {
        let schema = make_schema(vec![
            ("start", FieldTypeConfig::Date),
            ("end", FieldTypeConfig::Date),
            (
                "parent",
                FieldTypeConfig::Link {
                    allow_cycles: Some(false),
                    inverse: None,
                },
            ),
        ]);
        let d1 = ymd(2026, 1, 1);
        let d2 = ymd(2026, 1, 5);
        let store = make_store(
            &schema,
            vec![make_item(
                "a",
                vec![
                    ("start", FieldValue::Date(d1)),
                    ("end", FieldValue::Date(d2)),
                    (
                        "parent",
                        FieldValue::Link(crate::model::WorkItemId::from("epic-x".to_owned())),
                    ),
                ],
                "",
            )],
        );
        let view = gantt_view("start", "end", Some("parent"));

        let data = extract_gantt(&view, &store, &schema);

        assert_eq!(data.bars[0].group.as_deref(), Some("epic-x"));
    }

    #[test]
    fn missing_group_value_sorts_last() {
        let schema = date_schema();
        let d1 = ymd(2026, 1, 1);
        let d2 = ymd(2026, 1, 5);
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "without-team",
                    vec![
                        ("start", FieldValue::Date(d1)),
                        ("end", FieldValue::Date(d2)),
                    ],
                    "",
                ),
                make_item(
                    "with-team",
                    vec![
                        ("start", FieldValue::Date(d1)),
                        ("end", FieldValue::Date(d2)),
                        ("team", FieldValue::Choice("eng".into())),
                    ],
                    "",
                ),
            ],
        );
        let view = gantt_view("start", "end", Some("team"));

        let data = extract_gantt(&view, &store, &schema);

        let ids: Vec<&str> = data.bars.iter().map(|b| b.card.id.as_str()).collect();
        assert_eq!(ids, vec!["with-team", "without-team"]);
        assert_eq!(data.bars[0].group.as_deref(), Some("eng"));
        assert_eq!(data.bars[1].group, None);
    }

    // ── Duration mode ────────────────────────────────────────────────

    fn duration_schema() -> Schema {
        make_schema(vec![
            ("start", FieldTypeConfig::Date),
            (
                "estimate",
                FieldTypeConfig::Duration {
                    min: None,
                    max: None,
                },
            ),
        ])
    }

    fn duration_store(id: &str, start: NaiveDate, seconds: i64) -> (Schema, crate::store::Store) {
        let schema = duration_schema();
        let store = make_store(
            &schema,
            vec![make_item(
                id,
                vec![
                    ("start", FieldValue::Date(start)),
                    ("estimate", FieldValue::Duration(seconds)),
                ],
                "",
            )],
        );
        (schema, store)
    }

    #[test]
    fn duration_mode_full_day_value() {
        // 5d → 5 days → end = start + 4 (inclusive convention)
        let start = ymd(2026, 1, 1);
        let (schema, store) = duration_store("a", start, 5 * 86_400);
        let view = gantt_view_duration("start", "estimate", None);

        let data = extract_gantt(&view, &store, &schema);

        assert_eq!(data.bars.len(), 1);
        assert_eq!(data.bars[0].start, start);
        assert_eq!(data.bars[0].end, ymd(2026, 1, 5));
        assert!(data.unplaced.is_empty());
    }

    #[test]
    fn duration_mode_sub_day_ceils_to_one_day() {
        // 4h → ceil to 1 day → end = start (1-day bar)
        let start = ymd(2026, 1, 1);
        let (schema, store) = duration_store("a", start, 4 * 3_600);
        let view = gantt_view_duration("start", "estimate", None);

        let data = extract_gantt(&view, &store, &schema);

        assert_eq!(data.bars.len(), 1);
        assert_eq!(data.bars[0].start, start);
        assert_eq!(data.bars[0].end, start);
    }

    #[test]
    fn duration_mode_compound_value() {
        // 2w 3d = 17 days → end = start + 16
        let start = ymd(2026, 1, 1);
        let seconds = (2 * 7 + 3) * 86_400;
        let (schema, store) = duration_store("a", start, seconds);
        let view = gantt_view_duration("start", "estimate", None);

        let data = extract_gantt(&view, &store, &schema);

        assert_eq!(data.bars.len(), 1);
        assert_eq!(data.bars[0].end, ymd(2026, 1, 17));
    }

    #[test]
    fn duration_mode_one_day_exactly_yields_one_day_bar() {
        // 1d → ceil to 1 day → end = start
        let start = ymd(2026, 1, 1);
        let (schema, store) = duration_store("a", start, 86_400);
        let view = gantt_view_duration("start", "estimate", None);

        let data = extract_gantt(&view, &store, &schema);

        assert_eq!(data.bars[0].start, start);
        assert_eq!(data.bars[0].end, start);
    }

    #[test]
    fn duration_mode_one_second_over_one_day_yields_two_day_bar() {
        // 1d + 1s → ceil to 2 days → end = start + 1
        let start = ymd(2026, 1, 1);
        let (schema, store) = duration_store("a", start, 86_401);
        let view = gantt_view_duration("start", "estimate", None);

        let data = extract_gantt(&view, &store, &schema);

        assert_eq!(data.bars[0].end, ymd(2026, 1, 2));
    }

    #[test]
    fn duration_mode_missing_duration_value_unplaces() {
        let schema = duration_schema();
        let store = make_store(
            &schema,
            vec![make_item(
                "a",
                vec![("start", FieldValue::Date(ymd(2026, 1, 1)))],
                "",
            )],
        );
        let view = gantt_view_duration("start", "estimate", None);

        let data = extract_gantt(&view, &store, &schema);

        assert!(data.bars.is_empty());
        match &data.unplaced[0].reason {
            UnplacedReason::MissingValue { field } => assert_eq!(field, "estimate"),
            other => panic!("expected MissingValue for duration, got {other:?}"),
        }
    }

    #[test]
    fn duration_mode_missing_start_unplaces_with_start_field() {
        let schema = duration_schema();
        let store = make_store(
            &schema,
            vec![make_item(
                "a",
                vec![("estimate", FieldValue::Duration(86_400))],
                "",
            )],
        );
        let view = gantt_view_duration("start", "estimate", None);

        let data = extract_gantt(&view, &store, &schema);

        match &data.unplaced[0].reason {
            UnplacedReason::MissingValue { field } => assert_eq!(field, "start"),
            other => panic!("expected MissingValue for start, got {other:?}"),
        }
    }

    #[test]
    fn duration_mode_zero_duration_unplaces_invalid_range() {
        let start = ymd(2026, 1, 1);
        let (schema, store) = duration_store("a", start, 0);
        let view = gantt_view_duration("start", "estimate", None);

        let data = extract_gantt(&view, &store, &schema);

        assert!(data.bars.is_empty());
        match &data.unplaced[0].reason {
            UnplacedReason::InvalidRange { .. } => {}
            other => panic!("expected InvalidRange, got {other:?}"),
        }
    }

    #[test]
    fn duration_mode_negative_duration_unplaces_invalid_range() {
        let start = ymd(2026, 1, 1);
        let (schema, store) = duration_store("a", start, -2 * 86_400);
        let view = gantt_view_duration("start", "estimate", None);

        let data = extract_gantt(&view, &store, &schema);

        assert!(data.bars.is_empty());
        match &data.unplaced[0].reason {
            UnplacedReason::InvalidRange { .. } => {}
            other => panic!("expected InvalidRange, got {other:?}"),
        }
    }

    #[test]
    fn duration_mode_equivalent_to_end_mode() {
        // Two views over the same store: one with explicit end_date,
        // one with duration. Expect identical bar windows.
        let start = ymd(2026, 1, 1);
        let end = ymd(2026, 1, 5);
        let dur_seconds = 5 * 86_400;

        let schema = make_schema(vec![
            ("start", FieldTypeConfig::Date),
            ("end", FieldTypeConfig::Date),
            (
                "estimate",
                FieldTypeConfig::Duration {
                    min: None,
                    max: None,
                },
            ),
        ]);
        let store = make_store(
            &schema,
            vec![make_item(
                "a",
                vec![
                    ("start", FieldValue::Date(start)),
                    ("end", FieldValue::Date(end)),
                    ("estimate", FieldValue::Duration(dur_seconds)),
                ],
                "",
            )],
        );

        let end_view = gantt_view("start", "end", None);
        let dur_view = gantt_view_duration("start", "estimate", None);

        let end_data = extract_gantt(&end_view, &store, &schema);
        let dur_data = extract_gantt(&dur_view, &store, &schema);

        // Structural equivalence: same number of bars, same windows.
        assert_eq!(end_data.bars.len(), dur_data.bars.len());
        for (a, b) in end_data.bars.iter().zip(dur_data.bars.iter()) {
            assert_eq!(a.card.id.as_str(), b.card.id.as_str());
            assert_eq!(a.start, b.start);
            assert_eq!(a.end, b.end);
            assert_eq!(a.group, b.group);
        }
        assert!(end_data.unplaced.is_empty());
        assert!(dur_data.unplaced.is_empty());
    }
}
