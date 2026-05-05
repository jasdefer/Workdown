//! Tree renderer — turns [`TreeData`] into a Markdown document.
//!
//! Output shape: a top-level `# Tree: <field>` heading, then a nested
//! bullet list. Each node is `- [title](base/id.md)`, children are
//! indented two spaces per depth level. Roots appear in the order the
//! extractor produced (ascending by id); children inherit that order.
//! An empty tree emits just the heading.

use workdown_core::view_data::{TreeData, TreeNode};

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
        render_node(root, 0, item_link_base, &mut out);
    }
    out
}

fn render_node(node: &TreeNode, depth: usize, item_link_base: &str, out: &mut String) {
    let indent = "  ".repeat(depth);
    out.push_str(&format!(
        "{indent}- {link}\n",
        link = card_link(&node.card, item_link_base)
    ));
    for child in &node.children {
        render_node(child, depth + 1, item_link_base, out);
    }
}
