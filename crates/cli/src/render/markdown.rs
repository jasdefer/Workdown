//! Shared Markdown primitives used across every renderer.
//!
//! Link emission, structural escapes (link text, table cell, blockquote
//! italic), description emission, numeric formatting, the wording for
//! a view's synthetic "no value" bucket, and the two unplaced-item
//! conventions (detailed `## Unplaced` section, compact blockquote
//! summary). Kept deliberately small: only primitives that more than
//! one renderer needs. Renderer-specific formatting stays in its own
//! module.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use workdown_core::model::views::Aggregate;
use workdown_core::view_data::{Card, UnplacedCard, UnplacedReason};

/// Render a work item as a Markdown link: `[title-or-id](base/id.md)`.
///
/// No bullet and no trailing newline — the caller decides indentation and
/// line structure. `item_link_base` is the path from the rendered view
/// file to the work items directory (e.g. `"../workdown-items"`).
pub fn item_link(id: &str, title: Option<&str>, item_link_base: &str) -> String {
    let link_text = title.unwrap_or(id);
    let escaped = escape_link_text(link_text);
    format!("[{escaped}]({item_link_base}/{id}.md)")
}

/// [`item_link`] for a [`Card`]-carrying payload.
pub fn card_link(card: &Card, item_link_base: &str) -> String {
    item_link(card.id.as_str(), card.title.as_deref(), item_link_base)
}

/// Render a bare work item id as a Markdown link: `[id](base/id.md)`.
///
/// Used by renderers that have only an id and no `Card` to lean on (the
/// table renderer's `id` column, `Link`/`Links` cells). Workdown ids are
/// validated to `[a-z0-9][a-z0-9-]*`, so the link text needs no escaping.
pub fn id_link(id: &str, item_link_base: &str) -> String {
    format!("[{id}]({item_link_base}/{id}.md)")
}

/// Name the synthetic "no value" bucket a grouped view produces —
/// legends, gantt sections, and anything else that labels it inline.
///
/// The extractor reports that bucket as a structural `None` and never
/// names it (ADR-006: ViewData owns structure and order, renderers own
/// wording). Every terminal renderer routes through here so board,
/// gantt, and the line chart can't drift apart on the wording again.
/// The web front end has its own equivalent in `views/format.ts`;
/// the two differ only in that the web prettifies field names, as it
/// does everywhere it displays one.
pub fn no_value_label(field: &str) -> String {
    format!("(no {field})")
}

/// [`no_value_label`] in heading position — no surrounding parentheses,
/// leading capital, for renderers that give the bucket its own `##`
/// section rather than an inline label.
///
/// Paired with `no_value_label` in one place so the two forms stay a
/// deliberate choice about position rather than an accident.
pub fn no_value_heading(field: &str) -> String {
    format!("No {field}")
}

/// Emit a one-line view description below the `# Heading`.
///
/// Renderers receive a description string from the dispatcher (built by
/// [`super::description::description_for`]). Empty strings — currently
/// only used for view kinds without a description — produce no output,
/// keeping the rendered file flush against its content.
pub fn emit_description(description: &str, out: &mut String) {
    if !description.is_empty() {
        out.push_str(description);
        out.push_str("\n\n");
    }
}

/// Render an integer-valued f64 without a trailing `.0`.
///
/// Counts and integer sums round-trip through f64 but should display as
/// `12`, not `12.0`. Non-integer floats keep their default precision.
/// Used by renderers that surface arithmetic-derived numbers (metric
/// values, treemap sizes, etc.) — raw `FieldValue::Float` rendering
/// uses Rust's default `f64::to_string()` and doesn't need this.
pub fn format_number(n: f64) -> String {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        n.to_string()
    }
}

/// Escape characters that would break Markdown link-text parsing.
///
/// CommonMark terminates link text at unbalanced `]`, and a literal `\`
/// before a bracket needs its own escape to remain literal. Other
/// characters (parens, backticks, pipes, …) are fine inside link text.
pub fn escape_link_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '\\' | '[' | ']' => {
                out.push('\\');
                out.push(character);
            }
            _ => out.push(character),
        }
    }
    out
}

/// Neutralize the characters that would break a GFM table cell:
/// `|` ends the cell early, and a literal newline ends the row. Pipes
/// become `\|` (GFM-recognized) and newlines become `<br>`. Lone `\r`
/// is dropped so `\r\n` collapses to one `<br>`.
pub fn escape_cell(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '|' => out.push_str(r"\|"),
            '\n' => out.push_str("<br>"),
            '\r' => {}
            other => out.push(other),
        }
    }
    out
}

/// Escape `_` so a label or title doesn't accidentally close the
/// surrounding italic markers in a blockquote line. Used by renderers
/// that emit `> _… "<title>" …_` footers (gantt, metric, workload).
pub fn escape_blockquote_italic(text: &str) -> String {
    text.replace('_', r"\_")
}

/// `1 item` / `3 items` — count plus noun with a plural `s` past one.
/// Mirrors the web front end's `pluralize` in `views/format.ts`.
pub fn pluralize(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

/// The label naming what an aggregating view computed: `count`,
/// `sum of estimate`, or the bare aggregate when no value field is
/// configured. One phrase feeding the H1, the values-table header, and
/// the SVG axis title so the three can't drift apart.
pub fn aggregate_label(aggregate: Aggregate, value_field: Option<&str>) -> String {
    match aggregate {
        Aggregate::Count => "count".to_owned(),
        aggregate => match value_field {
            Some(value_field) => format!("{aggregate} of {value_field}"),
            None => format!("{aggregate}"),
        },
    }
}

// ── Unplaced items ──────────────────────────────────────────────────

/// The one place an [`UnplacedReason`] is put into words. Both unplaced
/// conventions (the `## Unplaced` section and the blockquote summary)
/// route through here, so a new variant is worded exactly once and the
/// two conventions can't drift apart on phrasing.
pub fn unplaced_reason_phrase(reason: &UnplacedReason) -> String {
    match reason {
        UnplacedReason::MissingValue { field } => format!("missing `{field}`"),
        UnplacedReason::InvalidRange {
            start_field,
            end_field,
        } => format!("start `{start_field}` after end `{end_field}`"),
        UnplacedReason::NoWorkingDays {
            start_field,
            end_field,
        } => format!("interval `{start_field}..{end_field}` falls entirely on non-working days"),
        UnplacedReason::NonNumericValue { field, .. } => format!("non-numeric `{field}`"),
        UnplacedReason::NoAnchor => "no anchor".to_owned(),
        UnplacedReason::PredecessorUnresolved { id } => format!("predecessor `{id}` unresolved"),
        UnplacedReason::Cycle { via } => format!("cycle in `{via}`"),
    }
}

/// Emit the chart family's unplaced convention: a `## Unplaced` section
/// with one linked bullet per item, reason inline. Emits nothing when
/// every item was placed.
///
/// The match over reasons lives in [`unplaced_reason_phrase`] and is
/// exhaustive — which reasons actually occur in a given view is the
/// extractor's knowledge, and the renderer displays whatever arrives.
pub fn emit_unplaced_section(unplaced: &[UnplacedCard], item_link_base: &str, out: &mut String) {
    if unplaced.is_empty() {
        return;
    }
    out.push_str("## Unplaced\n");
    for unplaced_card in unplaced {
        let link = card_link(&unplaced_card.card, item_link_base);
        let phrase = unplaced_reason_phrase(&unplaced_card.reason);
        let _ = writeln!(out, "- {link} — {phrase}");
    }
}

/// Emit the gantt family's unplaced convention: a compact blockquote
/// summary, one line per reason phrase with the affected titles. Groups
/// appear alphabetically by phrase (see [`group_unplaced_by_phrase`]);
/// items inside a group keep the extractor's id-sorted order. Emits
/// nothing when every item was placed.
pub fn emit_unplaced_blockquote(unplaced: &[UnplacedCard], out: &mut String) {
    if unplaced.is_empty() {
        return;
    }
    out.push('\n');
    let _ = writeln!(out, "> _{} dropped:_", pluralize(unplaced.len(), "item"));
    for (phrase, cards) in group_unplaced_by_phrase(unplaced) {
        let _ = writeln!(
            out,
            "> _- {phrase}: {titles}_",
            phrase = escape_blockquote_italic(&phrase),
            titles = format_quoted_titles(&cards),
        );
    }
}

/// Bucket unplaced cards by their reason phrase — the grouping the
/// blockquote summary renders. Keying on the generated phrase (rather
/// than hand-written per-variant buckets) means a new [`UnplacedReason`]
/// variant needs no edit here; the `BTreeMap` orders groups
/// alphabetically by phrase.
pub fn group_unplaced_by_phrase(unplaced: &[UnplacedCard]) -> BTreeMap<String, Vec<&UnplacedCard>> {
    let mut grouped: BTreeMap<String, Vec<&UnplacedCard>> = BTreeMap::new();
    for unplaced_card in unplaced {
        grouped
            .entry(unplaced_reason_phrase(&unplaced_card.reason))
            .or_default()
            .push(unplaced_card);
    }
    grouped
}

/// Comma-joined, quoted item titles (id when the title is absent) for
/// blockquote-italic footers: `"Fix login", "Add tests"`.
pub fn format_quoted_titles(cards: &[&UnplacedCard]) -> String {
    cards
        .iter()
        .map(|unplaced_card| {
            let name = unplaced_card
                .card
                .title
                .as_deref()
                .unwrap_or_else(|| unplaced_card.card.id.as_str());
            format!("\"{}\"", escape_blockquote_italic(name))
        })
        .collect::<Vec<_>>()
        .join(", ")
}
