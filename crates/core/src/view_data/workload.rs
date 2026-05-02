//! Workload view extractor.
//!
//! For each filter-matched item with valid `start`, `end`, and numeric
//! `effort`, spreads `effort / days_inclusive` across each day in
//! `[start..=end]` and sums the contributions into dense daily buckets
//! that span `min(start)..=max(end)` across all placed items. Days with
//! no contribution still appear as zero-total buckets so the renderer
//! sees a continuous time axis.
//!
//! Items with missing/invalid data land in `unplaced`, same structure
//! as Gantt: `MissingValue` for absent fields, `InvalidRange` when
//! `start > end`.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde::Serialize;

use crate::model::schema::Schema;
use crate::model::views::{View, ViewKind};
use crate::store::Store;

use super::common::{as_date, as_size, build_card, UnplacedCard, UnplacedReason};
use super::filter::filtered_items;

#[derive(Debug, Clone, Serialize)]
pub struct WorkloadData {
    pub start_field: String,
    pub end_field: String,
    pub effort_field: String,
    pub buckets: Vec<WorkloadBucket>,
    pub unplaced: Vec<UnplacedCard>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkloadBucket {
    pub date: NaiveDate,
    pub total: f64,
}

pub fn extract_workload(view: &View, store: &Store, schema: &Schema) -> WorkloadData {
    let ViewKind::Workload { start, end, effort } = &view.kind else {
        panic!("extract_workload called with non-workload view kind");
    };
    let items = filtered_items(view, store, schema);

    let mut totals: BTreeMap<NaiveDate, f64> = BTreeMap::new();
    let mut unplaced: Vec<UnplacedCard> = Vec::new();

    for item in &items {
        let card = build_card(item, schema, view);
        let start_date = as_date(item.fields.get(start));
        let end_date = as_date(item.fields.get(end));
        // Workload's per-day spread math needs f64; the Duration variant
        // is dropped here pending a proper renderer-side unit story.
        // `views_check` currently rejects duration-typed effort fields, so
        // this only sees Number values today.
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

        let days = (end_date - start_date).num_days() + 1;
        let per_day = effort_value / (days as f64);
        let mut day = start_date;
        while day <= end_date {
            *totals.entry(day).or_insert(0.0) += per_day;
            day = day.succ_opt().expect("date within valid chrono range");
        }
    }

    let mut buckets: Vec<WorkloadBucket> = Vec::new();
    if let (Some((&min, _)), Some((&max, _))) = (totals.iter().next(), totals.iter().next_back()) {
        let mut day = min;
        while day <= max {
            let total = totals.get(&day).copied().unwrap_or(0.0);
            buckets.push(WorkloadBucket { date: day, total });
            day = day.succ_opt().expect("date within valid chrono range");
        }
    }

    unplaced.sort_by(|left, right| left.card.id.as_str().cmp(right.card.id.as_str()));

    WorkloadData {
        start_field: start.clone(),
        end_field: end.clone(),
        effort_field: effort.clone(),
        buckets,
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

    fn ymd(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    fn close_enough(left: f64, right: f64) -> bool {
        (left - right).abs() < 1e-9
    }

    #[test]
    fn single_item_effort_spread_across_days() {
        let schema = workload_schema();
        let store = make_store(
            &schema,
            vec![make_item(
                "a",
                vec![
                    ("start", FieldValue::Date(ymd(2026, 1, 1))),
                    ("end", FieldValue::Date(ymd(2026, 1, 4))),
                    ("effort", FieldValue::Integer(8)),
                ],
                "",
            )],
        );
        let view = workload_view("start", "end", "effort");

        let data = extract_workload(&view, &store, &schema);

        assert_eq!(data.buckets.len(), 4);
        for bucket in &data.buckets {
            assert!(close_enough(bucket.total, 2.0));
        }
        assert!(data.unplaced.is_empty());
    }

    #[test]
    fn overlapping_items_sum_in_shared_days() {
        let schema = workload_schema();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "a",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 1))),
                        ("end", FieldValue::Date(ymd(2026, 1, 2))),
                        ("effort", FieldValue::Integer(4)),
                    ],
                    "",
                ),
                make_item(
                    "b",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 2))),
                        ("end", FieldValue::Date(ymd(2026, 1, 3))),
                        ("effort", FieldValue::Integer(2)),
                    ],
                    "",
                ),
            ],
        );
        let view = workload_view("start", "end", "effort");

        let data = extract_workload(&view, &store, &schema);

        // Jan 1: a=2, Jan 2: a=2 + b=1 = 3, Jan 3: b=1
        assert_eq!(data.buckets.len(), 3);
        assert!(close_enough(data.buckets[0].total, 2.0));
        assert!(close_enough(data.buckets[1].total, 3.0));
        assert!(close_enough(data.buckets[2].total, 1.0));
    }

    #[test]
    fn gap_days_appear_as_zero_buckets() {
        let schema = workload_schema();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "a",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 1))),
                        ("end", FieldValue::Date(ymd(2026, 1, 1))),
                        ("effort", FieldValue::Integer(3)),
                    ],
                    "",
                ),
                make_item(
                    "b",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 4))),
                        ("end", FieldValue::Date(ymd(2026, 1, 4))),
                        ("effort", FieldValue::Integer(1)),
                    ],
                    "",
                ),
            ],
        );
        let view = workload_view("start", "end", "effort");

        let data = extract_workload(&view, &store, &schema);

        assert_eq!(data.buckets.len(), 4);
        assert!(close_enough(data.buckets[0].total, 3.0));
        assert!(close_enough(data.buckets[1].total, 0.0));
        assert!(close_enough(data.buckets[2].total, 0.0));
        assert!(close_enough(data.buckets[3].total, 1.0));
    }

    #[test]
    fn missing_effort_is_unplaced() {
        let schema = workload_schema();
        let store = make_store(
            &schema,
            vec![make_item(
                "a",
                vec![
                    ("start", FieldValue::Date(ymd(2026, 1, 1))),
                    ("end", FieldValue::Date(ymd(2026, 1, 2))),
                ],
                "",
            )],
        );
        let view = workload_view("start", "end", "effort");

        let data = extract_workload(&view, &store, &schema);

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
                    ("start", FieldValue::Date(ymd(2026, 1, 10))),
                    ("end", FieldValue::Date(ymd(2026, 1, 5))),
                    ("effort", FieldValue::Integer(4)),
                ],
                "",
            )],
        );
        let view = workload_view("start", "end", "effort");

        let data = extract_workload(&view, &store, &schema);

        assert!(data.buckets.is_empty());
        match &data.unplaced[0].reason {
            UnplacedReason::InvalidRange { .. } => {}
            other => panic!("expected InvalidRange, got {other:?}"),
        }
    }

    #[test]
    fn empty_view_produces_zero_buckets() {
        let schema = workload_schema();
        let store = make_store(&schema, vec![]);
        let view = workload_view("start", "end", "effort");

        let data = extract_workload(&view, &store, &schema);

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
            vec![make_item(
                "a",
                vec![
                    ("start", FieldValue::Date(ymd(2026, 1, 1))),
                    ("end", FieldValue::Date(ymd(2026, 1, 2))),
                    ("effort", FieldValue::Float(3.0)),
                ],
                "",
            )],
        );
        let view = workload_view("start", "end", "effort");

        let data = extract_workload(&view, &store, &schema);

        assert_eq!(data.buckets.len(), 2);
        assert!(close_enough(data.buckets[0].total, 1.5));
        assert!(close_enough(data.buckets[1].total, 1.5));
    }
}
