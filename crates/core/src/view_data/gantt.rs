//! Gantt view extractor.
//!
//! Emits one bar per filter-matched item with a valid `[start..=end]`
//! window. Bars always carry resolved `(start, end)` `NaiveDate`s
//! regardless of how the view declared them — the view configures one
//! of three input recipes:
//!
//! - **`start + end`**: read both dates directly.
//! - **`start + duration`**: read `start`, read the duration field's
//!   canonical seconds, ceil to whole days, set `end = start + (days - 1)`.
//! - **`start + after + duration`** (predecessor mode): each item's
//!   `start` is `max(start_field?, max(predecessor.end))`. End computed
//!   from duration as in the simple duration mode. Predecessors are
//!   resolved against the full store, not the filtered set, so chains
//!   span filter boundaries. The recipe applies uniformly to every item
//!   resolved (filtered or not): a filtered-out predecessor without
//!   `duration` is unresolvable, which propagates `PredecessorUnresolved`
//!   to its dependents.
//!
//! `views_check` guarantees exactly one of these three combinations.
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

use std::collections::{HashMap, VecDeque};

use chrono::NaiveDate;
use serde::Serialize;

use crate::model::schema::{FieldTypeConfig, Schema};
use crate::model::views::{View, ViewKind};
use crate::model::{FieldValue, WorkItem};
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
        after,
        group,
    } = &view.kind
    else {
        panic!("extract_gantt called with non-gantt view kind");
    };
    let items = filtered_items(view, store, schema);

    let (mut bars, mut unplaced) = match after {
        Some(after_field) => {
            // views_check guarantees after-mode has duration set, end unset.
            let duration_field = duration
                .as_deref()
                .expect("views_check ensures duration is set in after-mode");
            extract_after_mode(
                view,
                schema,
                store,
                &items,
                start,
                after_field,
                duration_field,
                group.as_deref(),
            )
        }
        None => extract_simple_mode(
            view,
            schema,
            &items,
            start,
            end.as_deref(),
            duration.as_deref(),
            group.as_deref(),
        ),
    };

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

/// Extract bars/unplaced for the original two recipes:
/// `(start, end)` or `(start, duration)`. Each item is independent —
/// no inter-item resolution.
fn extract_simple_mode(
    view: &View,
    schema: &Schema,
    items: &[&WorkItem],
    start: &str,
    end: Option<&str>,
    duration: Option<&str>,
    group: Option<&str>,
) -> (Vec<GanttBar>, Vec<UnplacedCard>) {
    let mut bars: Vec<GanttBar> = Vec::new();
    let mut unplaced: Vec<UnplacedCard> = Vec::new();

    for item in items {
        let card = build_card(item, schema, view);
        let Some(start_date) = as_date(item.fields.get(start)) else {
            unplaced.push(UnplacedCard {
                card,
                reason: UnplacedReason::MissingValue {
                    field: start.to_owned(),
                },
            });
            continue;
        };

        let resolved_end = match (end, duration) {
            (Some(end_field), None) => match as_date(item.fields.get(end_field)) {
                Some(end_date) => Ok(end_date),
                None => Err(UnplacedReason::MissingValue {
                    field: end_field.to_owned(),
                }),
            },
            (None, Some(duration_field)) => {
                match as_duration_seconds(item.fields.get(duration_field)) {
                    Some(seconds) => match end_from_duration(start_date, seconds) {
                        Some(end_date) => Ok(end_date),
                        None => Err(UnplacedReason::InvalidRange {
                            start_field: start.to_owned(),
                            end_field: duration_field.to_owned(),
                        }),
                    },
                    None => Err(UnplacedReason::MissingValue {
                        field: duration_field.to_owned(),
                    }),
                }
            }
            // views_check guarantees exactly one of (end, duration) in simple mode.
            _ => unreachable!("views_check ensures exactly one of end / duration in simple mode"),
        };

        match resolved_end {
            Ok(end_date) if start_date > end_date => {
                let end_field_name = end.or(duration).unwrap().to_owned();
                unplaced.push(UnplacedCard {
                    card,
                    reason: UnplacedReason::InvalidRange {
                        start_field: start.to_owned(),
                        end_field: end_field_name,
                    },
                });
            }
            Ok(end_date) => {
                let group_value =
                    group.and_then(|name| item.fields.get(name).map(format_field_value));
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

    (bars, unplaced)
}

/// Extract bars/unplaced for predecessor mode `(start, after, duration)`.
///
/// Per-item recipe: `start = max(start_field?, max(predecessor.end))`,
/// `end = start + duration`. Predecessors are resolved against the full
/// store (not the filtered set), so a chain that crosses the filter
/// boundary still anchors its filtered tail.
///
/// Algorithm:
/// 1. Compute the transitive closure of `after`-predecessors of the
///    filtered set via BFS through `store.get`.
/// 2. Kahn's topological sort over the closure, processing items in
///    id-sorted order at each level for determinism.
/// 3. Resolve each item using the same recipe — including filtered-out
///    predecessors. An item whose preds aren't all resolved produces
///    `PredecessorUnresolved`.
/// 4. Items left over after the queue drains are in a cycle (or
///    downstream of one). Mark them `Cycle`. `allow_cycles: false` on
///    the link field is supposed to catch this earlier — defense in
///    depth keeps render robust.
/// 5. Emit bars/unplaced for the *filtered* items only; closure items
///    outside the filter are private to the resolution step.
#[allow(clippy::too_many_arguments)]
fn extract_after_mode(
    view: &View,
    schema: &Schema,
    store: &Store,
    filtered: &[&WorkItem],
    start_field: &str,
    after_field: &str,
    duration_field: &str,
    group: Option<&str>,
) -> (Vec<GanttBar>, Vec<UnplacedCard>) {
    let closure = compute_after_closure(filtered, after_field, store);
    let resolutions =
        resolve_after_closure(&closure, after_field, start_field, duration_field);

    let mut bars: Vec<GanttBar> = Vec::new();
    let mut unplaced: Vec<UnplacedCard> = Vec::new();

    for item in filtered {
        let card = build_card(item, schema, view);
        let resolution = resolutions
            .get(item.id.as_str())
            .expect("filtered items are seeded into the closure");
        match resolution {
            ItemResolution::Resolved {
                start_date,
                end_date,
            } => {
                let group_value =
                    group.and_then(|name| item.fields.get(name).map(format_field_value));
                bars.push(GanttBar {
                    card,
                    start: *start_date,
                    end: *end_date,
                    group: group_value,
                });
            }
            ItemResolution::Unplaced(reason) => {
                unplaced.push(UnplacedCard {
                    card,
                    reason: reason.clone(),
                });
            }
        }
    }

    (bars, unplaced)
}

/// Per-item resolution outcome inside the after-mode closure.
enum ItemResolution {
    Resolved {
        start_date: NaiveDate,
        end_date: NaiveDate,
    },
    Unplaced(UnplacedReason),
}

/// BFS the `after`-graph backwards from the filtered set, collecting
/// every transitively-referenced item that exists in the store. Broken
/// links (predecessor IDs not in the store) are silently skipped — the
/// dependent item then surfaces `PredecessorUnresolved` at resolve time.
fn compute_after_closure<'store>(
    filtered: &[&'store WorkItem],
    after_field: &str,
    store: &'store Store,
) -> HashMap<String, &'store WorkItem> {
    let mut closure: HashMap<String, &'store WorkItem> = HashMap::new();
    let mut queue: VecDeque<&'store WorkItem> = VecDeque::new();

    for item in filtered {
        if closure.insert(item.id.as_str().to_owned(), *item).is_none() {
            queue.push_back(*item);
        }
    }

    while let Some(item) = queue.pop_front() {
        for pred_id in predecessor_ids(item, after_field) {
            if closure.contains_key(&pred_id) {
                continue;
            }
            if let Some(pred) = store.get(&pred_id) {
                closure.insert(pred_id, pred);
                queue.push_back(pred);
            }
        }
    }

    closure
}

/// Read predecessor IDs from an item's `after` field. Treats `Link` as a
/// single-element list and `Links` as a multi-element list. Any other
/// value (including missing) returns an empty vec.
fn predecessor_ids(item: &WorkItem, after_field: &str) -> Vec<String> {
    match item.fields.get(after_field) {
        Some(FieldValue::Link(id)) => vec![id.as_str().to_owned()],
        Some(FieldValue::Links(ids)) => ids.iter().map(|id| id.as_str().to_owned()).collect(),
        _ => Vec::new(),
    }
}

/// Topo-sort the closure and resolve every item's `(start, end)` window.
///
/// Items with all in-closure predecessors resolved get processed; their
/// dependents' in-degree drops, and so on. Items left over have at
/// least one unresolved predecessor — by construction either in a cycle
/// or downstream of one. They map to `UnplacedReason::Cycle { via }`.
fn resolve_after_closure(
    closure: &HashMap<String, &WorkItem>,
    after_field: &str,
    start_field: &str,
    duration_field: &str,
) -> HashMap<String, ItemResolution> {
    // Per-item: (in-closure pred count, all preds, dependents).
    let mut in_degree: HashMap<String, usize> = HashMap::with_capacity(closure.len());
    let mut all_preds: HashMap<String, Vec<String>> = HashMap::with_capacity(closure.len());
    let mut dependents: HashMap<String, Vec<String>> = HashMap::new();

    for (id, item) in closure {
        let preds = predecessor_ids(item, after_field);
        let in_closure_count = preds
            .iter()
            .filter(|pred_id| closure.contains_key(pred_id.as_str()))
            .count();
        in_degree.insert(id.clone(), in_closure_count);
        for pred in &preds {
            if closure.contains_key(pred.as_str()) {
                dependents.entry(pred.clone()).or_default().push(id.clone());
            }
        }
        all_preds.insert(id.clone(), preds);
    }

    // Stable id-sorted processing for deterministic resolution and tests.
    let mut ready: Vec<String> = in_degree
        .iter()
        .filter_map(|(id, deg)| (*deg == 0).then(|| id.clone()))
        .collect();
    ready.sort();
    let mut queue: VecDeque<String> = ready.into();

    let mut resolutions: HashMap<String, ItemResolution> = HashMap::with_capacity(closure.len());

    while let Some(id) = queue.pop_front() {
        let item = closure[&id];
        let preds = &all_preds[&id];
        let resolution = resolve_item(
            item,
            preds,
            &resolutions,
            start_field,
            duration_field,
        );
        resolutions.insert(id.clone(), resolution);

        if let Some(deps) = dependents.get(&id) {
            let mut newly_ready: Vec<String> = Vec::new();
            for dep in deps {
                let deg = in_degree
                    .get_mut(dep)
                    .expect("dependent recorded in in_degree");
                *deg -= 1;
                if *deg == 0 {
                    newly_ready.push(dep.clone());
                }
            }
            newly_ready.sort();
            queue.extend(newly_ready);
        }
    }

    // Anything left is in a cycle (or downstream of one).
    for id in closure.keys() {
        if !resolutions.contains_key(id) {
            resolutions.insert(
                id.clone(),
                ItemResolution::Unplaced(UnplacedReason::Cycle {
                    via: after_field.to_owned(),
                }),
            );
        }
    }

    resolutions
}

/// Resolve a single item's window given its predecessors' resolutions.
///
/// Three failure modes:
/// - Any predecessor unresolved → `PredecessorUnresolved`.
/// - No predecessors and no `start` field value → `NoAnchor`.
/// - Missing or non-positive duration → `MissingValue` / `InvalidRange`.
///
/// On success, `start = max(start_field?, max(pred.end))`.
fn resolve_item(
    item: &WorkItem,
    preds: &[String],
    resolutions: &HashMap<String, ItemResolution>,
    start_field: &str,
    duration_field: &str,
) -> ItemResolution {
    let start_field_value = as_date(item.fields.get(start_field));

    let mut pred_max_end: Option<NaiveDate> = None;
    for pred_id in preds {
        match resolutions.get(pred_id) {
            Some(ItemResolution::Resolved { end_date, .. }) => {
                pred_max_end = Some(pred_max_end.map_or(*end_date, |e| e.max(*end_date)));
            }
            // Pred resolved as Unplaced, OR pred isn't in the closure
            // at all (broken link). Both surface to the caller as
            // `PredecessorUnresolved` — the chain breaks here.
            Some(ItemResolution::Unplaced(_)) | None => {
                return ItemResolution::Unplaced(UnplacedReason::PredecessorUnresolved {
                    id: pred_id.clone(),
                });
            }
        }
    }

    let start_date = match (start_field_value, pred_max_end) {
        (Some(s), Some(p)) => s.max(p),
        (Some(s), None) => s,
        (None, Some(p)) => p,
        (None, None) => return ItemResolution::Unplaced(UnplacedReason::NoAnchor),
    };

    let Some(seconds) = as_duration_seconds(item.fields.get(duration_field)) else {
        return ItemResolution::Unplaced(UnplacedReason::MissingValue {
            field: duration_field.to_owned(),
        });
    };
    let Some(end_date) = end_from_duration(start_date, seconds) else {
        return ItemResolution::Unplaced(UnplacedReason::InvalidRange {
            start_field: start_field.to_owned(),
            end_field: duration_field.to_owned(),
        });
    };
    if start_date > end_date {
        return ItemResolution::Unplaced(UnplacedReason::InvalidRange {
            start_field: start_field.to_owned(),
            end_field: duration_field.to_owned(),
        });
    }

    ItemResolution::Resolved {
        start_date,
        end_date,
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
                after: None,
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
                after: None,
                group: group.map(str::to_owned),
            },
        }
    }

    fn gantt_view_after(
        start: &str,
        after: &str,
        duration: &str,
        group: Option<&str>,
    ) -> View {
        View {
            id: "my-gantt".into(),
            where_clauses: vec![],
            title: None,
            kind: ViewKind::Gantt {
                start: start.to_owned(),
                end: None,
                duration: Some(duration.to_owned()),
                after: Some(after.to_owned()),
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

    // ── After mode (predecessor) ─────────────────────────────────────

    fn after_schema() -> Schema {
        make_schema(vec![
            ("start", FieldTypeConfig::Date),
            (
                "estimate",
                FieldTypeConfig::Duration {
                    min: None,
                    max: None,
                },
            ),
            (
                "depends_on",
                FieldTypeConfig::Links {
                    allow_cycles: Some(false),
                    inverse: None,
                },
            ),
            (
                "parent",
                FieldTypeConfig::Link {
                    allow_cycles: Some(false),
                    inverse: None,
                },
            ),
            (
                "team",
                FieldTypeConfig::Choice {
                    values: vec!["a".into(), "b".into()],
                },
            ),
        ])
    }

    fn link_id(id: &str) -> FieldValue {
        FieldValue::Link(crate::model::WorkItemId::from(id.to_owned()))
    }

    fn links_ids(ids: &[&str]) -> FieldValue {
        FieldValue::Links(
            ids.iter()
                .map(|id| crate::model::WorkItemId::from((*id).to_owned()))
                .collect(),
        )
    }

    /// Three-day duration in canonical seconds.
    const DUR_3D: i64 = 3 * 86_400;
    /// Five-day duration.
    const DUR_5D: i64 = 5 * 86_400;
    /// Two-day duration.
    const DUR_2D: i64 = 2 * 86_400;

    #[test]
    fn after_mode_simple_chain() {
        // Root A starts 2026-01-01, 3 days → ends 2026-01-03.
        // B depends on A, 2 days → starts 2026-01-03, ends 2026-01-04.
        // C depends on B, 5 days → starts 2026-01-04, ends 2026-01-08.
        let schema = after_schema();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "a",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 1))),
                        ("estimate", FieldValue::Duration(DUR_3D)),
                    ],
                    "",
                ),
                make_item(
                    "b",
                    vec![
                        ("estimate", FieldValue::Duration(DUR_2D)),
                        ("depends_on", links_ids(&["a"])),
                    ],
                    "",
                ),
                make_item(
                    "c",
                    vec![
                        ("estimate", FieldValue::Duration(DUR_5D)),
                        ("depends_on", links_ids(&["b"])),
                    ],
                    "",
                ),
            ],
        );
        let view = gantt_view_after("start", "depends_on", "estimate", None);

        let data = extract_gantt(&view, &store, &schema);

        assert!(data.unplaced.is_empty(), "got {:?}", data.unplaced);
        assert_eq!(data.bars.len(), 3);
        let by_id: HashMap<&str, &GanttBar> =
            data.bars.iter().map(|b| (b.card.id.as_str(), b)).collect();
        assert_eq!(by_id["a"].start, ymd(2026, 1, 1));
        assert_eq!(by_id["a"].end, ymd(2026, 1, 3));
        assert_eq!(by_id["b"].start, ymd(2026, 1, 3));
        assert_eq!(by_id["b"].end, ymd(2026, 1, 4));
        assert_eq!(by_id["c"].start, ymd(2026, 1, 4));
        assert_eq!(by_id["c"].end, ymd(2026, 1, 8));
    }

    #[test]
    fn after_mode_fan_in_takes_max_of_predecessor_ends() {
        // A ends 2026-01-03, B ends 2026-01-10. D depends on both, takes
        // 2026-01-10 (the later end) as start.
        let schema = after_schema();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "a",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 1))),
                        ("estimate", FieldValue::Duration(DUR_3D)),
                    ],
                    "",
                ),
                make_item(
                    "b",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 1))),
                        ("estimate", FieldValue::Duration(10 * 86_400)),
                    ],
                    "",
                ),
                make_item(
                    "d",
                    vec![
                        ("estimate", FieldValue::Duration(DUR_2D)),
                        ("depends_on", links_ids(&["a", "b"])),
                    ],
                    "",
                ),
            ],
        );
        let view = gantt_view_after("start", "depends_on", "estimate", None);

        let data = extract_gantt(&view, &store, &schema);

        assert!(data.unplaced.is_empty(), "got {:?}", data.unplaced);
        let d = data.bars.iter().find(|b| b.card.id == "d").unwrap();
        assert_eq!(d.start, ymd(2026, 1, 10));
        assert_eq!(d.end, ymd(2026, 1, 11));
    }

    #[test]
    fn after_mode_root_without_start_field_unplaces_no_anchor() {
        let schema = after_schema();
        let store = make_store(
            &schema,
            vec![make_item(
                "a",
                vec![("estimate", FieldValue::Duration(DUR_3D))],
                "",
            )],
        );
        let view = gantt_view_after("start", "depends_on", "estimate", None);

        let data = extract_gantt(&view, &store, &schema);

        assert!(data.bars.is_empty());
        assert_eq!(data.unplaced.len(), 1);
        assert!(matches!(
            &data.unplaced[0].reason,
            UnplacedReason::NoAnchor
        ));
    }

    #[test]
    fn after_mode_predecessor_outside_filter_still_anchors_dependent() {
        // A is in team "a", filtered out. B (team "b") depends on A and
        // resolves against the wider store.
        let schema = after_schema();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "a",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 1))),
                        ("estimate", FieldValue::Duration(DUR_3D)),
                        ("team", FieldValue::Choice("a".into())),
                    ],
                    "",
                ),
                make_item(
                    "b",
                    vec![
                        ("estimate", FieldValue::Duration(DUR_2D)),
                        ("depends_on", links_ids(&["a"])),
                        ("team", FieldValue::Choice("b".into())),
                    ],
                    "",
                ),
            ],
        );
        let view = View {
            id: "v".into(),
            where_clauses: vec!["team=b".into()],
            title: None,
            kind: ViewKind::Gantt {
                start: "start".to_owned(),
                end: None,
                duration: Some("estimate".to_owned()),
                after: Some("depends_on".to_owned()),
                group: None,
            },
        };

        let data = extract_gantt(&view, &store, &schema);

        assert!(data.unplaced.is_empty(), "got {:?}", data.unplaced);
        assert_eq!(data.bars.len(), 1);
        let b = &data.bars[0];
        assert_eq!(b.card.id, "b");
        assert_eq!(b.start, ymd(2026, 1, 3));
        assert_eq!(b.end, ymd(2026, 1, 4));
    }

    #[test]
    fn after_mode_unresolvable_predecessor_propagates() {
        // A has no duration → unresolvable as a predecessor. B depends
        // on A → unplaces with PredecessorUnresolved { id: "a" }.
        let schema = after_schema();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "a",
                    vec![("start", FieldValue::Date(ymd(2026, 1, 1)))],
                    "",
                ),
                make_item(
                    "b",
                    vec![
                        ("estimate", FieldValue::Duration(DUR_2D)),
                        ("depends_on", links_ids(&["a"])),
                    ],
                    "",
                ),
            ],
        );
        let view = gantt_view_after("start", "depends_on", "estimate", None);

        let data = extract_gantt(&view, &store, &schema);

        // A is filtered in, also unplaces (missing duration).
        // B unplaces because A couldn't be resolved.
        assert!(data.bars.is_empty());
        let b_reason = data
            .unplaced
            .iter()
            .find(|u| u.card.id == "b")
            .map(|u| &u.reason)
            .expect("b should be in unplaced");
        match b_reason {
            UnplacedReason::PredecessorUnresolved { id } => assert_eq!(id, "a"),
            other => panic!("expected PredecessorUnresolved, got {other:?}"),
        }
    }

    #[test]
    fn after_mode_start_field_max_with_predecessor_end() {
        // start_field set to 2026-01-15, predecessor ends 2026-01-03.
        // start = max(2026-01-15, 2026-01-03) = 2026-01-15.
        let schema = after_schema();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "a",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 1))),
                        ("estimate", FieldValue::Duration(DUR_3D)),
                    ],
                    "",
                ),
                make_item(
                    "b",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 15))),
                        ("estimate", FieldValue::Duration(DUR_2D)),
                        ("depends_on", links_ids(&["a"])),
                    ],
                    "",
                ),
            ],
        );
        let view = gantt_view_after("start", "depends_on", "estimate", None);

        let data = extract_gantt(&view, &store, &schema);

        let b = data.bars.iter().find(|x| x.card.id == "b").unwrap();
        assert_eq!(b.start, ymd(2026, 1, 15));
    }

    #[test]
    fn after_mode_predecessor_end_max_with_start_field() {
        // start_field set to 2026-01-01, predecessor ends 2026-01-10.
        // start = max(2026-01-01, 2026-01-10) = 2026-01-10.
        let schema = after_schema();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "a",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 1))),
                        ("estimate", FieldValue::Duration(10 * 86_400)),
                    ],
                    "",
                ),
                make_item(
                    "b",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 1))),
                        ("estimate", FieldValue::Duration(DUR_2D)),
                        ("depends_on", links_ids(&["a"])),
                    ],
                    "",
                ),
            ],
        );
        let view = gantt_view_after("start", "depends_on", "estimate", None);

        let data = extract_gantt(&view, &store, &schema);

        let b = data.bars.iter().find(|x| x.card.id == "b").unwrap();
        assert_eq!(b.start, ymd(2026, 1, 10));
    }

    #[test]
    fn after_mode_cycle_unplaces_with_cycle_reason() {
        // A → B → A. Both end up in cycle. (allow_cycles: false on the
        // schema field would normally catch this; we bypass validation
        // via direct insertion to confirm the converter's defense.)
        let schema = after_schema();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "a",
                    vec![
                        ("estimate", FieldValue::Duration(DUR_2D)),
                        ("depends_on", links_ids(&["b"])),
                    ],
                    "",
                ),
                make_item(
                    "b",
                    vec![
                        ("estimate", FieldValue::Duration(DUR_2D)),
                        ("depends_on", links_ids(&["a"])),
                    ],
                    "",
                ),
            ],
        );
        let view = gantt_view_after("start", "depends_on", "estimate", None);

        let data = extract_gantt(&view, &store, &schema);

        assert!(data.bars.is_empty());
        assert_eq!(data.unplaced.len(), 2);
        for unplaced in &data.unplaced {
            match &unplaced.reason {
                UnplacedReason::Cycle { via } => assert_eq!(via, "depends_on"),
                other => panic!("expected Cycle, got {other:?}"),
            }
        }
    }

    #[test]
    fn after_mode_accepts_single_link_field() {
        // `parent` is a single Link, not Links. Should still chain.
        let schema = after_schema();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "a",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 1))),
                        ("estimate", FieldValue::Duration(DUR_3D)),
                    ],
                    "",
                ),
                make_item(
                    "b",
                    vec![
                        ("estimate", FieldValue::Duration(DUR_2D)),
                        ("parent", link_id("a")),
                    ],
                    "",
                ),
            ],
        );
        let view = gantt_view_after("start", "parent", "estimate", None);

        let data = extract_gantt(&view, &store, &schema);

        assert!(data.unplaced.is_empty(), "got {:?}", data.unplaced);
        let b = data.bars.iter().find(|x| x.card.id == "b").unwrap();
        assert_eq!(b.start, ymd(2026, 1, 3));
        assert_eq!(b.end, ymd(2026, 1, 4));
    }

    #[test]
    fn after_mode_no_predecessors_equivalent_to_duration_mode() {
        // Single root item, no predecessors set. After-mode and
        // duration-mode should produce identical bars.
        let schema = after_schema();
        let store = make_store(
            &schema,
            vec![make_item(
                "a",
                vec![
                    ("start", FieldValue::Date(ymd(2026, 1, 1))),
                    ("estimate", FieldValue::Duration(DUR_5D)),
                ],
                "",
            )],
        );
        let dur_view = gantt_view_duration("start", "estimate", None);
        let after_view = gantt_view_after("start", "depends_on", "estimate", None);

        let dur_data = extract_gantt(&dur_view, &store, &schema);
        let after_data = extract_gantt(&after_view, &store, &schema);

        assert_eq!(dur_data.bars.len(), 1);
        assert_eq!(after_data.bars.len(), 1);
        assert_eq!(dur_data.bars[0].start, after_data.bars[0].start);
        assert_eq!(dur_data.bars[0].end, after_data.bars[0].end);
    }

    #[test]
    fn after_mode_missing_duration_unplaces_missing_value() {
        let schema = after_schema();
        let store = make_store(
            &schema,
            vec![make_item(
                "a",
                vec![("start", FieldValue::Date(ymd(2026, 1, 1)))],
                "",
            )],
        );
        let view = gantt_view_after("start", "depends_on", "estimate", None);

        let data = extract_gantt(&view, &store, &schema);

        assert!(data.bars.is_empty());
        match &data.unplaced[0].reason {
            UnplacedReason::MissingValue { field } => assert_eq!(field, "estimate"),
            other => panic!("expected MissingValue, got {other:?}"),
        }
    }
}
