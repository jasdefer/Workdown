//! Treemap view extractor.
//!
//! The view's `group` slot is a link field (e.g. `parent`); the forest
//! is walked the same way as Tree. `size` is read from leaves only —
//! internal nodes derive their size by summing their children. The
//! output wraps the top-level roots under a synthetic root whose `card`
//! is `None` and whose `size` is the grand total.
//!
//! Leaves without a numeric `size` field are routed to `unplaced` and
//! dropped from the tree. An internal node whose children all drop for
//! this reason cascades: it too disappears (no data to display).

use serde::Serialize;

use crate::model::schema::{FieldType, Schema};
use crate::model::views::{View, ViewKind};
use crate::store::Store;

use super::common::{as_size, build_card, Card, SizeValue, UnplacedCard, UnplacedReason};
use super::filter::filtered_items;
use super::traverse::{walk_forest, Traversal};

#[derive(Debug, Clone, Serialize)]
pub struct TreemapData {
    pub group_field: String,
    pub size_field: String,
    pub root: TreemapNode,
    pub unplaced: Vec<UnplacedCard>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TreemapNode {
    /// `None` on the synthetic top-level root; `Some` on every real item.
    pub card: Option<Card>,
    pub size: SizeValue,
    pub children: Vec<TreemapNode>,
}

pub fn extract_treemap(view: &View, store: &Store, schema: &Schema) -> TreemapData {
    let ViewKind::Treemap { group, size } = &view.kind else {
        panic!("extract_treemap called with non-treemap view kind");
    };
    let items = filtered_items(view, store, schema);
    let forest = walk_forest(&items, group, store);

    let mut unplaced: Vec<UnplacedCard> = Vec::new();

    let root_nodes: Vec<TreemapNode> = forest
        .into_iter()
        .filter_map(|traversal| to_treemap_node(traversal, size, schema, view, &mut unplaced))
        .collect();

    let zero = zero_for_size_field(size, schema);
    let total_size = root_nodes
        .iter()
        .map(|node| node.size)
        .fold(zero, |left, right| left + right);

    unplaced.sort_by(|left, right| left.card.id.as_str().cmp(right.card.id.as_str()));

    TreemapData {
        group_field: group.clone(),
        size_field: size.clone(),
        root: TreemapNode {
            card: None,
            size: total_size,
            children: root_nodes,
        },
        unplaced,
    }
}

fn to_treemap_node(
    traversal: Traversal,
    size_field: &str,
    schema: &Schema,
    view: &View,
    unplaced: &mut Vec<UnplacedCard>,
) -> Option<TreemapNode> {
    let item = traversal.item;
    let card = build_card(item, schema, view);
    let had_traversal_children = !traversal.children.is_empty();

    let children: Vec<TreemapNode> = traversal
        .children
        .into_iter()
        .filter_map(|child| to_treemap_node(child, size_field, schema, view, unplaced))
        .collect();

    let size = if had_traversal_children {
        if children.is_empty() {
            // All descendants dropped out; nothing left to show.
            return None;
        }
        let zero = zero_for_size_field(size_field, schema);
        children
            .iter()
            .map(|node| node.size)
            .fold(zero, |left, right| left + right)
    } else {
        match as_size(item.fields.get(size_field)) {
            Some(value) => value,
            None => {
                unplaced.push(UnplacedCard {
                    card,
                    reason: UnplacedReason::MissingValue {
                        field: size_field.to_owned(),
                    },
                });
                return None;
            }
        }
    };

    Some(TreemapNode {
        card: Some(card),
        size,
        children,
    })
}

/// Picks the `SizeValue` zero matching the size field's schema type.
///
/// Used to seed sums (synthetic root, internal-node rollups) with the
/// right variant so children of a duration field don't accidentally
/// add into a `Number` accumulator. `views_check` guarantees the field
/// resolves to one of the allowed numeric types; an unexpected type is
/// a programming error and falls back to `Number(0)` defensively.
fn zero_for_size_field(field: &str, schema: &Schema) -> SizeValue {
    match schema.fields.get(field).map(|config| config.field_type()) {
        Some(FieldType::Duration) => SizeValue::Duration(0),
        _ => SizeValue::Number(0.0),
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::{FieldTypeConfig, Schema};
    use crate::model::views::{View, ViewKind};
    use crate::view_data::test_support::{make_schema, make_store_with_files};

    fn treemap_view(group: &str, size: &str) -> View {
        View {
            id: "my-treemap".into(),
            where_clauses: vec![],
            title: None,
            kind: ViewKind::Treemap {
                group: group.to_owned(),
                size: size.to_owned(),
            },
        }
    }

    fn parent_schema() -> Schema {
        make_schema(vec![
            (
                "parent",
                FieldTypeConfig::Link {
                    allow_cycles: Some(false),
                    inverse: Some("children".into()),
                },
            ),
            (
                "effort",
                FieldTypeConfig::Integer {
                    min: None,
                    max: None,
                },
            ),
        ])
    }

    fn duration_schema() -> Schema {
        make_schema(vec![
            (
                "parent",
                FieldTypeConfig::Link {
                    allow_cycles: Some(false),
                    inverse: Some("children".into()),
                },
            ),
            (
                "estimate",
                FieldTypeConfig::Duration {
                    min: None,
                    max: None,
                },
            ),
        ])
    }

    #[test]
    fn single_root_sums_leaf_sizes() {
        let schema = parent_schema();
        let (_tmp, store) = make_store_with_files(
            &schema,
            vec![
                ("root.md", "---\n---\n"),
                ("a.md", "---\nparent: root\neffort: 3\n---\n"),
                ("b.md", "---\nparent: root\neffort: 5\n---\n"),
            ],
        );
        let view = treemap_view("parent", "effort");

        let data = extract_treemap(&view, &store, &schema);

        assert!(close_enough(data.root.size, 8.0));
        assert_eq!(data.root.children.len(), 1);
        let root_node = &data.root.children[0];
        assert_eq!(root_node.card.as_ref().unwrap().id.as_str(), "root");
        assert!(close_enough(root_node.size, 8.0));
        assert_eq!(root_node.children.len(), 2);
    }

    #[test]
    fn nested_sizes_cascade() {
        let schema = parent_schema();
        let (_tmp, store) = make_store_with_files(
            &schema,
            vec![
                ("a.md", "---\n---\n"),
                ("b.md", "---\nparent: a\n---\n"),
                ("c.md", "---\nparent: b\neffort: 2\n---\n"),
                ("d.md", "---\nparent: b\neffort: 3\n---\n"),
            ],
        );
        let view = treemap_view("parent", "effort");

        let data = extract_treemap(&view, &store, &schema);

        assert!(close_enough(data.root.size, 5.0));
        let a = &data.root.children[0];
        let b = &a.children[0];
        assert!(close_enough(a.size, 5.0));
        assert!(close_enough(b.size, 5.0));
        assert_eq!(b.children.len(), 2);
    }

    #[test]
    fn leaf_without_size_drops_from_tree_and_appears_in_unplaced() {
        let schema = parent_schema();
        let (_tmp, store) = make_store_with_files(
            &schema,
            vec![
                ("root.md", "---\n---\n"),
                ("with-size.md", "---\nparent: root\neffort: 4\n---\n"),
                ("no-size.md", "---\nparent: root\n---\n"),
            ],
        );
        let view = treemap_view("parent", "effort");

        let data = extract_treemap(&view, &store, &schema);

        let root_node = &data.root.children[0];
        assert!(close_enough(root_node.size, 4.0));
        let child_ids: Vec<&str> = root_node
            .children
            .iter()
            .map(|node| node.card.as_ref().unwrap().id.as_str())
            .collect();
        assert_eq!(child_ids, vec!["with-size"]);

        assert_eq!(data.unplaced.len(), 1);
        assert_eq!(data.unplaced[0].card.id.as_str(), "no-size");
    }

    #[test]
    fn internal_with_all_unplaced_children_cascades_out() {
        let schema = parent_schema();
        let (_tmp, store) = make_store_with_files(
            &schema,
            vec![
                ("root.md", "---\n---\n"),
                ("intermediate.md", "---\nparent: root\n---\n"),
                ("leaf.md", "---\nparent: intermediate\n---\n"),
            ],
        );
        let view = treemap_view("parent", "effort");

        let data = extract_treemap(&view, &store, &schema);

        // leaf is unplaced (no effort); intermediate has no surviving
        // children; root has no surviving children → everything cascades
        // out, tree is empty.
        assert!(data.root.children.is_empty());
        assert!(close_enough(data.root.size, 0.0));
        assert_eq!(data.unplaced.len(), 1);
        assert_eq!(data.unplaced[0].card.id.as_str(), "leaf");
    }

    #[test]
    fn multiple_roots_appear_at_top_level() {
        let schema = parent_schema();
        let (_tmp, store) = make_store_with_files(
            &schema,
            vec![
                ("a.md", "---\neffort: 2\n---\n"),
                ("b.md", "---\neffort: 3\n---\n"),
                ("c.md", "---\neffort: 5\n---\n"),
            ],
        );
        let view = treemap_view("parent", "effort");

        let data = extract_treemap(&view, &store, &schema);

        assert_eq!(data.root.children.len(), 3);
        assert!(close_enough(data.root.size, 10.0));
        let ids: Vec<&str> = data
            .root
            .children
            .iter()
            .map(|node| node.card.as_ref().unwrap().id.as_str())
            .collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    #[test]
    fn empty_view_produces_empty_synthetic_root() {
        let schema = parent_schema();
        let (_tmp, store) = make_store_with_files(&schema, vec![]);
        let view = treemap_view("parent", "effort");

        let data = extract_treemap(&view, &store, &schema);

        assert!(data.root.card.is_none());
        assert!(data.root.children.is_empty());
        assert!(close_enough(data.root.size, 0.0));
    }

    #[test]
    fn duration_size_field_preserves_variant() {
        let schema = duration_schema();
        let (_tmp, store) = make_store_with_files(
            &schema,
            vec![
                ("root.md", "---\n---\n"),
                ("a.md", "---\nparent: root\nestimate: 1h\n---\n"),
                ("b.md", "---\nparent: root\nestimate: 30min\n---\n"),
            ],
        );
        let view = treemap_view("parent", "estimate");

        let data = extract_treemap(&view, &store, &schema);

        // 1h + 30min = 5400s; carried through as Duration variant.
        assert_eq!(data.root.size, SizeValue::Duration(5400));
        let root_node = &data.root.children[0];
        assert_eq!(root_node.size, SizeValue::Duration(5400));
        assert_eq!(root_node.children.len(), 2);
        for child in &root_node.children {
            assert!(matches!(child.size, SizeValue::Duration(_)));
        }
    }

    #[test]
    fn duration_empty_view_uses_duration_zero() {
        let schema = duration_schema();
        let (_tmp, store) = make_store_with_files(&schema, vec![]);
        let view = treemap_view("parent", "estimate");

        let data = extract_treemap(&view, &store, &schema);

        // No children → variant defaults to schema's field type.
        assert_eq!(data.root.size, SizeValue::Duration(0));
    }

    fn close_enough(left: SizeValue, right: f64) -> bool {
        (left.as_f64() - right).abs() < 1e-9
    }
}
