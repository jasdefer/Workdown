//! Gantt-by-initiative extractor.
//!
//! Partitions a Gantt-shaped view by walking each bar's `root_link`
//! chain upward against the full store. Bars that share a root id end
//! up in the same [`Initiative`]; each initiative renders as its own
//! Mermaid `gantt` block.
//!
//! The chain walk uses the full store, not the filtered set, so chains
//! span filter boundaries — a filtered-in bar whose root is filtered out
//! still lands under that root's initiative, with the root labelling the
//! chart heading.
//!
//! Per-bar resolution (start, end, unplaced reasons) is delegated to
//! `super::gantt::resolve_bars`; this module is just the partition
//! and ordering on top.

use std::collections::HashMap;

use serde::Serialize;

use crate::model::schema::Schema;
use crate::model::views::{View, ViewKind};
use crate::model::WorkItem;
use crate::store::Store;
use crate::walker::walk_up;

use super::common::{build_card, Card, UnplacedCard};
use super::gantt::{resolve_bars, GanttBar, GanttResolution};

#[derive(Debug, Clone, Serialize)]
pub struct GanttByInitiativeData {
    pub initiatives: Vec<Initiative>,
    pub unplaced: Vec<UnplacedCard>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Initiative {
    pub root: Card,
    pub bars: Vec<GanttBar>,
}

pub fn extract_gantt_by_initiative(
    view: &View,
    store: &Store,
    schema: &Schema,
) -> GanttByInitiativeData {
    let ViewKind::GanttByInitiative {
        start,
        end,
        duration,
        after,
        root_link,
    } = &view.kind
    else {
        panic!("extract_gantt_by_initiative called with non-gantt-by-initiative view kind");
    };

    let cfg = GanttResolution {
        start,
        end: end.as_deref(),
        duration: duration.as_deref(),
        after: after.as_deref(),
        // No per-chart sectioning — each chart is already scoped to one
        // initiative. `group: None` means resolved bars carry `None`.
        group: None,
    };
    let (bars, unplaced) = resolve_bars(view, store, schema, &cfg);

    // Bucket bars by their initiative root id.
    let mut buckets: HashMap<String, Vec<GanttBar>> = HashMap::new();
    let mut roots: HashMap<String, &WorkItem> = HashMap::new();
    for bar in bars {
        let bar_item = store
            .get(bar.card.id.as_str())
            .expect("filtered bar item exists in store");
        let root_item = walk_to_root(bar_item, root_link, store);
        let root_id = root_item.id.as_str().to_owned();
        roots.entry(root_id.clone()).or_insert(root_item);
        buckets.entry(root_id).or_default().push(bar);
    }

    // Initiatives sorted alphabetically by root id; bars within sorted
    // by (start, id). Empty initiatives can't occur because each bucket
    // is created on a bar push.
    let mut initiative_ids: Vec<String> = buckets.keys().cloned().collect();
    initiative_ids.sort();
    let initiatives: Vec<Initiative> = initiative_ids
        .into_iter()
        .map(|root_id| {
            let mut bars = buckets
                .remove(&root_id)
                .expect("bucket exists for sorted id");
            bars.sort_by(|left, right| {
                (left.start, left.card.id.as_str()).cmp(&(right.start, right.card.id.as_str()))
            });
            let root_item = roots[&root_id];
            Initiative {
                root: build_card(root_item, schema, view),
                bars,
            }
        })
        .collect();

    GanttByInitiativeData {
        initiatives,
        unplaced,
    }
}

/// Walk `root_link` upward from `start` to find the initiative root.
///
/// Returns the last reachable ancestor — or `start` itself when the chain
/// is empty (no `root_link` value, target outside the store, or a cycle).
fn walk_to_root<'store>(
    start: &'store WorkItem,
    root_link: &'store str,
    store: &'store Store,
) -> &'store WorkItem {
    walk_up(start, root_link, store).last().unwrap_or(start)
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    use crate::model::schema::FieldTypeConfig;
    use crate::model::views::{View, ViewKind};
    use crate::model::{FieldValue, WorkItemId};
    use crate::view_data::test_support::{make_item, make_schema, make_store};

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn link_id(id: &str) -> FieldValue {
        FieldValue::Link(WorkItemId::from(id.to_owned()))
    }

    fn links_ids(ids: &[&str]) -> FieldValue {
        FieldValue::Links(
            ids.iter()
                .map(|id| WorkItemId::from((*id).to_owned()))
                .collect(),
        )
    }

    fn schema_with_parent() -> Schema {
        make_schema(vec![
            ("start", FieldTypeConfig::Date),
            ("end", FieldTypeConfig::Date),
            (
                "estimate",
                FieldTypeConfig::Duration {
                    min: None,
                    max: None,
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
                "depends_on",
                FieldTypeConfig::Links {
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

    fn view_end(start: &str, end: &str, root_link: &str) -> View {
        View {
            id: "v".into(),
            where_clauses: vec![],
            title: None,
            kind: ViewKind::GanttByInitiative {
                start: start.to_owned(),
                end: Some(end.to_owned()),
                duration: None,
                after: None,
                root_link: root_link.to_owned(),
            },
        }
    }

    fn view_after(start: &str, after: &str, duration: &str, root_link: &str) -> View {
        View {
            id: "v".into(),
            where_clauses: vec![],
            title: None,
            kind: ViewKind::GanttByInitiative {
                start: start.to_owned(),
                end: None,
                duration: Some(duration.to_owned()),
                after: Some(after.to_owned()),
                root_link: root_link.to_owned(),
            },
        }
    }

    // ── Single initiative ────────────────────────────────────────────

    #[test]
    fn single_initiative_with_one_root_and_two_children() {
        let schema = schema_with_parent();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "epic",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 1))),
                        ("end", FieldValue::Date(ymd(2026, 1, 31))),
                    ],
                    "",
                ),
                make_item(
                    "task-a",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 5))),
                        ("end", FieldValue::Date(ymd(2026, 1, 10))),
                        ("parent", link_id("epic")),
                    ],
                    "",
                ),
                make_item(
                    "task-b",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 12))),
                        ("end", FieldValue::Date(ymd(2026, 1, 18))),
                        ("parent", link_id("epic")),
                    ],
                    "",
                ),
            ],
        );
        let view = view_end("start", "end", "parent");

        let data = extract_gantt_by_initiative(&view, &store, &schema);

        assert_eq!(data.initiatives.len(), 1);
        let init = &data.initiatives[0];
        assert_eq!(init.root.id.as_str(), "epic");
        assert_eq!(init.bars.len(), 3);
        // Bars sorted by (start, id) within the initiative.
        let ids: Vec<&str> = init.bars.iter().map(|b| b.card.id.as_str()).collect();
        assert_eq!(ids, vec!["epic", "task-a", "task-b"]);
    }

    // ── Multiple initiatives ─────────────────────────────────────────

    #[test]
    fn multiple_initiatives_sorted_alphabetically_by_root_id() {
        let schema = schema_with_parent();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "beta",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 1))),
                        ("end", FieldValue::Date(ymd(2026, 1, 5))),
                    ],
                    "",
                ),
                make_item(
                    "alpha",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 1))),
                        ("end", FieldValue::Date(ymd(2026, 1, 5))),
                    ],
                    "",
                ),
                make_item(
                    "child-of-alpha",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 6))),
                        ("end", FieldValue::Date(ymd(2026, 1, 9))),
                        ("parent", link_id("alpha")),
                    ],
                    "",
                ),
            ],
        );
        let view = view_end("start", "end", "parent");

        let data = extract_gantt_by_initiative(&view, &store, &schema);

        assert_eq!(data.initiatives.len(), 2);
        assert_eq!(data.initiatives[0].root.id.as_str(), "alpha");
        assert_eq!(data.initiatives[0].bars.len(), 2);
        assert_eq!(data.initiatives[1].root.id.as_str(), "beta");
        assert_eq!(data.initiatives[1].bars.len(), 1);
    }

    // ── All-orphan (each item is its own root) ───────────────────────

    #[test]
    fn all_orphan_each_item_is_its_own_initiative() {
        let schema = schema_with_parent();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "a",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 1))),
                        ("end", FieldValue::Date(ymd(2026, 1, 5))),
                    ],
                    "",
                ),
                make_item(
                    "b",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 1))),
                        ("end", FieldValue::Date(ymd(2026, 1, 5))),
                    ],
                    "",
                ),
            ],
        );
        let view = view_end("start", "end", "parent");

        let data = extract_gantt_by_initiative(&view, &store, &schema);

        assert_eq!(data.initiatives.len(), 2);
        assert_eq!(data.initiatives[0].root.id.as_str(), "a");
        assert_eq!(data.initiatives[1].root.id.as_str(), "b");
    }

    // ── Deep chain ───────────────────────────────────────────────────

    #[test]
    fn deep_chain_walks_to_top_level_root() {
        // grand → mid → leaf. All three are bars; root is `grand`.
        let schema = schema_with_parent();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "grand",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 1))),
                        ("end", FieldValue::Date(ymd(2026, 1, 31))),
                    ],
                    "",
                ),
                make_item(
                    "mid",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 5))),
                        ("end", FieldValue::Date(ymd(2026, 1, 20))),
                        ("parent", link_id("grand")),
                    ],
                    "",
                ),
                make_item(
                    "leaf",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 7))),
                        ("end", FieldValue::Date(ymd(2026, 1, 9))),
                        ("parent", link_id("mid")),
                    ],
                    "",
                ),
            ],
        );
        let view = view_end("start", "end", "parent");

        let data = extract_gantt_by_initiative(&view, &store, &schema);

        assert_eq!(data.initiatives.len(), 1);
        assert_eq!(data.initiatives[0].root.id.as_str(), "grand");
        assert_eq!(data.initiatives[0].bars.len(), 3);
    }

    // ── Broken link mid-chain ────────────────────────────────────────

    #[test]
    fn broken_link_mid_chain_treats_intermediate_as_root() {
        // leaf → mid → ghost. `ghost` doesn't exist, so `mid` becomes
        // the effective root for `leaf`'s walk. `mid` itself walks to
        // the same root.
        let schema = schema_with_parent();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "mid",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 5))),
                        ("end", FieldValue::Date(ymd(2026, 1, 20))),
                        ("parent", link_id("ghost")),
                    ],
                    "",
                ),
                make_item(
                    "leaf",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 7))),
                        ("end", FieldValue::Date(ymd(2026, 1, 9))),
                        ("parent", link_id("mid")),
                    ],
                    "",
                ),
            ],
        );
        let view = view_end("start", "end", "parent");

        let data = extract_gantt_by_initiative(&view, &store, &schema);

        assert_eq!(data.initiatives.len(), 1);
        assert_eq!(data.initiatives[0].root.id.as_str(), "mid");
        assert_eq!(data.initiatives[0].bars.len(), 2);
    }

    // ── Filter excludes the root ─────────────────────────────────────

    #[test]
    fn filter_excludes_root_initiative_still_appears_with_root_title() {
        // Filter: team=b. Root `epic` is team=a (filtered out). Child
        // `task` is team=b (filtered in). Initiative still renders with
        // `epic` as the heading; only `task` appears as a bar.
        let schema = schema_with_parent();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "epic",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 1))),
                        ("end", FieldValue::Date(ymd(2026, 1, 31))),
                        ("team", FieldValue::Choice("a".into())),
                    ],
                    "",
                ),
                make_item(
                    "task",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 5))),
                        ("end", FieldValue::Date(ymd(2026, 1, 10))),
                        ("parent", link_id("epic")),
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
            kind: ViewKind::GanttByInitiative {
                start: "start".into(),
                end: Some("end".into()),
                duration: None,
                after: None,
                root_link: "parent".into(),
            },
        };

        let data = extract_gantt_by_initiative(&view, &store, &schema);

        assert_eq!(data.initiatives.len(), 1);
        let init = &data.initiatives[0];
        assert_eq!(init.root.id.as_str(), "epic");
        assert_eq!(init.bars.len(), 1);
        assert_eq!(init.bars[0].card.id.as_str(), "task");
    }

    // ── After-mode + by-initiative ───────────────────────────────────

    #[test]
    fn after_mode_works_within_initiative_partition() {
        // epic groups (a, b). a starts 2026-01-01 for 3d, b depends on a
        // for 2d. Both end up in the `epic` initiative; b's start is
        // anchored on a's end.
        let schema = schema_with_parent();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "epic",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 1))),
                        ("estimate", FieldValue::Duration(31 * 86_400)),
                    ],
                    "",
                ),
                make_item(
                    "a",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 1))),
                        ("estimate", FieldValue::Duration(3 * 86_400)),
                        ("parent", link_id("epic")),
                    ],
                    "",
                ),
                make_item(
                    "b",
                    vec![
                        ("estimate", FieldValue::Duration(2 * 86_400)),
                        ("depends_on", links_ids(&["a"])),
                        ("parent", link_id("epic")),
                    ],
                    "",
                ),
            ],
        );
        let view = view_after("start", "depends_on", "estimate", "parent");

        let data = extract_gantt_by_initiative(&view, &store, &schema);

        assert!(data.unplaced.is_empty(), "got {:?}", data.unplaced);
        assert_eq!(data.initiatives.len(), 1);
        assert_eq!(data.initiatives[0].root.id.as_str(), "epic");
        let bars = &data.initiatives[0].bars;
        assert_eq!(bars.len(), 3);
        let by_id: HashMap<&str, &GanttBar> =
            bars.iter().map(|b| (b.card.id.as_str(), b)).collect();
        assert_eq!(by_id["a"].start, ymd(2026, 1, 1));
        assert_eq!(by_id["a"].end, ymd(2026, 1, 3));
        assert_eq!(by_id["b"].start, ymd(2026, 1, 3));
        assert_eq!(by_id["b"].end, ymd(2026, 1, 4));
    }

    // ── Empty / unplaced ─────────────────────────────────────────────

    #[test]
    fn no_bars_only_unplaced_produces_empty_initiatives() {
        let schema = schema_with_parent();
        let store = make_store(
            &schema,
            // No start date → unplaced.
            vec![make_item(
                "a",
                vec![("end", FieldValue::Date(ymd(2026, 1, 5)))],
                "",
            )],
        );
        let view = view_end("start", "end", "parent");

        let data = extract_gantt_by_initiative(&view, &store, &schema);

        assert!(data.initiatives.is_empty());
        assert_eq!(data.unplaced.len(), 1);
    }

    // ── Title slot resolution for root ───────────────────────────────

    #[test]
    fn root_card_title_resolves_via_view_title_slot() {
        let schema = make_schema(vec![
            ("start", FieldTypeConfig::Date),
            ("end", FieldTypeConfig::Date),
            ("name", FieldTypeConfig::String { pattern: None }),
            (
                "parent",
                FieldTypeConfig::Link {
                    allow_cycles: Some(false),
                    inverse: None,
                },
            ),
        ]);
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "epic",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 1))),
                        ("end", FieldValue::Date(ymd(2026, 1, 31))),
                        ("name", FieldValue::String("User Auth Epic".into())),
                    ],
                    "",
                ),
                make_item(
                    "task",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 5))),
                        ("end", FieldValue::Date(ymd(2026, 1, 10))),
                        ("parent", link_id("epic")),
                    ],
                    "",
                ),
            ],
        );
        let view = View {
            id: "v".into(),
            where_clauses: vec![],
            title: Some("name".into()),
            kind: ViewKind::GanttByInitiative {
                start: "start".into(),
                end: Some("end".into()),
                duration: None,
                after: None,
                root_link: "parent".into(),
            },
        };

        let data = extract_gantt_by_initiative(&view, &store, &schema);

        let init = &data.initiatives[0];
        assert_eq!(init.root.title.as_deref(), Some("User Auth Epic"));
    }
}
