//! Shared Mermaid `gantt` block formatting for Gantt-shaped renderers.
//!
//! `render_gantt`, `render_gantt_by_initiative`, `render_gantt_by_depth`
//! (and any future Gantt variants) build the same Mermaid `gantt` block
//! shape. The block builder lives here so each variant only owns its
//! outer document structure (heading, sub-headings, partition logic);
//! their unplaced blockquote summary comes from
//! `markdown::emit_unplaced_blockquote`.

use std::fmt::Write as _;

use workdown_core::view_data::{Card, GanttBar};

use super::markdown::no_value_label;

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
        None => no_value_label(group_field.unwrap_or("group")),
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
