//! Tree renderer — turns [`TreeData`] into a Markdown document.
//!
//! Output shape: a top-level `# Tree: <field>` heading, then a nested
//! bullet list. Each node is `- [title](base/id.md)`, children are
//! indented two spaces per depth level. Roots appear in the order the
//! extractor produced (ascending by id); children inherit that order.
//!
//! When the view configures `columns:`, each node's set cells are
//! appended after the link as ` — name: value · name: value`, joining
//! with a middle dot and dropping `None` cells. A row with all-None
//! cells (or a view with no `columns:`) emits just the link, no em dash
//! — keeps the file tidy when every node is empty.
//!
//! An empty tree emits just the heading.

use workdown_core::model::field_value::format_field_value;
use workdown_core::model::FieldValue;
use workdown_core::view_data::{Column, TreeData, TreeNode};

use crate::render::markdown::{card_link, emit_description};

/// Render a `TreeData` as a Markdown string.
///
/// `item_link_base` is the relative path from the rendered view file to
/// the work items directory — see `render::board::render_board` for the
/// same parameter. `description` is the one-line caption emitted below
/// the heading.
pub fn render_tree(data: &TreeData, item_link_base: &str, description: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Tree: {}\n\n", data.field));
    emit_description(description, &mut out);
    for root in &data.roots {
        render_node(root, 0, &data.columns, item_link_base, &mut out);
    }
    out
}

fn render_node(
    node: &TreeNode,
    depth: usize,
    columns: &[Column],
    item_link_base: &str,
    out: &mut String,
) {
    let indent = "  ".repeat(depth);
    out.push_str(&indent);
    out.push_str("- ");
    out.push_str(&card_link(&node.card, item_link_base));

    let suffix = format_inline_fields(columns, &node.cells);
    if !suffix.is_empty() {
        out.push_str(" — ");
        out.push_str(&suffix);
    }
    out.push('\n');

    for child in &node.children {
        render_node(child, depth + 1, columns, item_link_base, out);
    }
}

/// Join the set cells of a node into `name: value · name: value`.
/// Returns an empty string when every cell is `None` or there are no
/// columns — caller skips the em dash in that case.
fn format_inline_fields(columns: &[Column], cells: &[Option<FieldValue>]) -> String {
    let parts: Vec<String> = columns
        .iter()
        .zip(cells.iter())
        .filter_map(|(column, cell)| {
            cell.as_ref()
                .map(|value| format!("{}: {}", column.name, format_field_value(value)))
        })
        .collect();
    parts.join(" · ")
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use workdown_core::model::schema::FieldType;
    use workdown_core::model::{FieldValue, WorkItemId};
    use workdown_core::view_data::{Card, Column, TreeData, TreeNode};

    fn card(id: &str, title: Option<&str>) -> Card {
        Card {
            id: WorkItemId::from(id.to_owned()),
            title: title.map(str::to_owned),
            subtitle: None,
            background: None,
            fields: vec![],
            body: String::new(),
        }
    }

    fn leaf(id: &str, title: Option<&str>, cells: Vec<Option<FieldValue>>) -> TreeNode {
        TreeNode {
            card: card(id, title),
            cells,
            children: vec![],
        }
    }

    fn data(columns: Vec<(&str, FieldType)>, roots: Vec<TreeNode>) -> TreeData {
        TreeData {
            field: "parent".to_owned(),
            columns: columns
                .into_iter()
                .map(|(name, field_type)| Column {
                    name: name.to_owned(),
                    field_type,
                })
                .collect(),
            roots,
        }
    }

    #[test]
    fn empty_tree_emits_only_heading() {
        let output = render_tree(&data(vec![], vec![]), "../workdown-items", "");
        assert_eq!(output, "# Tree: parent\n\n");
    }

    #[test]
    fn single_root_no_columns_emits_plain_bullet() {
        let tree = data(vec![], vec![leaf("task-a", Some("Login form"), vec![])]);
        let output = render_tree(&tree, "../workdown-items", "");
        assert!(output.contains("- [Login form](../workdown-items/task-a.md)\n"));
        assert!(!output.contains(" — "));
    }

    #[test]
    fn populated_columns_emit_inline_fields_after_em_dash() {
        let tree = data(
            vec![
                ("status", FieldType::Choice),
                ("points", FieldType::Integer),
            ],
            vec![leaf(
                "task-a",
                Some("Login"),
                vec![
                    Some(FieldValue::Choice("in_progress".into())),
                    Some(FieldValue::Integer(5)),
                ],
            )],
        );
        let output = render_tree(&tree, "../workdown-items", "");
        assert!(output.contains(
            "- [Login](../workdown-items/task-a.md) — status: in_progress · points: 5\n"
        ));
    }

    #[test]
    fn all_none_cells_skip_em_dash() {
        let tree = data(
            vec![
                ("status", FieldType::Choice),
                ("points", FieldType::Integer),
            ],
            vec![leaf("task-a", Some("Login"), vec![None, None])],
        );
        let output = render_tree(&tree, "../workdown-items", "");
        assert!(output.contains("- [Login](../workdown-items/task-a.md)\n"));
        assert!(!output.contains(" — "));
    }

    #[test]
    fn partial_none_cells_join_only_set_values() {
        let tree = data(
            vec![
                ("status", FieldType::Choice),
                ("points", FieldType::Integer),
            ],
            vec![leaf(
                "task-a",
                Some("Login"),
                vec![Some(FieldValue::Choice("open".into())), None],
            )],
        );
        let output = render_tree(&tree, "../workdown-items", "");
        assert!(output.contains("- [Login](../workdown-items/task-a.md) — status: open\n"));
    }

    #[test]
    fn nested_children_get_indented_and_carry_their_own_fields() {
        let tree = data(
            vec![("status", FieldType::Choice)],
            vec![TreeNode {
                card: card("epic", Some("Auth epic")),
                cells: vec![Some(FieldValue::Choice("open".into()))],
                children: vec![leaf(
                    "story",
                    Some("Login"),
                    vec![Some(FieldValue::Choice("in_progress".into()))],
                )],
            }],
        );
        let output = render_tree(&tree, "../workdown-items", "");
        assert!(output.contains("- [Auth epic](../workdown-items/epic.md) — status: open\n"));
        assert!(output.contains("  - [Login](../workdown-items/story.md) — status: in_progress\n"));
    }

    #[test]
    fn full_output_snapshot() {
        let tree = data(
            vec![
                ("status", FieldType::Choice),
                ("assignee", FieldType::String),
            ],
            vec![TreeNode {
                card: card("epic", Some("Auth")),
                cells: vec![Some(FieldValue::Choice("open".into())), None],
                children: vec![leaf(
                    "story",
                    Some("Login flow"),
                    vec![
                        Some(FieldValue::Choice("in_progress".into())),
                        Some(FieldValue::String("alice".into())),
                    ],
                )],
            }],
        );
        let output = render_tree(&tree, "../workdown-items", "");
        let expected = concat!(
            "# Tree: parent\n",
            "\n",
            "- [Auth](../workdown-items/epic.md) — status: open\n",
            "  - [Login flow](../workdown-items/story.md) — status: in_progress · assignee: alice\n",
        );
        assert_eq!(output, expected);
    }
}
