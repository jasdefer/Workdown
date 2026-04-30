//! Gantt renderer — turns [`GanttData`] into a Markdown document with a
//! Mermaid `gantt` block.
//!
//! Output shape: a top-level `# Gantt` heading, then a Mermaid `gantt`
//! code block. When the view has `group:` set, items partition into
//! `section <value>` blocks in extractor-determined order; the synthetic
//! "no value" section appears last as `(no <field>)`. An empty view
//! renders the heading only — Mermaid handles empty `gantt` blocks
//! inconsistently across viewers, so the safe shape is to omit the
//! block entirely. Filter-matched items that couldn't be placed (missing
//! dates, inverted range) are summarized in a blockquote footer below
//! the chart.
//!
//! Mermaid task syntax is `<title> :<id>, <start>, <end>` where `:` `,`
//! `#` and newlines are reserved separators; titles are sanitized to
//! drop those. Workdown ids match `[a-z0-9][a-z0-9-]*` and pass through
//! unchanged.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use workdown_core::view_data::{Card, GanttData, UnplacedCard, UnplacedReason};

/// Render a `GanttData` as a Markdown string.
pub fn render_gantt(data: &GanttData) -> String {
    let mut out = String::new();
    out.push_str("# Gantt\n\n");

    if data.bars.is_empty() {
        render_unplaced_footer(&data.unplaced, &mut out);
        return out;
    }

    out.push_str("```mermaid\n");
    out.push_str("gantt\n");
    out.push_str("    dateFormat YYYY-MM-DD\n");

    let mut current_group: Option<&Option<String>> = None;
    for bar in &data.bars {
        if data.group_field.is_some() && current_group != Some(&bar.group) {
            let heading = section_heading(&bar.group, data.group_field.as_deref());
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
    render_unplaced_footer(&data.unplaced, &mut out);
    out
}

fn section_heading(group_value: &Option<String>, group_field: Option<&str>) -> String {
    match group_value {
        Some(value) => sanitize_label(value),
        None => format!("(no {})", group_field.unwrap_or("group")),
    }
}

fn label_for(card: &Card) -> String {
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
fn sanitize_label(text: &str) -> String {
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
/// `NonNumericValue` (theoretically possible but never produced by the
/// gantt extractor today). Within each group, items follow the
/// extractor's id-sorted order.
fn render_unplaced_footer(unplaced: &[UnplacedCard], out: &mut String) {
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

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use workdown_core::model::WorkItemId;
    use workdown_core::view_data::{Card, GanttBar, GanttData, UnplacedCard, UnplacedReason};

    fn ymd(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    fn card(id: &str, title: Option<&str>) -> Card {
        Card {
            id: WorkItemId::from(id.to_owned()),
            title: title.map(str::to_owned),
            fields: vec![],
            body: String::new(),
        }
    }

    fn bar(id: &str, title: Option<&str>, start: NaiveDate, end: NaiveDate) -> GanttBar {
        GanttBar {
            card: card(id, title),
            start,
            end,
            group: None,
        }
    }

    fn grouped_bar(
        id: &str,
        title: Option<&str>,
        start: NaiveDate,
        end: NaiveDate,
        group: &str,
    ) -> GanttBar {
        GanttBar {
            card: card(id, title),
            start,
            end,
            group: Some(group.to_owned()),
        }
    }

    fn data(
        bars: Vec<GanttBar>,
        group_field: Option<&str>,
        unplaced: Vec<UnplacedCard>,
    ) -> GanttData {
        GanttData {
            group_field: group_field.map(str::to_owned),
            bars,
            unplaced,
        }
    }

    #[test]
    fn empty_bars_emits_heading_only_no_mermaid_block() {
        let output = render_gantt(&data(vec![], None, vec![]));
        assert_eq!(output, "# Gantt\n\n");
        assert!(!output.contains("```mermaid"));
    }

    #[test]
    fn single_bar_no_group_emits_no_section_lines() {
        let output = render_gantt(&data(
            vec![bar("a", Some("Task A"), ymd(2026, 1, 1), ymd(2026, 1, 5))],
            None,
            vec![],
        ));
        assert!(output.contains("```mermaid\ngantt\n    dateFormat YYYY-MM-DD\n"));
        assert!(output.contains("    Task A :a, 2026-01-01, 2026-01-05\n"));
        assert!(!output.contains("section"));
        assert!(output.trim_end().ends_with("```"));
    }

    #[test]
    fn bars_with_group_emit_section_lines_in_order() {
        let d1 = ymd(2026, 1, 1);
        let d2 = ymd(2026, 1, 5);
        let output = render_gantt(&data(
            vec![
                grouped_bar("c", None, d1, d2, "ops"),
                grouped_bar("a", None, d1, d2, "eng"),
                grouped_bar("b", None, d1, d2, "eng"),
            ],
            Some("team"),
            vec![],
        ));
        let ops_at = output.find("    section ops\n").expect("ops section");
        let eng_at = output.find("    section eng\n").expect("eng section");
        let bar_c_at = output.find("c, 2026-01-01").expect("c bar");
        let bar_a_at = output.find("a, 2026-01-01").expect("a bar");
        let bar_b_at = output.find("b, 2026-01-01").expect("b bar");
        assert!(ops_at < bar_c_at);
        assert!(bar_c_at < eng_at);
        assert!(eng_at < bar_a_at);
        assert!(bar_a_at < bar_b_at);
    }

    #[test]
    fn missing_group_value_renders_no_field_section() {
        let d1 = ymd(2026, 1, 1);
        let d2 = ymd(2026, 1, 5);
        let output = render_gantt(&data(
            vec![
                grouped_bar("a", None, d1, d2, "eng"),
                bar("b", None, d1, d2),
            ],
            Some("team"),
            vec![],
        ));
        assert!(output.contains("    section eng\n"));
        assert!(output.contains("    section (no team)\n"));
        let eng_at = output.find("section eng\n").unwrap();
        let no_team_at = output.find("section (no team)\n").unwrap();
        assert!(eng_at < no_team_at);
    }

    #[test]
    fn unplaced_footer_groups_by_reason_with_titles() {
        let d1 = ymd(2026, 1, 1);
        let d2 = ymd(2026, 1, 5);
        let unplaced = vec![
            UnplacedCard {
                card: card("a", Some("Title A")),
                reason: UnplacedReason::MissingValue {
                    field: "start".into(),
                },
            },
            UnplacedCard {
                card: card("b", Some("Title B")),
                reason: UnplacedReason::MissingValue {
                    field: "start".into(),
                },
            },
            UnplacedCard {
                card: card("c", Some("Title C")),
                reason: UnplacedReason::MissingValue {
                    field: "end".into(),
                },
            },
            UnplacedCard {
                card: card("d", Some("Title D")),
                reason: UnplacedReason::InvalidRange {
                    start_field: "start".into(),
                    end_field: "end".into(),
                },
            },
        ];
        let output = render_gantt(&data(vec![bar("ok", Some("Ok"), d1, d2)], None, unplaced));
        assert!(output.contains("> _4 items dropped:_\n"));
        assert!(output.contains("> _- missing 'end': \"Title C\"_\n"));
        assert!(output.contains("> _- missing 'start': \"Title A\", \"Title B\"_\n"));
        assert!(output.contains("> _- invalid range: \"Title D\"_\n"));
        let end_at = output.find("missing 'end'").unwrap();
        let start_at = output.find("missing 'start'").unwrap();
        let invalid_at = output.find("invalid range").unwrap();
        assert!(end_at < start_at);
        assert!(start_at < invalid_at);
    }

    #[test]
    fn unplaced_footer_falls_back_to_id_when_title_missing() {
        let unplaced = vec![UnplacedCard {
            card: card("orphan", None),
            reason: UnplacedReason::MissingValue {
                field: "start".into(),
            },
        }];
        let output = render_gantt(&data(vec![], None, unplaced));
        assert!(output.contains("> _- missing 'start': \"orphan\"_\n"));
    }

    #[test]
    fn empty_bars_with_unplaced_emits_heading_and_footer_no_block() {
        let unplaced = vec![UnplacedCard {
            card: card("a", Some("A")),
            reason: UnplacedReason::MissingValue {
                field: "start".into(),
            },
        }];
        let output = render_gantt(&data(vec![], None, unplaced));
        assert!(output.starts_with("# Gantt\n\n"));
        assert!(!output.contains("```mermaid"));
        assert!(output.contains("> _1 items dropped:_\n"));
    }

    #[test]
    fn title_with_colon_comma_hash_is_sanitized_to_spaces() {
        let output = render_gantt(&data(
            vec![bar(
                "a",
                Some("Fix bug: login, urgent #priority"),
                ymd(2026, 1, 1),
                ymd(2026, 1, 5),
            )],
            None,
            vec![],
        ));
        assert!(output.contains("    Fix bug login urgent priority :a, 2026-01-01, 2026-01-05\n"));
    }

    #[test]
    fn crlf_in_title_collapses_to_single_space() {
        let output = render_gantt(&data(
            vec![bar(
                "a",
                Some("line one\r\nline two"),
                ymd(2026, 1, 1),
                ymd(2026, 1, 5),
            )],
            None,
            vec![],
        ));
        assert!(output.contains("    line one line two :a, 2026-01-01, 2026-01-05\n"));
    }

    #[test]
    fn bar_falls_back_to_id_when_title_missing() {
        let output = render_gantt(&data(
            vec![bar("plain-task", None, ymd(2026, 1, 1), ymd(2026, 1, 5))],
            None,
            vec![],
        ));
        assert!(output.contains("    plain-task :plain-task, 2026-01-01, 2026-01-05\n"));
    }

    #[test]
    fn title_underscore_escaped_in_unplaced_footer() {
        let unplaced = vec![UnplacedCard {
            card: card("a", Some("snake_case_title")),
            reason: UnplacedReason::MissingValue {
                field: "start".into(),
            },
        }];
        let output = render_gantt(&data(vec![], None, unplaced));
        assert!(output.contains(r#"> _- missing 'start': "snake\_case\_title"_"#));
    }

    #[test]
    fn full_output_snapshot_with_sections_and_unplaced() {
        let d1 = ymd(2026, 1, 1);
        let d2 = ymd(2026, 1, 5);
        let bars = vec![
            grouped_bar("a", Some("Alpha"), d1, d2, "eng"),
            grouped_bar("b", Some("Beta"), d1, d2, "ops"),
        ];
        let unplaced = vec![UnplacedCard {
            card: card("z", Some("Zeta")),
            reason: UnplacedReason::InvalidRange {
                start_field: "start".into(),
                end_field: "end".into(),
            },
        }];
        let output = render_gantt(&data(bars, Some("team"), unplaced));
        let expected = "# Gantt\n\n\
            ```mermaid\n\
            gantt\n    \
            dateFormat YYYY-MM-DD\n    \
            section eng\n    \
            Alpha :a, 2026-01-01, 2026-01-05\n    \
            section ops\n    \
            Beta :b, 2026-01-01, 2026-01-05\n\
            ```\n\n\
            > _1 items dropped:_\n\
            > _- invalid range: \"Zeta\"_\n";
        assert_eq!(output, expected);
    }
}
