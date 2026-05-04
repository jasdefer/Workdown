//! Workload view extractor.
//!
//! For each filter-matched item with valid `start`, `end`, and numeric
//! or duration `effort`, spreads `effort / working_day_count` across
//! each working day in `[start..=end]` and sums the contributions into
//! dense daily buckets. Non-working days never produce buckets — the
//! resulting time axis only shows the days that actually carry work.
//!
//! The active calendar is the per-view `working_days` override when
//! set, otherwise the project-level calendar passed in by the caller
//! (which itself defaults to Monday–Friday in `Config::working_calendar`).
//!
//! Items with missing/invalid data land in `unplaced`:
//! - `MissingValue` for an absent `start`, `end`, or `effort`
//! - `InvalidRange` when `start > end`
//! - `NoWorkingDays` when the interval is non-empty but every day in
//!   it falls on a non-working day per the active calendar
//!
//! Bucket totals are stored as `f64`. `WorkloadData::unit` carries the
//! interpretation: `Number` (raw f64) or `Duration` (canonical seconds
//! as f64). Renderers format accordingly.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde::Serialize;

use crate::model::calendar::WorkingCalendar;
use crate::model::schema::{FieldType, Schema};
use crate::model::views::{View, ViewKind};
use crate::store::Store;

use super::common::{as_date, as_size, build_card, UnplacedCard, UnplacedReason};
use super::filter::filtered_items;

/// How a [`WorkloadBucket::total`] should be interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkloadUnit {
    /// Raw numeric value — sum of integer/float effort fields.
    Number,
    /// Canonical seconds — sum of duration effort fields. Renderers pick
    /// a display unit (hours, days, …) per chart.
    Duration,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkloadData {
    pub start_field: String,
    pub end_field: String,
    pub effort_field: String,
    pub unit: WorkloadUnit,
    /// One bucket per working day in `[min(working_day) ..= max(working_day)]`
    /// across all placed items. Days not in the active working calendar
    /// never appear.
    pub buckets: Vec<WorkloadBucket>,
    pub unplaced: Vec<UnplacedCard>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkloadBucket {
    pub date: NaiveDate,
    pub total: f64,
}

pub fn extract_workload(
    view: &View,
    store: &Store,
    schema: &Schema,
    config_calendar: &WorkingCalendar,
) -> WorkloadData {
    let ViewKind::Workload {
        start,
        end,
        effort,
        working_days,
    } = &view.kind
    else {
        panic!("extract_workload called with non-workload view kind");
    };
    let calendar = match working_days {
        Some(days) => WorkingCalendar::from_days(days.iter().copied()),
        None => config_calendar.clone(),
    };
    let unit = unit_for_effort_field(effort, schema);
    let items = filtered_items(view, store, schema);

    let mut totals: BTreeMap<NaiveDate, f64> = BTreeMap::new();
    let mut unplaced: Vec<UnplacedCard> = Vec::new();

    for item in &items {
        let card = build_card(item, schema, view);
        let start_date = as_date(item.fields.get(start));
        let end_date = as_date(item.fields.get(end));
        // `as_size` returns the value with its variant preserved; we
        // only care about the magnitude here, so collapse to f64. The
        // variant has already been captured at the data level via `unit`.
        let effort_value = as_size(item.fields.get(effort)).map(|size| size.as_f64());

        let (start_date, end_date, effort_value) = match (start_date, end_date, effort_value) {
            (Some(start_date), Some(end_date), Some(effort_value)) => {
                (start_date, end_date, effort_value)
            }
            (None, _, _) => {
                unplaced.push(UnplacedCard {
                    card,
                    reason: UnplacedReason::MissingValue {
                        field: start.clone(),
                    },
                });
                continue;
            }
            (_, None, _) => {
                unplaced.push(UnplacedCard {
                    card,
                    reason: UnplacedReason::MissingValue { field: end.clone() },
                });
                continue;
            }
            (_, _, None) => {
                unplaced.push(UnplacedCard {
                    card,
                    reason: UnplacedReason::MissingValue {
                        field: effort.clone(),
                    },
                });
                continue;
            }
        };

        if start_date > end_date {
            unplaced.push(UnplacedCard {
                card,
                reason: UnplacedReason::InvalidRange {
                    start_field: start.clone(),
                    end_field: end.clone(),
                },
            });
            continue;
        }

        let working_days_in_range: Vec<NaiveDate> =
            collect_working_days(start_date, end_date, &calendar);
        if working_days_in_range.is_empty() {
            unplaced.push(UnplacedCard {
                card,
                reason: UnplacedReason::NoWorkingDays {
                    start_field: start.clone(),
                    end_field: end.clone(),
                },
            });
            continue;
        }

        let per_day = effort_value / (working_days_in_range.len() as f64);
        for day in working_days_in_range {
            *totals.entry(day).or_insert(0.0) += per_day;
        }
    }

    let buckets = dense_working_buckets(&totals, &calendar);

    unplaced.sort_by(|left, right| left.card.id.as_str().cmp(right.card.id.as_str()));

    WorkloadData {
        start_field: start.clone(),
        end_field: end.clone(),
        effort_field: effort.clone(),
        unit,
        buckets,
        unplaced,
    }
}

/// Pick the [`WorkloadUnit`] from the effort field's schema type.
///
/// `views_check` guarantees the field resolves to integer, float, or
/// duration; an unexpected type falls back to `Number` defensively
/// rather than panicking — the bucket totals will read as raw f64s
/// either way.
fn unit_for_effort_field(field: &str, schema: &Schema) -> WorkloadUnit {
    match schema.fields.get(field).map(|config| config.field_type()) {
        Some(FieldType::Duration) => WorkloadUnit::Duration,
        _ => WorkloadUnit::Number,
    }
}

/// Walk `[start..=end]` and collect each date that the active calendar
/// classifies as a working day. Order-preserving (chronological).
fn collect_working_days(
    start: NaiveDate,
    end: NaiveDate,
    calendar: &WorkingCalendar,
) -> Vec<NaiveDate> {
    let mut days = Vec::new();
    let mut day = start;
    while day <= end {
        if calendar.is_working(day) {
            days.push(day);
        }
        day = day.succ_opt().expect("date within valid chrono range");
    }
    days
}

/// Build the dense bucket list spanning min..=max of contributed days,
/// emitting one bucket per working day. Non-working days inside that
/// range are skipped so the bar chart's x-axis only shows day-of-work
/// ticks. Within-stretch zero buckets (a working day that no item
/// touched) are emitted as `total = 0.0`, so the renderer sees an
/// uninterrupted working-day series.
fn dense_working_buckets(
    totals: &BTreeMap<NaiveDate, f64>,
    calendar: &WorkingCalendar,
) -> Vec<WorkloadBucket> {
    let Some((&min, _)) = totals.iter().next() else {
        return Vec::new();
    };
    let Some((&max, _)) = totals.iter().next_back() else {
        return Vec::new();
    };

    let mut buckets = Vec::new();
    let mut day = min;
    while day <= max {
        if calendar.is_working(day) {
            let total = totals.get(&day).copied().unwrap_or(0.0);
            buckets.push(WorkloadBucket { date: day, total });
        }
        day = day.succ_opt().expect("date within valid chrono range");
    }
    buckets
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    use crate::model::schema::{FieldTypeConfig, Schema};
    use crate::model::views::{View, ViewKind};
    use crate::model::weekday::Weekday;
    use crate::model::FieldValue;
    use crate::view_data::test_support::{make_item, make_schema, make_store};

    fn workload_view(start: &str, end: &str, effort: &str) -> View {
        View {
            id: "my-workload".into(),
            where_clauses: vec![],
            title: None,
            kind: ViewKind::Workload {
                start: start.to_owned(),
                end: end.to_owned(),
                effort: effort.to_owned(),
                working_days: None,
            },
        }
    }

    fn workload_view_with_days(start: &str, end: &str, effort: &str, days: Vec<Weekday>) -> View {
        View {
            id: "my-workload".into(),
            where_clauses: vec![],
            title: None,
            kind: ViewKind::Workload {
                start: start.to_owned(),
                end: end.to_owned(),
                effort: effort.to_owned(),
                working_days: Some(days),
            },
        }
    }

    fn workload_schema() -> Schema {
        make_schema(vec![
            ("start", FieldTypeConfig::Date),
            ("end", FieldTypeConfig::Date),
            (
                "effort",
                FieldTypeConfig::Integer {
                    min: None,
                    max: None,
                },
            ),
        ])
    }

    fn business_week() -> WorkingCalendar {
        WorkingCalendar::default_business_week()
    }

    fn ymd(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    fn close_enough(left: f64, right: f64) -> bool {
        (left - right).abs() < 1e-9
    }

    // 2026-01-05 is a Monday. Most tests anchor to that week so the
    // weekend exclusion is unambiguous.

    #[test]
    fn single_item_effort_spread_across_working_days() {
        let schema = workload_schema();
        let store = make_store(
            &schema,
            // Mon Jan 5 → Thu Jan 8: 4 working days.
            vec![make_item(
                "a",
                vec![
                    ("start", FieldValue::Date(ymd(2026, 1, 5))),
                    ("end", FieldValue::Date(ymd(2026, 1, 8))),
                    ("effort", FieldValue::Integer(8)),
                ],
                "",
            )],
        );
        let view = workload_view("start", "end", "effort");

        let data = extract_workload(&view, &store, &schema, &business_week());

        assert_eq!(data.buckets.len(), 4);
        for bucket in &data.buckets {
            assert!(close_enough(bucket.total, 2.0));
        }
        assert_eq!(data.unit, WorkloadUnit::Number);
        assert!(data.unplaced.is_empty());
    }

    #[test]
    fn weekends_skipped_under_default_calendar() {
        let schema = workload_schema();
        let store = make_store(
            &schema,
            // Mon Jan 5 → Sun Jan 11: 7 calendar days, 5 working.
            vec![make_item(
                "a",
                vec![
                    ("start", FieldValue::Date(ymd(2026, 1, 5))),
                    ("end", FieldValue::Date(ymd(2026, 1, 11))),
                    ("effort", FieldValue::Integer(10)),
                ],
                "",
            )],
        );
        let view = workload_view("start", "end", "effort");

        let data = extract_workload(&view, &store, &schema, &business_week());

        // 5 working days, 10/5 = 2.0 each, no Sat/Sun buckets.
        assert_eq!(data.buckets.len(), 5);
        for bucket in &data.buckets {
            assert!(close_enough(bucket.total, 2.0));
            // None of the buckets should be Saturday or Sunday.
            let weekday = bucket.date.format("%A").to_string();
            assert_ne!(weekday, "Saturday");
            assert_ne!(weekday, "Sunday");
        }
    }

    #[test]
    fn all_seven_days_calendar_matches_uniform_distribution() {
        let schema = workload_schema();
        let store = make_store(
            &schema,
            // Mon Jan 5 → Sun Jan 11.
            vec![make_item(
                "a",
                vec![
                    ("start", FieldValue::Date(ymd(2026, 1, 5))),
                    ("end", FieldValue::Date(ymd(2026, 1, 11))),
                    ("effort", FieldValue::Integer(7)),
                ],
                "",
            )],
        );
        let view = workload_view("start", "end", "effort");
        let all_days = WorkingCalendar::from_days([
            Weekday::Monday,
            Weekday::Tuesday,
            Weekday::Wednesday,
            Weekday::Thursday,
            Weekday::Friday,
            Weekday::Saturday,
            Weekday::Sunday,
        ]);

        let data = extract_workload(&view, &store, &schema, &all_days);

        assert_eq!(data.buckets.len(), 7);
        for bucket in &data.buckets {
            assert!(close_enough(bucket.total, 1.0));
        }
    }

    #[test]
    fn overlapping_items_sum_in_shared_working_days() {
        let schema = workload_schema();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "a",
                    vec![
                        // Mon Jan 5 → Tue Jan 6: 2 working days.
                        ("start", FieldValue::Date(ymd(2026, 1, 5))),
                        ("end", FieldValue::Date(ymd(2026, 1, 6))),
                        ("effort", FieldValue::Integer(4)),
                    ],
                    "",
                ),
                make_item(
                    "b",
                    vec![
                        // Tue Jan 6 → Wed Jan 7: 2 working days.
                        ("start", FieldValue::Date(ymd(2026, 1, 6))),
                        ("end", FieldValue::Date(ymd(2026, 1, 7))),
                        ("effort", FieldValue::Integer(2)),
                    ],
                    "",
                ),
            ],
        );
        let view = workload_view("start", "end", "effort");

        let data = extract_workload(&view, &store, &schema, &business_week());

        // Mon: a=2, Tue: a=2 + b=1 = 3, Wed: b=1.
        assert_eq!(data.buckets.len(), 3);
        assert!(close_enough(data.buckets[0].total, 2.0));
        assert!(close_enough(data.buckets[1].total, 3.0));
        assert!(close_enough(data.buckets[2].total, 1.0));
    }

    #[test]
    fn gap_working_days_appear_as_zero_buckets() {
        let schema = workload_schema();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "a",
                    vec![
                        // Mon Jan 5.
                        ("start", FieldValue::Date(ymd(2026, 1, 5))),
                        ("end", FieldValue::Date(ymd(2026, 1, 5))),
                        ("effort", FieldValue::Integer(3)),
                    ],
                    "",
                ),
                make_item(
                    "b",
                    vec![
                        // Thu Jan 8.
                        ("start", FieldValue::Date(ymd(2026, 1, 8))),
                        ("end", FieldValue::Date(ymd(2026, 1, 8))),
                        ("effort", FieldValue::Integer(1)),
                    ],
                    "",
                ),
            ],
        );
        let view = workload_view("start", "end", "effort");

        let data = extract_workload(&view, &store, &schema, &business_week());

        // Mon=3, Tue=0, Wed=0, Thu=1 — 4 buckets, no weekends in range.
        assert_eq!(data.buckets.len(), 4);
        assert!(close_enough(data.buckets[0].total, 3.0));
        assert!(close_enough(data.buckets[1].total, 0.0));
        assert!(close_enough(data.buckets[2].total, 0.0));
        assert!(close_enough(data.buckets[3].total, 1.0));
    }

    #[test]
    fn bridging_weekend_skips_saturday_and_sunday() {
        let schema = workload_schema();
        let store = make_store(
            &schema,
            // Fri Jan 9 → Mon Jan 12: 4 calendar days, 2 working.
            vec![make_item(
                "a",
                vec![
                    ("start", FieldValue::Date(ymd(2026, 1, 9))),
                    ("end", FieldValue::Date(ymd(2026, 1, 12))),
                    ("effort", FieldValue::Integer(8)),
                ],
                "",
            )],
        );
        let view = workload_view("start", "end", "effort");

        let data = extract_workload(&view, &store, &schema, &business_week());

        // Fri + Mon, no Sat/Sun.
        assert_eq!(data.buckets.len(), 2);
        assert_eq!(data.buckets[0].date, ymd(2026, 1, 9));
        assert_eq!(data.buckets[1].date, ymd(2026, 1, 12));
        assert!(close_enough(data.buckets[0].total, 4.0));
        assert!(close_enough(data.buckets[1].total, 4.0));
    }

    #[test]
    fn missing_effort_is_unplaced() {
        let schema = workload_schema();
        let store = make_store(
            &schema,
            vec![make_item(
                "a",
                vec![
                    ("start", FieldValue::Date(ymd(2026, 1, 5))),
                    ("end", FieldValue::Date(ymd(2026, 1, 6))),
                ],
                "",
            )],
        );
        let view = workload_view("start", "end", "effort");

        let data = extract_workload(&view, &store, &schema, &business_week());

        assert!(data.buckets.is_empty());
        assert_eq!(data.unplaced.len(), 1);
        match &data.unplaced[0].reason {
            UnplacedReason::MissingValue { field } => assert_eq!(field, "effort"),
            other => panic!("expected MissingValue(effort), got {other:?}"),
        }
    }

    #[test]
    fn start_after_end_is_unplaced_invalid_range() {
        let schema = workload_schema();
        let store = make_store(
            &schema,
            vec![make_item(
                "a",
                vec![
                    ("start", FieldValue::Date(ymd(2026, 1, 12))),
                    ("end", FieldValue::Date(ymd(2026, 1, 5))),
                    ("effort", FieldValue::Integer(4)),
                ],
                "",
            )],
        );
        let view = workload_view("start", "end", "effort");

        let data = extract_workload(&view, &store, &schema, &business_week());

        assert!(data.buckets.is_empty());
        match &data.unplaced[0].reason {
            UnplacedReason::InvalidRange { .. } => {}
            other => panic!("expected InvalidRange, got {other:?}"),
        }
    }

    #[test]
    fn interval_entirely_on_weekend_is_unplaced_no_working_days() {
        let schema = workload_schema();
        let store = make_store(
            &schema,
            // Sat Jan 10 → Sun Jan 11: zero working days under Mon–Fri.
            vec![make_item(
                "a",
                vec![
                    ("start", FieldValue::Date(ymd(2026, 1, 10))),
                    ("end", FieldValue::Date(ymd(2026, 1, 11))),
                    ("effort", FieldValue::Integer(4)),
                ],
                "",
            )],
        );
        let view = workload_view("start", "end", "effort");

        let data = extract_workload(&view, &store, &schema, &business_week());

        assert!(data.buckets.is_empty());
        assert_eq!(data.unplaced.len(), 1);
        match &data.unplaced[0].reason {
            UnplacedReason::NoWorkingDays {
                start_field,
                end_field,
            } => {
                assert_eq!(start_field, "start");
                assert_eq!(end_field, "end");
            }
            other => panic!("expected NoWorkingDays, got {other:?}"),
        }
    }

    #[test]
    fn empty_view_produces_zero_buckets() {
        let schema = workload_schema();
        let store = make_store(&schema, vec![]);
        let view = workload_view("start", "end", "effort");

        let data = extract_workload(&view, &store, &schema, &business_week());

        assert!(data.buckets.is_empty());
        assert!(data.unplaced.is_empty());
    }

    #[test]
    fn float_effort_supported() {
        let schema = make_schema(vec![
            ("start", FieldTypeConfig::Date),
            ("end", FieldTypeConfig::Date),
            (
                "effort",
                FieldTypeConfig::Float {
                    min: None,
                    max: None,
                },
            ),
        ]);
        let store = make_store(
            &schema,
            // Mon Jan 5 → Tue Jan 6.
            vec![make_item(
                "a",
                vec![
                    ("start", FieldValue::Date(ymd(2026, 1, 5))),
                    ("end", FieldValue::Date(ymd(2026, 1, 6))),
                    ("effort", FieldValue::Float(3.0)),
                ],
                "",
            )],
        );
        let view = workload_view("start", "end", "effort");

        let data = extract_workload(&view, &store, &schema, &business_week());

        assert_eq!(data.buckets.len(), 2);
        assert_eq!(data.unit, WorkloadUnit::Number);
        assert!(close_enough(data.buckets[0].total, 1.5));
        assert!(close_enough(data.buckets[1].total, 1.5));
    }

    #[test]
    fn duration_effort_yields_seconds_buckets_with_duration_unit() {
        let schema = make_schema(vec![
            ("start", FieldTypeConfig::Date),
            ("end", FieldTypeConfig::Date),
            (
                "effort",
                FieldTypeConfig::Duration {
                    min: None,
                    max: None,
                },
            ),
        ]);
        let store = make_store(
            &schema,
            // Mon Jan 5 → Thu Jan 8: 4 working days, 8h total → 2h/day.
            vec![make_item(
                "a",
                vec![
                    ("start", FieldValue::Date(ymd(2026, 1, 5))),
                    ("end", FieldValue::Date(ymd(2026, 1, 8))),
                    ("effort", FieldValue::Duration(8 * 3600)),
                ],
                "",
            )],
        );
        let view = workload_view("start", "end", "effort");

        let data = extract_workload(&view, &store, &schema, &business_week());

        assert_eq!(data.unit, WorkloadUnit::Duration);
        assert_eq!(data.buckets.len(), 4);
        for bucket in &data.buckets {
            // 2 hours = 7200 canonical seconds.
            assert!(close_enough(bucket.total, 7200.0));
        }
    }

    #[test]
    fn view_level_working_days_override_beats_config_calendar() {
        let schema = workload_schema();
        let store = make_store(
            &schema,
            // Mon Jan 5 → Wed Jan 7.
            vec![make_item(
                "a",
                vec![
                    ("start", FieldValue::Date(ymd(2026, 1, 5))),
                    ("end", FieldValue::Date(ymd(2026, 1, 7))),
                    ("effort", FieldValue::Integer(2)),
                ],
                "",
            )],
        );
        // Config calendar says only Monday counts.
        let config_calendar = WorkingCalendar::from_days([Weekday::Monday]);
        // View overrides: only Tuesday and Wednesday count.
        let view = workload_view_with_days(
            "start",
            "end",
            "effort",
            vec![Weekday::Tuesday, Weekday::Wednesday],
        );

        let data = extract_workload(&view, &store, &schema, &config_calendar);

        // Override wins: 2 buckets (Tue, Wed), each 1.0.
        assert_eq!(data.buckets.len(), 2);
        assert_eq!(data.buckets[0].date, ymd(2026, 1, 6));
        assert_eq!(data.buckets[1].date, ymd(2026, 1, 7));
        for bucket in &data.buckets {
            assert!(close_enough(bucket.total, 1.0));
        }
    }
}
