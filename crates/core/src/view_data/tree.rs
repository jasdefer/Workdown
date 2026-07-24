//! Tree view extractor.
//!
//! Walks a link field (e.g. `parent`) upward to identify roots and
//! downward to collect children. Items whose link target is outside the
//! filtered set, absent, or broken become roots. Siblings at every level
//! are sorted by id ascending.
//!
//! Each node carries a [`Card`] (with title pre-resolved via the view's
//! `title` display role) plus a `cells` list parallel to the columns
//! derived from the `fields` display role — unset falls back to every
//! schema field, like the table view. Same column semantics as the
//! table view: virtual `id` column, `None` cells when the item lacks
//! that field.

use serde::Serialize;

use crate::model::schema::Schema;
use crate::model::views::{View, ViewKind};
use crate::model::{FieldValue, WorkItem};
use crate::store::Store;

use super::common::{build_card, build_column, column_cell, effective_fields, Card, Column};
use super::filter::filtered_items;
use super::traverse::{walk_forest, Traversal};

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct TreeData {
    pub field: String,
    /// Columns shown alongside the hierarchy, from the `fields` display
    /// role (or the all-schema-fields fallback when the role is unset).
    pub columns: Vec<Column>,
    pub roots: Vec<TreeNode>,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct TreeNode {
    pub card: Card,
    /// Cell values parallel to [`TreeData::columns`]. Empty when the
    /// view has no `columns:` configured.
    pub cells: Vec<Option<FieldValue>>,
    pub children: Vec<TreeNode>,
}

pub fn extract_tree(view: &View, store: &Store, schema: &Schema) -> TreeData {
    let ViewKind::Tree { field } = &view.kind else {
        panic!("extract_tree called with non-tree view kind");
    };
    let items = filtered_items(view, store, schema);
    let columns = effective_fields(view, schema);
    build_tree_data(&items, field, &columns, store, schema, view)
}

/// Build a [`TreeData`] from an already-filtered set of items by walking a
/// link `field` upward to identify roots and downward to collect children.
///
/// Shared with the graph extractor, which uses it to construct the
/// `Option<TreeData>` produced when a graph view's `group_by` slot is set.
/// Graph callers pass an empty `columns` slice — the graph view kind
/// doesn't surface columns.
pub(super) fn build_tree_data(
    items: &[&WorkItem],
    field: &str,
    columns: &[&str],
    store: &Store,
    schema: &Schema,
    view: &View,
) -> TreeData {
    let resolved_columns: Vec<Column> = columns
        .iter()
        .map(|name| build_column(name, schema))
        .collect();
    let forest = walk_forest(items, field, store);
    let roots = forest
        .into_iter()
        .map(|traversal| to_tree_node(traversal, columns, schema, view))
        .collect();
    TreeData {
        field: field.to_owned(),
        columns: resolved_columns,
        roots,
    }
}

fn to_tree_node(traversal: Traversal, columns: &[&str], schema: &Schema, view: &View) -> TreeNode {
    let cells = columns
        .iter()
        .map(|column| column_cell(column, traversal.item))
        .collect();
    TreeNode {
        card: build_card(traversal.item, schema, view),
        cells,
        children: traversal
            .children
            .into_iter()
            .map(|child| to_tree_node(child, columns, schema, view))
            .collect(),
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::{FieldTypeConfig, Schema};
    use crate::model::views::{DisplayConfig, View, ViewKind};
    use crate::view_data::test_support::{make_schema, make_store_with_files};

    fn tree_view(field: &str, where_clauses: Vec<&str>) -> View {
        View {
            id: "my-tree".into(),
            where_clauses: where_clauses.into_iter().map(str::to_owned).collect(),
            display: DisplayConfig::default(),
            kind: ViewKind::Tree {
                field: field.to_owned(),
            },
        }
    }

    fn tree_view_with_columns(field: &str, where_clauses: Vec<&str>, columns: Vec<&str>) -> View {
        let mut view = tree_view(field, where_clauses);
        view.display.fields = Some(columns.into_iter().map(str::to_owned).collect());
        view
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
        assert_eq!(data.roots[0].children[0].children[0].card.id.as_str(), "c");
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

    // ── Columns + cells ─────────────────────────────────────────────

    #[test]
    fn unset_fields_role_falls_back_to_all_schema_fields() {
        let schema = parent_schema();
        let (_tmp, store) = make_store_with_files(
            &schema,
            vec![
                ("a.md", "---\nstatus: open\n---\n"),
                ("b.md", "---\nstatus: open\nparent: a\n---\n"),
            ],
        );
        let view = tree_view("parent", vec![]);

        let data = extract_tree(&view, &store, &schema);

        let names: Vec<&str> = data
            .columns
            .iter()
            .map(|column| column.name.as_str())
            .collect();
        assert_eq!(names, vec!["parent", "status"]);
        assert_eq!(data.roots[0].cells.len(), 2);
        assert_eq!(data.roots[0].children[0].cells.len(), 2);
    }

    #[test]
    fn columns_carry_field_types_and_id_is_virtual_string() {
        use crate::model::schema::FieldType;
        let schema = parent_schema();
        let (_tmp, store) = make_store_with_files(&schema, vec![]);
        let view = tree_view_with_columns("parent", vec![], vec!["id", "status"]);

        let data = extract_tree(&view, &store, &schema);

        let types: Vec<FieldType> = data
            .columns
            .iter()
            .map(|column| column.field_type)
            .collect();
        assert_eq!(types, vec![FieldType::String, FieldType::Choice]);
        let names: Vec<&str> = data.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["id", "status"]);
    }

    #[test]
    fn cells_parallel_to_columns_and_missing_fields_are_none() {
        use crate::model::FieldValue;
        let schema = parent_schema();
        let (_tmp, store) =
            make_store_with_files(&schema, vec![("a.md", "---\nstatus: open\n---\n")]);
        let view = tree_view_with_columns("parent", vec![], vec!["id", "status", "parent"]);

        let data = extract_tree(&view, &store, &schema);

        let cells = &data.roots[0].cells;
        assert_eq!(cells[0], Some(FieldValue::String("a".into())));
        assert_eq!(cells[1], Some(FieldValue::Choice("open".into())));
        assert_eq!(cells[2], None, "`a` has no parent");
    }

    #[test]
    fn columns_propagate_to_children() {
        use crate::model::FieldValue;
        let schema = parent_schema();
        let (_tmp, store) = make_store_with_files(
            &schema,
            vec![
                ("root.md", "---\nstatus: open\n---\n"),
                ("child.md", "---\nstatus: done\nparent: root\n---\n"),
            ],
        );
        let view = tree_view_with_columns("parent", vec![], vec!["status"]);

        let data = extract_tree(&view, &store, &schema);

        assert_eq!(
            data.roots[0].cells,
            vec![Some(FieldValue::Choice("open".into()))]
        );
        assert_eq!(
            data.roots[0].children[0].cells,
            vec![Some(FieldValue::Choice("done".into()))]
        );
    }
}
