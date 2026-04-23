//! Gantt view extractor.
//!
//! Emits one bar per filter-matched item with a valid `[start..=end]`
//! window. Items missing either date or with `start > end` land in
//! `unplaced` with a structured reason; `start == end` is a legitimate
//! zero-duration bar and the renderer decides how to display it.
//!
//! The optional `group` slot resolves to a stringified field value per
//! bar (via `format_field_value`), giving swim-lane style renderings
//! something to group by; missing `group` values surface as `None` on
//! the bar.

use chrono::NaiveDate;
use serde::Serialize;

use crate::model::schema::Schema;
use crate::model::views::{View, ViewKind};
use crate::query::format::format_field_value;
use crate::store::Store;

use super::common::{as_date, build_card, Card, UnplacedCard, UnplacedReason};
use super::filter::filtered_items;

#[derive(Debug, Clone, Serialize)]
pub struct GanttData {
    pub start_field: String,
    pub end_field: String,
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
    let ViewKind::Gantt { start, end, group } = &view.kind else {
        panic!("extract_gantt called with non-gantt view kind");
    };
    let items = filtered_items(view, store, schema);

    let mut bars: Vec<GanttBar> = Vec::new();
    let mut unplaced: Vec<UnplacedCard> = Vec::new();

    for item in &items {
        let card = build_card(item, schema, view);
        let start_date = as_date(item.fields.get(start));
        let end_date = as_date(item.fields.get(end));

        match (start_date, end_date) {
            (Some(start_date), Some(end_date)) => {
                if start_date > end_date {
                    unplaced.push(UnplacedCard {
                        card,
                        reason: UnplacedReason::InvalidRange {
                            start_field: start.clone(),
                            end_field: end.clone(),
                        },
                    });
                } else {
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
            }
            (None, _) => unplaced.push(UnplacedCard {
                card,
                reason: UnplacedReason::MissingValue {
                    field: start.clone(),
                },
            }),
            (_, None) => unplaced.push(UnplacedCard {
                card,
                reason: UnplacedReason::MissingValue { field: end.clone() },
            }),
        }
    }

    bars.sort_by(|left, right| left.card.id.as_str().cmp(right.card.id.as_str()));
    unplaced.sort_by(|left, right| left.card.id.as_str().cmp(right.card.id.as_str()));

    GanttData {
        start_field: start.clone(),
        end_field: end.clone(),
        group_field: group.clone(),
        bars,
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

    fn gantt_view(start: &str, end: &str, group: Option<&str>) -> View {
        View {
            id: "my-gantt".into(),
            where_clauses: vec![],
            title: None,
            kind: ViewKind::Gantt {
                start: start.to_owned(),
                end: end.to_owned(),
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
    fn bars_and_unplaced_sorted_by_id() {
        let schema = date_schema();
        let d1 = ymd(2026, 1, 1);
        let d2 = ymd(2026, 1, 5);
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "c",
                    vec![
                        ("start", FieldValue::Date(d1)),
                        ("end", FieldValue::Date(d2)),
                    ],
                    "",
                ),
                make_item(
                    "a",
                    vec![
                        ("start", FieldValue::Date(d1)),
                        ("end", FieldValue::Date(d2)),
                    ],
                    "",
                ),
                make_item("z", vec![], ""),
                make_item("m", vec![], ""),
            ],
        );
        let view = gantt_view("start", "end", None);

        let data = extract_gantt(&view, &store, &schema);

        let bar_ids: Vec<&str> = data.bars.iter().map(|b| b.card.id.as_str()).collect();
        assert_eq!(bar_ids, vec!["a", "c"]);
        let unplaced_ids: Vec<&str> = data.unplaced.iter().map(|u| u.card.id.as_str()).collect();
        assert_eq!(unplaced_ids, vec!["m", "z"]);
    }
}
