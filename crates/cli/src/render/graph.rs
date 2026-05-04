//! Graph renderer — turns [`GraphData`] into a Markdown document with a
//! Mermaid `flowchart TD` block.
//!
//! Output shape: a top-level `# Graph: <field>` heading, then a Mermaid
//! code block. When the view has `group_by` set, items nest into
//! `subgraph` blocks following the link chain — an item with children in
//! the filtered tree becomes a labelled box, an item without becomes a
//! plain node. Edges follow node declarations and use plain `-->`
//! arrows. An empty graph renders the heading only (no Mermaid block).

use std::collections::HashMap;
use std::fmt::Write as _;

use workdown_core::view_data::{Card, GraphData, TreeNode};

use crate::render::common::emit_description;

/// Render a `GraphData` as a Markdown string.
///
/// Workdown ids are validated to `[a-z0-9][a-z0-9-]*` (no trailing
/// hyphen) at parse time, which is a strict subset of what Mermaid
/// accepts as a raw flowchart node id — so we can use the workdown id
/// directly as the Mermaid node id without escaping or aliasing.
/// `description` is the one-line caption emitted below the heading.
pub fn render_graph(data: &GraphData, description: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Graph: {}\n\n", data.field));
    emit_description(description, &mut out);

    if data.nodes.is_empty() {
        return out;
    }

    out.push_str("```mermaid\n");
    out.push_str("flowchart TD\n");

    let card_for: HashMap<&str, &Card> = data.nodes.iter().map(|c| (c.id.as_str(), c)).collect();

    match &data.groups {
        Some(tree) => {
            for root in &tree.roots {
                render_tree_node(root, 1, &card_for, &mut out);
            }
        }
        None => {
            for card in &data.nodes {
                render_leaf(card, 1, &mut out);
            }
        }
    }

    for edge in &data.edges {
        let _ = writeln!(out, "    {} --> {}", edge.from, edge.to);
    }

    out.push_str("```\n");
    out
}

/// Recursively emit a `TreeNode` as either a leaf node line or a
/// `subgraph ... end` block. Nesting depth grows the indent — Mermaid is
/// whitespace-tolerant; the indent is for `.md` source readability.
fn render_tree_node(
    node: &TreeNode,
    depth: usize,
    card_for: &HashMap<&str, &Card>,
    out: &mut String,
) {
    let card = card_for
        .get(node.card.id.as_str())
        .copied()
        .unwrap_or(&node.card);
    if node.children.is_empty() {
        render_leaf(card, depth, out);
    } else {
        let indent = indent_for(depth);
        let _ = writeln!(
            out,
            "{indent}subgraph {id} [\"{label}\"]",
            id = card.id,
            label = sanitize(label_for(card)),
        );
        for child in &node.children {
            render_tree_node(child, depth + 1, card_for, out);
        }
        let _ = writeln!(out, "{indent}end");
    }
}

fn render_leaf(card: &Card, depth: usize, out: &mut String) {
    let indent = indent_for(depth);
    let _ = writeln!(
        out,
        "{indent}{id}[\"{label}\"]",
        id = card.id,
        label = sanitize(label_for(card)),
    );
}

fn label_for(card: &Card) -> &str {
    card.title.as_deref().unwrap_or_else(|| card.id.as_str())
}

fn indent_for(depth: usize) -> String {
    "    ".repeat(depth)
}

/// Neutralize the only characters that break Mermaid's quoted-label
/// syntax: `"` closes the label early, `\n`/`\r` end the statement.
/// Other characters (parens, brackets, `<`, `>`, `&`, `#`) render
/// literally inside `["..."]`.
fn sanitize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '"' => out.push('\''),
            '\n' | '\r' => out.push(' '),
            other => out.push(other),
        }
    }
    out
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use workdown_core::model::WorkItemId;
    use workdown_core::view_data::{Card, Edge, GraphData, TreeData, TreeNode};

    fn card(id: &str, title: Option<&str>) -> Card {
        Card {
            id: WorkItemId::from(id.to_owned()),
            title: title.map(str::to_owned),
            fields: vec![],
            body: String::new(),
        }
    }

    fn graph(field: &str, nodes: Vec<Card>, edges: Vec<Edge>) -> GraphData {
        GraphData {
            field: field.to_owned(),
            group_by: None,
            nodes,
            edges,
            groups: None,
        }
    }

    fn edge(from: &str, to: &str) -> Edge {
        Edge {
            from: WorkItemId::from(from.to_owned()),
            to: WorkItemId::from(to.to_owned()),
        }
    }

    fn tree_node(id: &str, title: Option<&str>, children: Vec<TreeNode>) -> TreeNode {
        TreeNode {
            card: card(id, title),
            children,
        }
    }

    #[test]
    fn renders_top_heading_with_field_name() {
        let data = graph("depends_on", vec![card("a", None)], vec![]);
        let output = render_graph(&data, "");
        assert!(output.starts_with("# Graph: depends_on\n"));
    }

    #[test]
    fn empty_graph_emits_heading_only() {
        let data = graph("depends_on", vec![], vec![]);
        let output = render_graph(&data, "");
        assert_eq!(output, "# Graph: depends_on\n\n");
        assert!(!output.contains("```mermaid"));
    }

    #[test]
    fn flat_graph_emits_nodes_and_edges_inside_mermaid_block() {
        let data = graph(
            "depends_on",
            vec![card("a", Some("Item A")), card("b", None)],
            vec![edge("a", "b")],
        );
        let output = render_graph(&data, "");
        assert!(output.contains("```mermaid\nflowchart TD\n"));
        assert!(output.contains("    a[\"Item A\"]\n"));
        assert!(output.contains("    b[\"b\"]\n"));
        assert!(output.contains("    a --> b\n"));
        assert!(output.trim_end().ends_with("```"));
    }

    #[test]
    fn grouped_graph_nests_subgraph_blocks() {
        let data = GraphData {
            field: "depends_on".into(),
            group_by: Some("parent".into()),
            nodes: vec![
                card("epic-a", Some("Epic A")),
                card("task-1", Some("Task 1")),
                card("task-2", Some("Task 2")),
            ],
            edges: vec![edge("task-2", "task-1")],
            groups: Some(TreeData {
                field: "parent".into(),
                roots: vec![tree_node(
                    "epic-a",
                    Some("Epic A"),
                    vec![
                        tree_node("task-1", Some("Task 1"), vec![]),
                        tree_node("task-2", Some("Task 2"), vec![]),
                    ],
                )],
            }),
        };
        let output = render_graph(&data, "");
        assert!(output.contains("    subgraph epic-a [\"Epic A\"]\n"));
        assert!(output.contains("        task-1[\"Task 1\"]\n"));
        assert!(output.contains("        task-2[\"Task 2\"]\n"));
        assert!(output.contains("    end\n"));
        // Edges follow the subgraph block.
        let end_at = output.find("    end\n").expect("end marker");
        let edge_at = output.find("    task-2 --> task-1\n").expect("edge");
        assert!(end_at < edge_at);
    }

    #[test]
    fn nested_groups_increase_indent() {
        let data = GraphData {
            field: "depends_on".into(),
            group_by: Some("parent".into()),
            nodes: vec![
                card("milestone", None),
                card("epic", None),
                card("task", None),
            ],
            edges: vec![],
            groups: Some(TreeData {
                field: "parent".into(),
                roots: vec![tree_node(
                    "milestone",
                    None,
                    vec![tree_node(
                        "epic",
                        None,
                        vec![tree_node("task", None, vec![])],
                    )],
                )],
            }),
        };
        let output = render_graph(&data, "");
        assert!(output.contains("    subgraph milestone [\"milestone\"]\n"));
        assert!(output.contains("        subgraph epic [\"epic\"]\n"));
        assert!(output.contains("            task[\"task\"]\n"));
    }

    #[test]
    fn label_sanitization_replaces_quote_and_collapses_newline() {
        let data = graph(
            "depends_on",
            vec![card("a", Some("has \"quotes\"\nand newline"))],
            vec![],
        );
        let output = render_graph(&data, "");
        assert!(output.contains("    a[\"has 'quotes' and newline\"]\n"));
    }

    #[test]
    fn antiparallel_and_self_loop_edges_emitted() {
        let data = graph(
            "depends_on",
            vec![card("a", None), card("b", None)],
            vec![edge("a", "b"), edge("b", "a"), edge("a", "a")],
        );
        let output = render_graph(&data, "");
        assert!(output.contains("    a --> b\n"));
        assert!(output.contains("    b --> a\n"));
        assert!(output.contains("    a --> a\n"));
    }

    #[test]
    fn edge_targeting_subgraph_id_renders() {
        let data = GraphData {
            field: "depends_on".into(),
            group_by: Some("parent".into()),
            nodes: vec![card("epic", Some("Epic")), card("loose", Some("Loose"))],
            edges: vec![edge("loose", "epic")],
            groups: Some(TreeData {
                field: "parent".into(),
                roots: vec![
                    tree_node(
                        "epic",
                        Some("Epic"),
                        vec![tree_node("inside", Some("Inside"), vec![])],
                    ),
                    tree_node("loose", Some("Loose"), vec![]),
                ],
            }),
        };
        let output = render_graph(&data, "");
        assert!(output.contains("    subgraph epic [\"Epic\"]\n"));
        assert!(output.contains("    loose[\"Loose\"]\n"));
        assert!(output.contains("    loose --> epic\n"));
    }
}
