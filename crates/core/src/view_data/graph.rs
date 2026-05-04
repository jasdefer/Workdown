//! Graph view extractor.
//!
//! Produces one node per filter-matched item plus one edge per outgoing
//! link on the view's `field`. The field name may be either a direct
//! Link/Links field or an inverse name (e.g. `children` when `parent`
//! declares `inverse: children`) — inverse names resolve to the original
//! field via [`Schema::inverse_table`] and the underlying edges are the
//! same.
//!
//! Edges to targets outside the filtered set are dropped silently (the
//! store already reported broken references at load time). Duplicate
//! targets are deduped; self-loops kept; orphan nodes (no outgoing edges)
//! remain as isolated nodes.

use std::collections::HashSet;

use serde::Serialize;

use crate::model::schema::Schema;
use crate::model::views::{View, ViewKind};
use crate::model::{FieldValue, WorkItemId};
use crate::store::Store;

use super::common::{build_card, Card};
use super::filter::filtered_items;
use super::tree::{build_tree_data, TreeData};

#[derive(Debug, Clone, Serialize)]
pub struct GraphData {
    pub field: String,
    /// The `group_by` field name from the view config, if any. Mirrored
    /// here so renderers can label or omit grouping without re-reading
    /// `views.yaml`.
    pub group_by: Option<String>,
    pub nodes: Vec<Card>,
    pub edges: Vec<Edge>,
    /// Subgraph nesting tree produced when `group_by` is set. Roots are
    /// items whose `group_by` value is absent or points outside the
    /// filtered set; descendants follow the link chain. Items in `nodes`
    /// and items in `groups` cover the same set in different layouts.
    pub groups: Option<TreeData>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Edge {
    pub from: WorkItemId,
    pub to: WorkItemId,
}

pub fn extract_graph(view: &View, store: &Store, schema: &Schema) -> GraphData {
    let ViewKind::Graph { field, group_by } = &view.kind else {
        panic!("extract_graph called with non-graph view kind");
    };
    let items = filtered_items(view, store, schema);
    let filtered_ids: HashSet<&str> = items.iter().map(|item| item.id.as_str()).collect();
    let source_field = resolve_field(field, schema);

    let nodes: Vec<Card> = items
        .iter()
        .map(|item| build_card(item, schema, view))
        .collect();

    let mut edges: Vec<Edge> = Vec::new();
    let mut seen: HashSet<(WorkItemId, WorkItemId)> = HashSet::new();
    for item in &items {
        let targets: Vec<&WorkItemId> = match item.fields.get(source_field) {
            Some(FieldValue::Link(target)) => vec![target],
            Some(FieldValue::Links(list)) => list.iter().collect(),
            _ => continue,
        };
        for target in targets {
            if !filtered_ids.contains(target.as_str()) {
                continue;
            }
            let key = (item.id.clone(), target.clone());
            if seen.insert(key.clone()) {
                edges.push(Edge {
                    from: key.0,
                    to: key.1,
                });
            }
        }
    }
    edges.sort_by(|left, right| {
        left.from
            .as_str()
            .cmp(right.from.as_str())
            .then_with(|| left.to.as_str().cmp(right.to.as_str()))
    });

    let groups = group_by
        .as_deref()
        .map(|gb_field| build_tree_data(&items, gb_field, store, schema, view));

    GraphData {
        field: field.clone(),
        group_by: group_by.clone(),
        nodes,
        edges,
        groups,
    }
}

fn resolve_field<'schema>(name: &'schema str, schema: &'schema Schema) -> &'schema str {
    if schema.fields.contains_key(name) {
        name
    } else if let Some(original) = schema.inverse_table.get(name) {
        original.as_str()
    } else {
        panic!("views_check validates graph field reference");
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::{FieldTypeConfig, Schema};
    use crate::model::views::{View, ViewKind};
    use crate::view_data::test_support::{
        make_item, make_schema, make_store, make_store_with_files,
    };

    fn graph_view(field: &str) -> View {
        View {
            id: "my-graph".into(),
            where_clauses: vec![],
            title: None,
            kind: ViewKind::Graph {
                field: field.to_owned(),
                group_by: None,
            },
        }
    }

    fn deps_schema() -> Schema {
        make_schema(vec![(
            "depends_on",
            FieldTypeConfig::Links {
                allow_cycles: Some(false),
                inverse: Some("dependents".into()),
            },
        )])
    }

    #[test]
    fn links_field_emits_source_to_target_edges() {
        let schema = deps_schema();
        let store = make_store(
            &schema,
            vec![
                make_item("a", vec![], ""),
                make_item("b", vec![], ""),
                make_item(
                    "c",
                    vec![(
                        "depends_on",
                        FieldValue::Links(vec![
                            WorkItemId::from("a".to_owned()),
                            WorkItemId::from("b".to_owned()),
                        ]),
                    )],
                    "",
                ),
            ],
        );
        let view = graph_view("depends_on");

        let data = extract_graph(&view, &store, &schema);

        let node_ids: Vec<&str> = data.nodes.iter().map(|node| node.id.as_str()).collect();
        assert_eq!(node_ids, vec!["a", "b", "c"]);
        let edge_pairs: Vec<(&str, &str)> = data
            .edges
            .iter()
            .map(|edge| (edge.from.as_str(), edge.to.as_str()))
            .collect();
        assert_eq!(edge_pairs, vec![("c", "a"), ("c", "b")]);
    }

    #[test]
    fn single_link_field_supported() {
        let schema = make_schema(vec![(
            "parent",
            FieldTypeConfig::Link {
                allow_cycles: Some(false),
                inverse: Some("children".into()),
            },
        )]);
        let store = make_store(
            &schema,
            vec![
                make_item("root", vec![], ""),
                make_item(
                    "child",
                    vec![(
                        "parent",
                        FieldValue::Link(WorkItemId::from("root".to_owned())),
                    )],
                    "",
                ),
            ],
        );
        let view = graph_view("parent");

        let data = extract_graph(&view, &store, &schema);

        assert_eq!(data.edges.len(), 1);
        assert_eq!(data.edges[0].from.as_str(), "child");
        assert_eq!(data.edges[0].to.as_str(), "root");
    }

    #[test]
    fn inverse_name_resolves_to_original_field() {
        let schema = deps_schema();
        let store = make_store(
            &schema,
            vec![
                make_item("a", vec![], ""),
                make_item(
                    "b",
                    vec![(
                        "depends_on",
                        FieldValue::Links(vec![WorkItemId::from("a".to_owned())]),
                    )],
                    "",
                ),
            ],
        );
        let view = graph_view("dependents");

        let data = extract_graph(&view, &store, &schema);

        let edge_pairs: Vec<(&str, &str)> = data
            .edges
            .iter()
            .map(|edge| (edge.from.as_str(), edge.to.as_str()))
            .collect();
        assert_eq!(edge_pairs, vec![("b", "a")]);
    }

    #[test]
    fn orphan_nodes_included_without_edges() {
        let schema = deps_schema();
        let store = make_store(
            &schema,
            vec![
                make_item("solo", vec![], ""),
                make_item("other", vec![], ""),
            ],
        );
        let view = graph_view("depends_on");

        let data = extract_graph(&view, &store, &schema);

        assert_eq!(data.nodes.len(), 2);
        assert!(data.edges.is_empty());
    }

    #[test]
    fn self_loops_kept() {
        let schema = make_schema(vec![(
            "blocks",
            FieldTypeConfig::Links {
                allow_cycles: Some(true),
                inverse: None,
            },
        )]);
        let store = make_store(
            &schema,
            vec![make_item(
                "a",
                vec![(
                    "blocks",
                    FieldValue::Links(vec![WorkItemId::from("a".to_owned())]),
                )],
                "",
            )],
        );
        let view = graph_view("blocks");

        let data = extract_graph(&view, &store, &schema);

        assert_eq!(data.edges.len(), 1);
        assert_eq!(data.edges[0].from.as_str(), "a");
        assert_eq!(data.edges[0].to.as_str(), "a");
    }

    #[test]
    fn duplicate_targets_deduped() {
        let schema = deps_schema();
        let store = make_store(
            &schema,
            vec![
                make_item("a", vec![], ""),
                make_item(
                    "b",
                    vec![(
                        "depends_on",
                        FieldValue::Links(vec![
                            WorkItemId::from("a".to_owned()),
                            WorkItemId::from("a".to_owned()),
                        ]),
                    )],
                    "",
                ),
            ],
        );
        let view = graph_view("depends_on");

        let data = extract_graph(&view, &store, &schema);

        assert_eq!(data.edges.len(), 1);
    }

    #[test]
    fn edge_target_outside_filter_dropped() {
        let schema = make_schema(vec![
            (
                "depends_on",
                FieldTypeConfig::Links {
                    allow_cycles: Some(false),
                    inverse: None,
                },
            ),
            (
                "status",
                FieldTypeConfig::Choice {
                    values: vec!["open".into(), "done".into()],
                },
            ),
        ]);
        let store = make_store(
            &schema,
            vec![
                make_item("a", vec![("status", FieldValue::Choice("done".into()))], ""),
                make_item(
                    "b",
                    vec![
                        ("status", FieldValue::Choice("open".into())),
                        (
                            "depends_on",
                            FieldValue::Links(vec![WorkItemId::from("a".to_owned())]),
                        ),
                    ],
                    "",
                ),
            ],
        );
        let view = View {
            id: "g".into(),
            where_clauses: vec!["status=open".into()],
            title: None,
            kind: ViewKind::Graph {
                field: "depends_on".into(),
                group_by: None,
            },
        };

        let data = extract_graph(&view, &store, &schema);

        assert_eq!(data.nodes.len(), 1);
        assert_eq!(data.nodes[0].id.as_str(), "b");
        assert!(data.edges.is_empty());
    }

    #[test]
    fn edges_sorted_by_from_then_to() {
        let schema = deps_schema();
        let store = make_store(
            &schema,
            vec![
                make_item("a", vec![], ""),
                make_item("b", vec![], ""),
                make_item("c", vec![], ""),
                make_item(
                    "d",
                    vec![(
                        "depends_on",
                        FieldValue::Links(vec![
                            WorkItemId::from("c".to_owned()),
                            WorkItemId::from("a".to_owned()),
                            WorkItemId::from("b".to_owned()),
                        ]),
                    )],
                    "",
                ),
            ],
        );
        let view = graph_view("depends_on");

        let data = extract_graph(&view, &store, &schema);

        let edge_pairs: Vec<(&str, &str)> = data
            .edges
            .iter()
            .map(|edge| (edge.from.as_str(), edge.to.as_str()))
            .collect();
        assert_eq!(edge_pairs, vec![("d", "a"), ("d", "b"), ("d", "c")]);
    }

    // ── group_by ────────────────────────────────────────────────

    fn graph_view_grouped(field: &str, group_by: &str) -> View {
        View {
            id: "my-graph".into(),
            where_clauses: vec![],
            title: None,
            kind: ViewKind::Graph {
                field: field.to_owned(),
                group_by: Some(group_by.to_owned()),
            },
        }
    }

    fn group_schema() -> Schema {
        make_schema(vec![
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
                    inverse: Some("children".into()),
                },
            ),
            (
                "status",
                FieldTypeConfig::Choice {
                    values: vec!["open".into(), "done".into()],
                },
            ),
        ])
    }

    #[test]
    fn group_by_unset_leaves_groups_none() {
        let schema = deps_schema();
        let store = make_store(&schema, vec![make_item("a", vec![], "")]);
        let view = graph_view("depends_on");
        let data = extract_graph(&view, &store, &schema);
        assert!(data.group_by.is_none());
        assert!(data.groups.is_none());
    }

    #[test]
    fn group_by_builds_nested_tree_scoped_to_filter() {
        let schema = group_schema();
        let (_tmp, store) = make_store_with_files(
            &schema,
            vec![
                ("epic-a.md", "---\nstatus: open\n---\n"),
                ("task-a1.md", "---\nstatus: open\nparent: epic-a\n---\n"),
                ("task-a2.md", "---\nstatus: open\nparent: epic-a\n---\n"),
                ("epic-b.md", "---\nstatus: open\n---\n"),
                ("task-b1.md", "---\nstatus: open\nparent: epic-b\n---\n"),
            ],
        );
        let view = graph_view_grouped("depends_on", "parent");

        let data = extract_graph(&view, &store, &schema);

        let groups = data.groups.expect("group_by should populate groups");
        assert_eq!(groups.field, "parent");
        let root_ids: Vec<&str> = groups.roots.iter().map(|n| n.card.id.as_str()).collect();
        assert_eq!(root_ids, vec!["epic-a", "epic-b"]);
        let epic_a_children: Vec<&str> = groups.roots[0]
            .children
            .iter()
            .map(|n| n.card.id.as_str())
            .collect();
        assert_eq!(epic_a_children, vec!["task-a1", "task-a2"]);
    }

    #[test]
    fn group_by_filtered_out_parent_promotes_child_to_root() {
        let schema = group_schema();
        let (_tmp, store) = make_store_with_files(
            &schema,
            vec![
                ("epic.md", "---\nstatus: done\n---\n"),
                ("task.md", "---\nstatus: open\nparent: epic\n---\n"),
            ],
        );
        let view = View {
            id: "g".into(),
            where_clauses: vec!["status=open".into()],
            title: None,
            kind: ViewKind::Graph {
                field: "depends_on".into(),
                group_by: Some("parent".into()),
            },
        };

        let data = extract_graph(&view, &store, &schema);

        let groups = data.groups.expect("groups present");
        let root_ids: Vec<&str> = groups.roots.iter().map(|n| n.card.id.as_str()).collect();
        assert_eq!(root_ids, vec!["task"]);
        assert!(groups.roots[0].children.is_empty());
    }
}
