//! Gantt-by-initiative renderer — turns [`GanttByInitiativeData`] into a
//! Markdown document with one Mermaid `gantt` block per initiative.
//!
//! Output shape: a top-level `# Gantt by initiative` heading, then one
//! `## <root title>` subheading + Mermaid block per initiative. Empty
//! initiatives are skipped — no chart, no heading. Filter-matched items
//! that couldn't be placed (missing dates, inverted range, etc.) appear
//! in a single global blockquote footer at the document bottom, same
//! shape as basic Gantt.
//!
//! No per-initiative section grouping: each chart is already scoped to
//! one initiative, so the inner block is flat. Block formatting,
//! sanitization, and footer reuse [`super::mermaid_gantt`].

use workdown_core::view_data::GanttByInitiativeData;

use super::mermaid_gantt::{label_for, render_gantt_block, render_unplaced_footer};
use crate::render::markdown::emit_description;

/// Render a `GanttByInitiativeData` as a Markdown string.
///
/// `description` is the one-line caption emitted below the heading.
pub fn render_gantt_by_initiative(data: &GanttByInitiativeData, description: &str) -> String {
    let mut out = String::new();
    out.push_str("# Gantt by initiative\n\n");
    emit_description(description, &mut out);

    let mut first = true;
    for initiative in &data.initiatives {
        if initiative.bars.is_empty() {
            continue;
        }
        if !first {
            out.push('\n');
        }
        first = false;
        out.push_str(&format!("## {}\n\n", label_for(&initiative.root)));
        out.push_str(&render_gantt_block(&initiative.bars, None));
    }

    render_unplaced_footer(&data.unplaced, &mut out);
    out
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::test_fixtures::card;
    use super::*;
    use chrono::NaiveDate;
    use workdown_core::view_data::{
        GanttBar, GanttByInitiativeData, Initiative, UnplacedCard, UnplacedReason,
    };

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn bar(id: &str, title: Option<&str>, start: NaiveDate, end: NaiveDate) -> GanttBar {
        GanttBar {
            card: card(id, title),
            start,
            end,
            group: None,
        }
    }

    fn data(initiatives: Vec<Initiative>, unplaced: Vec<UnplacedCard>) -> GanttByInitiativeData {
        GanttByInitiativeData {
            initiatives,
            unplaced,
        }
    }

    #[test]
    fn empty_data_emits_heading_only() {
        let output = render_gantt_by_initiative(&data(vec![], vec![]), "");
        assert_eq!(output, "# Gantt by initiative\n\n");
    }

    #[test]
    fn single_initiative_emits_subheading_and_one_block() {
        let init = Initiative {
            root: card("epic", Some("User Auth Epic")),
            bars: vec![bar("a", Some("Login"), ymd(2026, 1, 1), ymd(2026, 1, 5))],
        };
        let output = render_gantt_by_initiative(&data(vec![init], vec![]), "");
        assert!(output.starts_with("# Gantt by initiative\n\n"));
        assert!(output.contains("## User Auth Epic\n\n"));
        assert!(output.contains("```mermaid\ngantt\n    dateFormat YYYY-MM-DD\n"));
        assert!(output.contains("    Login :a, 2026-01-01, 2026-01-05\n"));
        assert!(!output.contains("section "));
    }

    #[test]
    fn multiple_initiatives_emit_in_extractor_order() {
        let alpha = Initiative {
            root: card("alpha", Some("Alpha")),
            bars: vec![bar("a", None, ymd(2026, 1, 1), ymd(2026, 1, 5))],
        };
        let beta = Initiative {
            root: card("beta", Some("Beta")),
            bars: vec![bar("b", None, ymd(2026, 1, 1), ymd(2026, 1, 5))],
        };
        let output = render_gantt_by_initiative(&data(vec![alpha, beta], vec![]), "");
        let alpha_at = output.find("## Alpha").unwrap();
        let beta_at = output.find("## Beta").unwrap();
        assert!(alpha_at < beta_at);
    }

    #[test]
    fn empty_initiative_is_skipped_no_subheading() {
        let init = Initiative {
            root: card("empty-epic", Some("Empty Epic")),
            bars: vec![],
        };
        let output = render_gantt_by_initiative(&data(vec![init], vec![]), "");
        assert_eq!(output, "# Gantt by initiative\n\n");
        assert!(!output.contains("Empty Epic"));
    }

    #[test]
    fn root_falls_back_to_id_when_title_missing() {
        let init = Initiative {
            root: card("plain-epic", None),
            bars: vec![bar("a", None, ymd(2026, 1, 1), ymd(2026, 1, 5))],
        };
        let output = render_gantt_by_initiative(&data(vec![init], vec![]), "");
        assert!(output.contains("## plain-epic\n\n"));
    }

    #[test]
    fn unplaced_footer_renders_below_all_initiatives() {
        let init = Initiative {
            root: card("epic", Some("Epic")),
            bars: vec![bar("a", None, ymd(2026, 1, 1), ymd(2026, 1, 5))],
        };
        let unplaced = vec![UnplacedCard {
            card: card("z", Some("Zeta")),
            reason: UnplacedReason::MissingValue {
                field: "start".into(),
            },
        }];
        let output = render_gantt_by_initiative(&data(vec![init], unplaced), "");
        let block_at = output.find("```mermaid").unwrap();
        let footer_at = output.find("> _1 items dropped:_").unwrap();
        assert!(block_at < footer_at);
        assert!(output.contains("> _- missing 'start': \"Zeta\"_\n"));
    }

    #[test]
    fn full_output_snapshot_two_initiatives_with_unplaced() {
        let alpha = Initiative {
            root: card("alpha", Some("Alpha")),
            bars: vec![bar("a", Some("Task A"), ymd(2026, 1, 1), ymd(2026, 1, 5))],
        };
        let beta = Initiative {
            root: card("beta", Some("Beta")),
            bars: vec![bar("b", Some("Task B"), ymd(2026, 1, 6), ymd(2026, 1, 9))],
        };
        let unplaced = vec![UnplacedCard {
            card: card("z", Some("Zeta")),
            reason: UnplacedReason::InvalidRange {
                start_field: "start".into(),
                end_field: "end".into(),
            },
        }];
        let output = render_gantt_by_initiative(&data(vec![alpha, beta], unplaced), "");
        let expected = "# Gantt by initiative\n\n\
            ## Alpha\n\n\
            ```mermaid\n\
            gantt\n    \
            dateFormat YYYY-MM-DD\n    \
            Task A :a, 2026-01-01, 2026-01-05\n\
            ```\n\n\
            ## Beta\n\n\
            ```mermaid\n\
            gantt\n    \
            dateFormat YYYY-MM-DD\n    \
            Task B :b, 2026-01-06, 2026-01-09\n\
            ```\n\n\
            > _1 items dropped:_\n\
            > _- invalid range: \"Zeta\"_\n";
        // Spacing: each gantt block ends with `\n`. Between initiatives
        // an extra `\n` adds the blank line. Footer adds its own leading
        // `\n` for the blank line before the unplaced summary.
        assert_eq!(output, expected);
    }

    #[test]
    fn empty_initiatives_with_only_unplaced_emits_heading_and_footer() {
        let unplaced = vec![UnplacedCard {
            card: card("a", Some("A")),
            reason: UnplacedReason::MissingValue {
                field: "start".into(),
            },
        }];
        let output = render_gantt_by_initiative(&data(vec![], unplaced), "");
        assert!(output.starts_with("# Gantt by initiative\n\n"));
        assert!(!output.contains("```mermaid"));
        assert!(output.contains("> _1 items dropped:_\n"));
    }
}
