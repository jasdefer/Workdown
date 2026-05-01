//! Gantt-by-depth extractor.
//!
//! Partitions a Gantt-shaped view by computing each bar's depth in the
//! `depth_link` chain (level 0 = roots, level 1 = direct children, etc.).
//! Bars at the same depth render in the same Mermaid `gantt` block. The
//! chain walk uses the full store, not the filtered set, so depth is
//! invariant under filtering — a leaf whose intermediate ancestors are
//! filtered out still reports its true depth.
//!
//! Per-bar resolution (start, end, unplaced reasons) is delegated to
//! [`super::gantt::resolve_bars`]; this module is just the depth
//! computation, bucketing, and ordering.

use std::collections::HashMap;
use std::collections::HashSet;

use serde::Serialize;

use crate::model::schema::Schema;
use crate::model::views::{View, ViewKind};
use crate::model::{FieldValue, WorkItem};
use crate::store::Store;

use super::common::UnplacedCard;
use super::gantt::{resolve_bars, GanttBar, GanttResolution};

#[derive(Debug, Clone, Serialize)]
pub struct GanttByDepthData {
    pub levels: Vec<Level>,
    pub unplaced: Vec<UnplacedCard>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Level {
    pub depth: usize,
    pub bars: Vec<GanttBar>,
}

pub fn extract_gantt_by_depth(view: &View, store: &Store, schema: &Schema) -> GanttByDepthData {
    let ViewKind::GanttByDepth {
        start,
        end,
        duration,
        after,
        depth_link,
    } = &view.kind
    else {
        panic!("extract_gantt_by_depth called with non-gantt-by-depth view kind");
    };

    let cfg = GanttResolution {
        start,
        end: end.as_deref(),
        duration: duration.as_deref(),
        after: after.as_deref(),
        // No per-chart sectioning — each chart is already scoped to one
        // depth level. `group: None` means resolved bars carry `None`.
        group: None,
    };
    let (bars, unplaced) = resolve_bars(view, store, schema, &cfg);

    // Bucket bars by their depth in the link chain.
    let mut buckets: HashMap<usize, Vec<GanttBar>> = HashMap::new();
    for bar in bars {
        let bar_item = store
            .get(bar.card.id.as_str())
            .expect("filtered bar item exists in store");
        let depth = walk_to_depth(bar_item, depth_link, store);
        buckets.entry(depth).or_default().push(bar);
    }

    // Levels sorted by ascending depth; bars within sorted by (start, id).
    // Empty levels can't occur because each bucket is created on a bar push.
    let mut depths: Vec<usize> = buckets.keys().copied().collect();
    depths.sort();
    let levels: Vec<Level> = depths
        .into_iter()
        .map(|depth| {
            let mut bars = buckets
                .remove(&depth)
                .expect("bucket exists for sorted depth");
            bars.sort_by(|left, right| {
                (left.start, left.card.id.as_str()).cmp(&(right.start, right.card.id.as_str()))
            });
            Level { depth, bars }
        })
        .collect();

    GanttByDepthData { levels, unplaced }
}

/// Walk `depth_link` upward from `start` and count steps to a root.
///
/// Termination cases (return the step count at termination):
/// - Item has no `depth_link` value → it is its own root (depth 0 if
///   start, otherwise the count up to it).
/// - Link target id isn't in the store → effective root.
/// - Cycle defense: a revisit (which `views_check + allow_cycles: false`
///   should already prevent) terminates the walk at the current count.
fn walk_to_depth(start: &WorkItem, depth_link: &str, store: &Store) -> usize {
    let mut visited: HashSet<&str> = HashSet::new();
    visited.insert(start.id.as_str());
    let mut current = start;
    let mut depth: usize = 0;
    loop {
        let next_id = match current.fields.get(depth_link) {
            Some(FieldValue::Link(id)) => id.as_str(),
            _ => return depth,
        };
        let Some(parent) = store.get(next_id) else {
            return depth;
        };
        if !visited.insert(parent.id.as_str()) {
            return depth;
        }
        current = parent;
        depth += 1;
    }
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

    fn view_end(start: &str, end: &str, depth_link: &str) -> View {
        View {
            id: "v".into(),
            where_clauses: vec![],
            title: None,
            kind: ViewKind::GanttByDepth {
                start: start.to_owned(),
                end: Some(end.to_owned()),
                duration: None,
                after: None,
                depth_link: depth_link.to_owned(),
            },
        }
    }

    fn view_after(start: &str, after: &str, duration: &str, depth_link: &str) -> View {
        View {
            id: "v".into(),
            where_clauses: vec![],
            title: None,
            kind: ViewKind::GanttByDepth {
                start: start.to_owned(),
                end: None,
                duration: Some(duration.to_owned()),
                after: Some(after.to_owned()),
                depth_link: depth_link.to_owned(),
            },
        }
    }

    // ── Shallow tree: only level 0 ───────────────────────────────────

    #[test]
    fn shallow_tree_only_roots_produces_one_level() {
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

        let data = extract_gantt_by_depth(&view, &store, &schema);

        assert_eq!(data.levels.len(), 1);
        assert_eq!(data.levels[0].depth, 0);
        let ids: Vec<&str> = data.levels[0]
            .bars
            .iter()
            .map(|b| b.card.id.as_str())
            .collect();
        assert_eq!(ids, vec!["a", "b"]);
    }

    // ── Deep chain: each level has one item ──────────────────────────

    #[test]
    fn deep_chain_produces_one_level_per_step() {
        // grand → mid → leaf. Three depths, one bar each.
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

        let data = extract_gantt_by_depth(&view, &store, &schema);

        assert_eq!(data.levels.len(), 3);
        assert_eq!(data.levels[0].depth, 0);
        assert_eq!(data.levels[0].bars[0].card.id.as_str(), "grand");
        assert_eq!(data.levels[1].depth, 1);
        assert_eq!(data.levels[1].bars[0].card.id.as_str(), "mid");
        assert_eq!(data.levels[2].depth, 2);
        assert_eq!(data.levels[2].bars[0].card.id.as_str(), "leaf");
    }

    // ── Mixed depths: siblings at different depths ───────────────────

    #[test]
    fn mixed_depths_groups_items_by_depth() {
        // root with two children; one child has a grandchild.
        // Levels: 0=[root], 1=[child-a, child-b], 2=[gc].
        let schema = schema_with_parent();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "root",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 1))),
                        ("end", FieldValue::Date(ymd(2026, 1, 31))),
                    ],
                    "",
                ),
                make_item(
                    "child-a",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 1))),
                        ("end", FieldValue::Date(ymd(2026, 1, 10))),
                        ("parent", link_id("root")),
                    ],
                    "",
                ),
                make_item(
                    "child-b",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 12))),
                        ("end", FieldValue::Date(ymd(2026, 1, 20))),
                        ("parent", link_id("root")),
                    ],
                    "",
                ),
                make_item(
                    "gc",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 5))),
                        ("end", FieldValue::Date(ymd(2026, 1, 8))),
                        ("parent", link_id("child-a")),
                    ],
                    "",
                ),
            ],
        );
        let view = view_end("start", "end", "parent");

        let data = extract_gantt_by_depth(&view, &store, &schema);

        assert_eq!(data.levels.len(), 3);
        assert_eq!(data.levels[0].depth, 0);
        let l0: Vec<&str> = data.levels[0]
            .bars
            .iter()
            .map(|b| b.card.id.as_str())
            .collect();
        assert_eq!(l0, vec!["root"]);

        assert_eq!(data.levels[1].depth, 1);
        let l1: Vec<&str> = data.levels[1]
            .bars
            .iter()
            .map(|b| b.card.id.as_str())
            .collect();
        // Sorted by (start, id): child-a starts 2026-01-01, child-b 2026-01-12.
        assert_eq!(l1, vec!["child-a", "child-b"]);

        assert_eq!(data.levels[2].depth, 2);
        let l2: Vec<&str> = data.levels[2]
            .bars
            .iter()
            .map(|b| b.card.id.as_str())
            .collect();
        assert_eq!(l2, vec!["gc"]);
    }

    // ── Filter excludes intermediate ancestor ────────────────────────

    #[test]
    fn filter_excludes_intermediate_ancestor_full_store_walk_keeps_depth() {
        // root → mid → leaf. Filter `team=b` matches only `leaf`.
        // Full-store walk gives depth 2; bar lands alone at level 2.
        let schema = schema_with_parent();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "root",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 1))),
                        ("end", FieldValue::Date(ymd(2026, 1, 31))),
                        ("team", FieldValue::Choice("a".into())),
                    ],
                    "",
                ),
                make_item(
                    "mid",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 5))),
                        ("end", FieldValue::Date(ymd(2026, 1, 20))),
                        ("parent", link_id("root")),
                        ("team", FieldValue::Choice("a".into())),
                    ],
                    "",
                ),
                make_item(
                    "leaf",
                    vec![
                        ("start", FieldValue::Date(ymd(2026, 1, 7))),
                        ("end", FieldValue::Date(ymd(2026, 1, 9))),
                        ("parent", link_id("mid")),
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
            kind: ViewKind::GanttByDepth {
                start: "start".into(),
                end: Some("end".into()),
                duration: None,
                after: None,
                depth_link: "parent".into(),
            },
        };

        let data = extract_gantt_by_depth(&view, &store, &schema);

        assert_eq!(data.levels.len(), 1);
        assert_eq!(data.levels[0].depth, 2);
        assert_eq!(data.levels[0].bars.len(), 1);
        assert_eq!(data.levels[0].bars[0].card.id.as_str(), "leaf");
    }

    // ── Broken link mid-chain ────────────────────────────────────────

    #[test]
    fn broken_link_mid_chain_treats_intermediate_as_root() {
        // leaf → mid → ghost. ghost doesn't exist, so mid's walk
        // terminates at depth 0; leaf's at depth 1.
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

        let data = extract_gantt_by_depth(&view, &store, &schema);

        assert_eq!(data.levels.len(), 2);
        assert_eq!(data.levels[0].depth, 0);
        assert_eq!(data.levels[0].bars[0].card.id.as_str(), "mid");
        assert_eq!(data.levels[1].depth, 1);
        assert_eq!(data.levels[1].bars[0].card.id.as_str(), "leaf");
    }

    // ── After-mode + by-depth interaction ────────────────────────────

    #[test]
    fn after_mode_works_within_depth_partition() {
        // root with children (a, b). a starts 2026-01-01 for 3d, b
        // depends on a for 2d. All resolve, partition by depth.
        let schema = schema_with_parent();
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "root",
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
                        ("parent", link_id("root")),
                    ],
                    "",
                ),
                make_item(
                    "b",
                    vec![
                        ("estimate", FieldValue::Duration(2 * 86_400)),
                        ("depends_on", links_ids(&["a"])),
                        ("parent", link_id("root")),
                    ],
                    "",
                ),
            ],
        );
        let view = view_after("start", "depends_on", "estimate", "parent");

        let data = extract_gantt_by_depth(&view, &store, &schema);

        assert!(data.unplaced.is_empty(), "got {:?}", data.unplaced);
        assert_eq!(data.levels.len(), 2);
        assert_eq!(data.levels[0].depth, 0);
        assert_eq!(data.levels[0].bars[0].card.id.as_str(), "root");
        assert_eq!(data.levels[1].depth, 1);
        let l1: Vec<&str> = data.levels[1]
            .bars
            .iter()
            .map(|b| b.card.id.as_str())
            .collect();
        assert_eq!(l1, vec!["a", "b"]);
        let by_id: HashMap<&str, &GanttBar> = data.levels[1]
            .bars
            .iter()
            .map(|b| (b.card.id.as_str(), b))
            .collect();
        assert_eq!(by_id["b"].start, ymd(2026, 1, 3));
        assert_eq!(by_id["b"].end, ymd(2026, 1, 4));
    }

    // ── Empty / unplaced ─────────────────────────────────────────────

    #[test]
    fn no_bars_only_unplaced_produces_empty_levels() {
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

        let data = extract_gantt_by_depth(&view, &store, &schema);

        assert!(data.levels.is_empty());
        assert_eq!(data.unplaced.len(), 1);
    }
}
