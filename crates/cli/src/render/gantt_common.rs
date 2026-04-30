//! Shared formatting for Gantt-shaped Markdown renderers.
//!
//! `render_gantt` (basic) and `render_gantt_by_initiative` (and any future
//! Gantt variants) build the same Mermaid `gantt` block shape and the same
//! unplaced footer. The block builder lives here so each variant only owns
//! its outer document structure (heading, sub-headings, partition logic).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use workdown_core::view_data::{Card, GanttBar, UnplacedCard, UnplacedReason};

/// Render a Mermaid `gantt` block (fenced code block included) for a list
/// of bars, optionally split into `section <value>` blocks.
///
/// Precondition: `bars` is non-empty. Both callers (`render_gantt`,
/// `render_gantt_by_initiative`) check for emptiness and skip the block
/// entirely; an empty Mermaid block renders inconsistently across viewers
/// so this helper refuses to produce one.
///
/// When `group_field` is `Some`, bars are emitted in the order received
/// (the extractor sorted them by section already). The synthetic
/// "no value" section uses `(no <field>)`. When `None`, bars are emitted
/// flat with no `section` lines.
pub(crate) fn render_gantt_block(bars: &[GanttBar], group_field: Option<&str>) -> String {
    debug_assert!(!bars.is_empty(), "render_gantt_block called with no bars");

    let mut out = String::new();
    out.push_str("```mermaid\n");
    out.push_str("gantt\n");
    out.push_str("    dateFormat YYYY-MM-DD\n");

    let mut current_group: Option<&Option<String>> = None;
    for bar in bars {
        if group_field.is_some() && current_group != Some(&bar.group) {
            let heading = section_heading(&bar.group, group_field);
            let _ = writeln!(out, "    section {heading}");
            current_group = Some(&bar.group);
        }
        let _ = writeln!(
            out,
            "    {label} :{id}, {start}, {end}",
            label = label_for(&bar.card),
            id = bar.card.id,
            start = bar.start.format("%Y-%m-%d"),
            end = bar.end.format("%Y-%m-%d"),
        );
    }

    out.push_str("```\n");
    out
}

fn section_heading(group_value: &Option<String>, group_field: Option<&str>) -> String {
    match group_value {
        Some(value) => sanitize_label(value),
        None => format!("(no {})", group_field.unwrap_or("group")),
    }
}

/// Render a card's Mermaid task label with sanitization and id fallback.
pub(crate) fn label_for(card: &Card) -> String {
    let raw = card.title.as_deref().unwrap_or_else(|| card.id.as_str());
    let sanitized = sanitize_label(raw);
    if sanitized.is_empty() {
        card.id.as_str().to_owned()
    } else {
        sanitized
    }
}

/// Replace Mermaid-gantt-reserved characters (`:` `,` `#` `\n` `\r`) with
/// spaces, then collapse consecutive whitespace and trim. Predictable
/// and lossy by design — the offending characters can't survive in a
/// task line at all.
pub(crate) fn sanitize_label(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_was_space = false;
    for c in text.chars() {
        let mapped = match c {
            ':' | ',' | '#' | '\n' | '\r' | '\t' => ' ',
            other => other,
        };
        if mapped == ' ' {
            if !last_was_space && !out.is_empty() {
                out.push(' ');
                last_was_space = true;
            }
        } else {
            out.push(mapped);
            last_was_space = false;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

/// Emit the bulleted blockquote summary of items that filter-matched but
/// couldn't be rendered as bars.
///
/// Reasons are grouped by discriminant; `MissingValue` groups appear
/// alphabetically by field name, then `InvalidRange`, then any
/// `NonNumericValue` (theoretically possible but never produced by gantt
/// extractors today). Within each group, items follow the extractor's
/// id-sorted order.
pub(crate) fn render_unplaced_footer(unplaced: &[UnplacedCard], out: &mut String) {
    if unplaced.is_empty() {
        return;
    }

    let mut missing: BTreeMap<&str, Vec<&UnplacedCard>> = BTreeMap::new();
    let mut invalid_range: Vec<&UnplacedCard> = Vec::new();
    let mut non_numeric: BTreeMap<&str, Vec<&UnplacedCard>> = BTreeMap::new();
    let mut no_anchor: Vec<&UnplacedCard> = Vec::new();
    let mut predecessor_unresolved: BTreeMap<&str, Vec<&UnplacedCard>> = BTreeMap::new();
    let mut cycle: BTreeMap<&str, Vec<&UnplacedCard>> = BTreeMap::new();

    for unplaced_card in unplaced {
        match &unplaced_card.reason {
            UnplacedReason::MissingValue { field } => {
                missing
                    .entry(field.as_str())
                    .or_default()
                    .push(unplaced_card);
            }
            UnplacedReason::InvalidRange { .. } => {
                invalid_range.push(unplaced_card);
            }
            UnplacedReason::NonNumericValue { field, .. } => {
                non_numeric
                    .entry(field.as_str())
                    .or_default()
                    .push(unplaced_card);
            }
            UnplacedReason::NoAnchor => {
                no_anchor.push(unplaced_card);
            }
            UnplacedReason::PredecessorUnresolved { id } => {
                predecessor_unresolved
                    .entry(id.as_str())
                    .or_default()
                    .push(unplaced_card);
            }
            UnplacedReason::Cycle { via } => {
                cycle.entry(via.as_str()).or_default().push(unplaced_card);
            }
        }
    }

    out.push('\n');
    let _ = writeln!(out, "> _{} items dropped:_", unplaced.len());
    for (field, cards) in &missing {
        let _ = writeln!(out, "> _- missing '{field}': {}_", format_titles(cards));
    }
    if !invalid_range.is_empty() {
        let _ = writeln!(
            out,
            "> _- invalid range: {}_",
            format_titles(&invalid_range)
        );
    }
    for (field, cards) in &non_numeric {
        let _ = writeln!(out, "> _- non-numeric '{field}': {}_", format_titles(cards));
    }
    if !no_anchor.is_empty() {
        let _ = writeln!(out, "> _- no anchor: {}_", format_titles(&no_anchor));
    }
    for (id, cards) in &predecessor_unresolved {
        let _ = writeln!(
            out,
            "> _- predecessor '{id}' unresolved: {}_",
            format_titles(cards)
        );
    }
    for (via, cards) in &cycle {
        let _ = writeln!(out, "> _- cycle in '{via}': {}_", format_titles(cards));
    }
}

fn format_titles(cards: &[&UnplacedCard]) -> String {
    cards
        .iter()
        .map(|c| {
            let name = c
                .card
                .title
                .as_deref()
                .unwrap_or_else(|| c.card.id.as_str());
            format!("\"{}\"", escape_blockquote_italic(name))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Escape `_` so a title doesn't accidentally close the surrounding
/// italic markers in a blockquote line.
fn escape_blockquote_italic(text: &str) -> String {
    text.replace('_', r"\_")
}
