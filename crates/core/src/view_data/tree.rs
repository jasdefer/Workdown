//! Tree view extractor.
//!
//! Walks a link field (e.g. `parent`) upward to identify roots and
//! downward to collect children. Items whose link target is outside the
//! filtered set, absent, or broken become roots. Siblings at every level
//! are sorted by id ascending.

use serde::Serialize;

use crate::model::schema::Schema;
use crate::model::views::{View, ViewKind};
use crate::store::Store;

use super::common::{build_card, Card};
use super::filter::filtered_items;
use super::traverse::{walk_forest, Traversal};

#[derive(Debug, Clone, Serialize)]
pub struct TreeData {
    pub field: String,
    pub roots: Vec<TreeNode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TreeNode {
    pub card: Card,
    pub children: Vec<TreeNode>,
}

pub fn extract_tree(view: &View, store: &Store, schema: &Schema) -> TreeData {
    let ViewKind::Tree { field } = &view.kind else {
        panic!("extract_tree called with non-tree view kind");
    };
    let items = filtered_items(view, store, schema);
    let forest = walk_forest(&items, field, store);
    let roots = forest
        .into_iter()
        .map(|traversal| to_tree_node(traversal, schema, view))
        .collect();
    TreeData {
        field: field.clone(),
        roots,
    }
}

fn to_tree_node(traversal: Traversal, schema: &Schema, view: &View) -> TreeNode {
    TreeNode {
        card: build_card(traversal.item, schema, view),
        children: traversal
            .children
            .into_iter()
            .map(|child| to_tree_node(child, schema, view))
            .collect(),
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::{FieldTypeConfig, Schema};
    use crate::model::views::{View, ViewKind};
    use crate::view_data::test_support::{make_schema, make_store_with_files};

    fn tree_view(field: &str, where_clauses: Vec<&str>) -> View {
        View {
            id: "my-tree".into(),
            where_clauses: where_clauses.into_iter().map(str::to_owned).collect(),
            title: None,
            kind: ViewKind::Tree {
                field: field.to_owned(),
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
                "status",
                FieldTypeConfig::Choice {
                    values: vec!["open".into(), "done".into()],
                },
            ),
        ])
    }

    #[test]
    fn single_root_with_children_sorted_by_id() {
        let schema = parent_schema();
        let (_tmp, store) = make_store_with_files(
            &schema,
            vec![
                ("root.md", "---\nstatus: open\n---\n"),
                ("child-b.md", "---\nstatus: open\nparent: root\n---\n"),
                ("child-a.md", "---\nstatus: open\nparent: root\n---\n"),
            ],
        );
        let view = tree_view("parent", vec![]);

        let data = extract_tree(&view, &store, &schema);

        assert_eq!(data.roots.len(), 1);
        assert_eq!(data.roots[0].card.id.as_str(), "root");
        let child_ids: Vec<&str> = data.roots[0]
            .children
            .iter()
            .map(|node| node.card.id.as_str())
            .collect();
        assert_eq!(child_ids, vec!["child-a", "child-b"]);
    }

    #[test]
    fn deeply_nested_structure_preserved() {
        let schema = parent_schema();
        let (_tmp, store) = make_store_with_files(
            &schema,
            vec![
                ("a.md", "---\nstatus: open\n---\n"),
                ("b.md", "---\nstatus: open\nparent: a\n---\n"),
                ("c.md", "---\nstatus: open\nparent: b\n---\n"),
            ],
        );
        let view = tree_view("parent", vec![]);

        let data = extract_tree(&view, &store, &schema);

        assert_eq!(data.roots.len(), 1);
        assert_eq!(data.roots[0].card.id.as_str(), "a");
        assert_eq!(data.roots[0].children[0].card.id.as_str(), "b");
        assert_eq!(
            data.roots[0].children[0].children[0].card.id.as_str(),
            "c"
        );
    }

    #[test]
    fn broken_parent_reference_makes_item_a_root() {
        let schema = parent_schema();
        let (_tmp, store) = make_store_with_files(
            &schema,
            vec![
                ("orphan.md", "---\nstatus: open\nparent: missing\n---\n"),
                ("root.md", "---\nstatus: open\n---\n"),
            ],
        );
        let view = tree_view("parent", vec![]);

        let data = extract_tree(&view, &store, &schema);

        let ids: Vec<&str> = data.roots.iter().map(|n| n.card.id.as_str()).collect();
        assert_eq!(ids, vec!["orphan", "root"]);
    }

    #[test]
    fn where_filter_restricts_tree_and_promotes_orphans() {
        let schema = parent_schema();
        let (_tmp, store) = make_store_with_files(
            &schema,
            vec![
                ("a.md", "---\nstatus: done\n---\n"),
                ("b.md", "---\nstatus: open\nparent: a\n---\n"),
                ("c.md", "---\nstatus: open\nparent: b\n---\n"),
            ],
        );
        let view = tree_view("parent", vec!["status=open"]);

        let data = extract_tree(&view, &store, &schema);

        // `a` is filtered out so `b` becomes a root; `c` stays under it.
        assert_eq!(data.roots.len(), 1);
        assert_eq!(data.roots[0].card.id.as_str(), "b");
        assert_eq!(data.roots[0].children[0].card.id.as_str(), "c");
    }

    #[test]
    fn empty_store_produces_zero_roots() {
        let schema = parent_schema();
        let (_tmp, store) = make_store_with_files(&schema, vec![]);
        let view = tree_view("parent", vec![]);

        let data = extract_tree(&view, &store, &schema);

        assert!(data.roots.is_empty());
    }

    #[test]
    fn multiple_roots_sorted_by_id() {
        let schema = parent_schema();
        let (_tmp, store) = make_store_with_files(
            &schema,
            vec![
                ("z.md", "---\nstatus: open\n---\n"),
                ("a.md", "---\nstatus: open\n---\n"),
                ("m.md", "---\nstatus: open\n---\n"),
            ],
        );
        let view = tree_view("parent", vec![]);

        let data = extract_tree(&view, &store, &schema);

        let ids: Vec<&str> = data.roots.iter().map(|n| n.card.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "m", "z"]);
    }
}
