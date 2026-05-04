//! Treemap renderer — turns [`TreemapData`] into a Markdown document.
//!
//! Output shape: a top-level `# Treemap: <size> by <group>` heading, an
//! optional one-line description, a `**Total: <size>**` line summarizing
//! the synthetic root, then a nested bullet list. Each line leads with
//! the rolled-up size, an optional `(N%)` share-of-parent annotation,
//! and an em-dash followed by the linked title. Children of every node
//! sort by size descending; ties break by id ascending. Items that
//! filter-matched but lack the size field appear in a trailing
//! `## Unplaced (missing <field>)` section. An empty view (no roots, no
//! unplaced) emits the heading, the description, and `_(no items)_`.

use std::cmp::Ordering;
use std::fmt::Write as _;

use workdown_core::model::duration::format_duration_seconds;
use workdown_core::view_data::{SizeValue, TreemapData, TreemapNode};

use crate::render::common::{card_link, emit_description, format_number, id_link};

/// Render a `TreemapData` as a Markdown string.
///
/// `item_link_base` is the relative path from the rendered view file to
/// the work items directory — same parameter as `render::board::render_board`.
/// `description` is the one-line caption emitted below the heading.
pub fn render_treemap(data: &TreemapData, item_link_base: &str, description: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# Treemap: {size} by {group}\n\n",
        size = data.size_field,
        group = data.group_field,
    ));
    emit_description(description, &mut out);

    if data.root.children.is_empty() && data.unplaced.is_empty() {
        out.push_str("_(no items)_\n");
        return out;
    }

    let _ = writeln!(out, "**Total: {}**", format_size(data.root.size));
    out.push('\n');

    let parent_size = data.root.size.as_f64();
    let sorted = sorted_children(&data.root.children);
    for child in &sorted {
        render_node(child, parent_size, 0, item_link_base, &mut out);
    }

    if !data.unplaced.is_empty() {
        out.push('\n');
        let _ = writeln!(
            out,
            "## Unplaced (missing `{field}`)",
            field = data.size_field,
        );
        for unplaced in &data.unplaced {
            let _ = writeln!(out, "- {}", card_link(&unplaced.card, item_link_base));
        }
    }

    out
}

fn render_node(
    node: &TreemapNode,
    parent_size: f64,
    depth: usize,
    item_link_base: &str,
    out: &mut String,
) {
    let indent = "  ".repeat(depth);
    let size_str = format_size(node.size);
    let percent = percent_of_parent(node.size.as_f64(), parent_size);
    let link = match &node.card {
        Some(card) => card_link(card, item_link_base),
        // Synthetic-root nodes shouldn't reach here (we only iterate
        // `data.root.children`), but degrade gracefully to the id link
        // form rather than panicking if a future caller bypasses that.
        None => id_link("(unnamed)", item_link_base),
    };
    match percent {
        Some(percent) => {
            let _ = writeln!(out, "{indent}- **{size_str}** ({percent}%) — {link}");
        }
        None => {
            let _ = writeln!(out, "{indent}- **{size_str}** — {link}");
        }
    }

    let child_parent = node.size.as_f64();
    let sorted = sorted_children(&node.children);
    for child in &sorted {
        render_node(child, child_parent, depth + 1, item_link_base, out);
    }
}

/// Sort children by size descending; ties break by id ascending so
/// snapshot tests and rendered output are deterministic.
fn sorted_children(children: &[TreemapNode]) -> Vec<&TreemapNode> {
    let mut refs: Vec<&TreemapNode> = children.iter().collect();
    refs.sort_by(|left, right| {
        right
            .size
            .as_f64()
            .partial_cmp(&left.size.as_f64())
            .unwrap_or(Ordering::Equal)
            .then_with(|| {
                let left_id = left.card.as_ref().map(|c| c.id.as_str()).unwrap_or("");
                let right_id = right.card.as_ref().map(|c| c.id.as_str()).unwrap_or("");
                left_id.cmp(right_id)
            })
    });
    refs
}

/// Compute child's share of its parent as an integer percent.
///
/// Returns `None` when the parent is zero — there's no meaningful
/// proportion to show (and dividing would NaN/inf). Renderers omit the
/// `(N%)` segment in that case.
fn percent_of_parent(child: f64, parent: f64) -> Option<i64> {
    if parent.abs() < f64::EPSILON {
        None
    } else {
        Some((child / parent * 100.0).round() as i64)
    }
}

fn format_size(size: SizeValue) -> String {
    match size {
        SizeValue::Number(number) => format_number(number),
        SizeValue::Duration(seconds) => format_duration_seconds(seconds),
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use workdown_core::model::WorkItemId;
    use workdown_core::view_data::{Card, TreemapData, TreemapNode, UnplacedCard, UnplacedReason};

    fn card(id: &str, title: Option<&str>) -> Card {
        Card {
            id: WorkItemId::from(id.to_owned()),
            title: title.map(str::to_owned),
            fields: vec![],
            body: String::new(),
        }
    }

    fn leaf(id: &str, title: Option<&str>, size: SizeValue) -> TreemapNode {
        TreemapNode {
            card: Some(card(id, title)),
            size,
            children: vec![],
        }
    }

    fn branch(
        id: &str,
        title: Option<&str>,
        size: SizeValue,
        children: Vec<TreemapNode>,
    ) -> TreemapNode {
        TreemapNode {
            card: Some(card(id, title)),
            size,
            children,
        }
    }

    fn data(size_field: &str, root: TreemapNode, unplaced: Vec<UnplacedCard>) -> TreemapData {
        TreemapData {
            group_field: "parent".to_owned(),
            size_field: size_field.to_owned(),
            root,
            unplaced,
        }
    }

    fn empty_root(size: SizeValue) -> TreemapNode {
        TreemapNode {
            card: None,
            size,
            children: vec![],
        }
    }

    fn synthetic_root(size: SizeValue, children: Vec<TreemapNode>) -> TreemapNode {
        TreemapNode {
            card: None,
            size,
            children,
        }
    }

    fn unplaced(id: &str, title: Option<&str>, field: &str) -> UnplacedCard {
        UnplacedCard {
            card: card(id, title),
            reason: UnplacedReason::MissingValue {
                field: field.to_owned(),
            },
        }
    }

    #[test]
    fn renders_top_heading_with_size_and_group() {
        let output = render_treemap(
            &data("effort", empty_root(SizeValue::Number(0.0)), vec![]),
            "../workdown-items",
            "",
        );
        assert!(output.starts_with("# Treemap: effort by parent\n"));
    }

    #[test]
    fn empty_view_emits_no_items_marker() {
        let output = render_treemap(
            &data("effort", empty_root(SizeValue::Number(0.0)), vec![]),
            "../workdown-items",
            "",
        );
        assert!(output.contains("_(no items)_\n"));
        assert!(!output.contains("Total:"));
    }

    #[test]
    fn description_emitted_under_heading() {
        let output = render_treemap(
            &data("effort", empty_root(SizeValue::Number(0.0)), vec![]),
            "../workdown-items",
            "Hierarchical breakdown of `effort` summed up the `parent` chain.",
        );
        assert!(output.contains(
            "# Treemap: effort by parent\n\nHierarchical breakdown of `effort` summed up the `parent` chain.\n\n"
        ));
    }

    #[test]
    fn total_line_appears_above_bullets() {
        let root = synthetic_root(
            SizeValue::Number(8.0),
            vec![branch(
                "root",
                None,
                SizeValue::Number(8.0),
                vec![
                    leaf("a", None, SizeValue::Number(3.0)),
                    leaf("b", None, SizeValue::Number(5.0)),
                ],
            )],
        );
        let output = render_treemap(&data("effort", root, vec![]), "../workdown-items", "");
        let total_at = output.find("**Total: 8**").expect("total line");
        let first_bullet = output.find("- **").expect("first bullet");
        assert!(total_at < first_bullet);
    }

    #[test]
    fn integer_size_drops_decimal() {
        let root = synthetic_root(
            SizeValue::Number(12.0),
            vec![leaf("a", None, SizeValue::Number(12.0))],
        );
        let output = render_treemap(&data("effort", root, vec![]), "../workdown-items", "");
        assert!(output.contains("**Total: 12**"));
        assert!(output.contains("- **12**"));
        assert!(!output.contains("12.0"));
    }

    #[test]
    fn duration_size_uses_duration_formatter() {
        let root = synthetic_root(
            SizeValue::Duration(5400),
            vec![branch(
                "root",
                None,
                SizeValue::Duration(5400),
                vec![
                    leaf("a", None, SizeValue::Duration(3600)),
                    leaf("b", None, SizeValue::Duration(1800)),
                ],
            )],
        );
        let output = render_treemap(&data("estimate", root, vec![]), "../workdown-items", "");
        assert!(output.contains("**Total: 1h 30min**"));
        assert!(output.contains("- **1h**"));
        assert!(output.contains("- **30min**"));
    }

    #[test]
    fn children_sort_by_size_desc() {
        let root = synthetic_root(
            SizeValue::Number(10.0),
            vec![
                leaf("a", None, SizeValue::Number(2.0)),
                leaf("b", None, SizeValue::Number(5.0)),
                leaf("c", None, SizeValue::Number(3.0)),
            ],
        );
        let output = render_treemap(&data("effort", root, vec![]), "../workdown-items", "");
        let b_at = output.find("/b.md").expect("b link");
        let c_at = output.find("/c.md").expect("c link");
        let a_at = output.find("/a.md").expect("a link");
        assert!(b_at < c_at, "5 should appear before 3");
        assert!(c_at < a_at, "3 should appear before 2");
    }

    #[test]
    fn ties_on_size_broken_by_id_asc() {
        let root = synthetic_root(
            SizeValue::Number(9.0),
            vec![
                leaf("c", None, SizeValue::Number(3.0)),
                leaf("a", None, SizeValue::Number(3.0)),
                leaf("b", None, SizeValue::Number(3.0)),
            ],
        );
        let output = render_treemap(&data("effort", root, vec![]), "../workdown-items", "");
        let a_at = output.find("/a.md").expect("a link");
        let b_at = output.find("/b.md").expect("b link");
        let c_at = output.find("/c.md").expect("c link");
        assert!(a_at < b_at);
        assert!(b_at < c_at);
    }

    #[test]
    fn percent_of_parent_renders_inline() {
        // root=10, a=4 (40% of root), b=6 (60% of root). a has child x=4 (100% of a).
        let root = synthetic_root(
            SizeValue::Number(10.0),
            vec![
                branch(
                    "a",
                    None,
                    SizeValue::Number(4.0),
                    vec![leaf("x", None, SizeValue::Number(4.0))],
                ),
                leaf("b", None, SizeValue::Number(6.0)),
            ],
        );
        let output = render_treemap(&data("effort", root, vec![]), "../workdown-items", "");
        assert!(output.contains("- **6** (60%)"));
        assert!(output.contains("- **4** (40%)"));
        assert!(output.contains("- **4** (100%)"));
    }

    #[test]
    fn percent_omitted_when_parent_is_zero() {
        let root = synthetic_root(
            SizeValue::Number(0.0),
            vec![leaf("a", None, SizeValue::Number(0.0))],
        );
        let output = render_treemap(&data("effort", root, vec![]), "../workdown-items", "");
        // No percentage on the bullet — but the bullet still appears.
        assert!(output.contains("- **0** — "));
        assert!(!output.contains("(0%)"));
    }

    #[test]
    fn unplaced_section_appears_when_items_dropped() {
        let root = synthetic_root(
            SizeValue::Number(4.0),
            vec![leaf("present", Some("Present"), SizeValue::Number(4.0))],
        );
        let output = render_treemap(
            &data(
                "effort",
                root,
                vec![
                    unplaced("missing-1", Some("First missing"), "effort"),
                    unplaced("missing-2", None, "effort"),
                ],
            ),
            "../workdown-items",
            "",
        );
        assert!(output.contains("## Unplaced (missing `effort`)\n"));
        assert!(output.contains("- [First missing](../workdown-items/missing-1.md)"));
        assert!(output.contains("- [missing-2](../workdown-items/missing-2.md)"));
    }

    #[test]
    fn no_unplaced_section_when_clean() {
        let root = synthetic_root(
            SizeValue::Number(4.0),
            vec![leaf("a", None, SizeValue::Number(4.0))],
        );
        let output = render_treemap(&data("effort", root, vec![]), "../workdown-items", "");
        assert!(!output.contains("Unplaced"));
    }

    #[test]
    fn full_output_snapshot() {
        // Two top-level roots, one with a nested child.
        let root = synthetic_root(
            SizeValue::Number(10.0),
            vec![
                branch(
                    "alpha",
                    Some("Alpha"),
                    SizeValue::Number(7.0),
                    vec![
                        leaf("alpha-1", Some("Alpha 1"), SizeValue::Number(4.0)),
                        leaf("alpha-2", Some("Alpha 2"), SizeValue::Number(3.0)),
                    ],
                ),
                leaf("beta", Some("Beta"), SizeValue::Number(3.0)),
            ],
        );
        let output = render_treemap(
            &data(
                "effort",
                root,
                vec![unplaced("orphan", Some("Orphan"), "effort")],
            ),
            "../workdown-items",
            "Hierarchical breakdown of `effort` summed up the `parent` chain.",
        );
        let expected = "\
# Treemap: effort by parent

Hierarchical breakdown of `effort` summed up the `parent` chain.

**Total: 10**

- **7** (70%) — [Alpha](../workdown-items/alpha.md)
  - **4** (57%) — [Alpha 1](../workdown-items/alpha-1.md)
  - **3** (43%) — [Alpha 2](../workdown-items/alpha-2.md)
- **3** (30%) — [Beta](../workdown-items/beta.md)

## Unplaced (missing `effort`)
- [Orphan](../workdown-items/orphan.md)
";
        assert_eq!(output, expected);
    }
}
