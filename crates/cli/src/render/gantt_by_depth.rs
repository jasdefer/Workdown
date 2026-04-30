//! Gantt-by-depth renderer — turns [`GanttByDepthData`] into a Markdown
//! document with one Mermaid `gantt` block per non-empty depth level.
//!
//! Output shape: a top-level `# Gantt by depth` heading, then one
//! `## Level <n>` subheading + Mermaid block per non-empty level
//! (ascending by depth). Empty levels are skipped — gaps are implicit.
//! Filter-matched items that couldn't be placed (missing dates, inverted
//! range, etc.) appear in a single global blockquote footer at the
//! document bottom, same shape as basic Gantt.
//!
//! No per-level section grouping: each chart is already scoped to one
//! depth, so the inner block is flat. Block formatting, sanitization,
//! and footer reuse [`super::gantt_common`].

use workdown_core::view_data::GanttByDepthData;

use super::gantt_common::{render_gantt_block, render_unplaced_footer};

/// Render a `GanttByDepthData` as a Markdown string.
pub fn render_gantt_by_depth(data: &GanttByDepthData) -> String {
    let mut out = String::new();
    out.push_str("# Gantt by depth\n\n");

    let mut first = true;
    for level in &data.levels {
        if level.bars.is_empty() {
            continue;
        }
        if !first {
            out.push('\n');
        }
        first = false;
        out.push_str(&format!("## Level {}\n\n", level.depth));
        out.push_str(&render_gantt_block(&level.bars, None));
    }

    render_unplaced_footer(&data.unplaced, &mut out);
    out
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use workdown_core::model::WorkItemId;
    use workdown_core::view_data::{
        Card, GanttBar, GanttByDepthData, Level, UnplacedCard, UnplacedReason,
    };

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
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

    fn data(levels: Vec<Level>, unplaced: Vec<UnplacedCard>) -> GanttByDepthData {
        GanttByDepthData { levels, unplaced }
    }

    #[test]
    fn empty_data_emits_heading_only() {
        let output = render_gantt_by_depth(&data(vec![], vec![]));
        assert_eq!(output, "# Gantt by depth\n\n");
    }

    #[test]
    fn single_level_emits_subheading_and_one_block() {
        let level = Level {
            depth: 0,
            bars: vec![bar("a", Some("Login"), ymd(2026, 1, 1), ymd(2026, 1, 5))],
        };
        let output = render_gantt_by_depth(&data(vec![level], vec![]));
        assert!(output.starts_with("# Gantt by depth\n\n"));
        assert!(output.contains("## Level 0\n\n"));
        assert!(output.contains("```mermaid\ngantt\n    dateFormat YYYY-MM-DD\n"));
        assert!(output.contains("    Login :a, 2026-01-01, 2026-01-05\n"));
        assert!(!output.contains("section "));
    }

    #[test]
    fn multiple_levels_emit_in_data_order() {
        let l0 = Level {
            depth: 0,
            bars: vec![bar("root", None, ymd(2026, 1, 1), ymd(2026, 1, 31))],
        };
        let l1 = Level {
            depth: 1,
            bars: vec![bar("child", None, ymd(2026, 1, 5), ymd(2026, 1, 10))],
        };
        let output = render_gantt_by_depth(&data(vec![l0, l1], vec![]));
        let l0_at = output.find("## Level 0").unwrap();
        let l1_at = output.find("## Level 1").unwrap();
        assert!(l0_at < l1_at);
    }

    #[test]
    fn level_gap_is_implicit() {
        // Levels 0 and 2 emitted directly with no level-1 block in between.
        let l0 = Level {
            depth: 0,
            bars: vec![bar("root", None, ymd(2026, 1, 1), ymd(2026, 1, 31))],
        };
        let l2 = Level {
            depth: 2,
            bars: vec![bar("leaf", None, ymd(2026, 1, 5), ymd(2026, 1, 10))],
        };
        let output = render_gantt_by_depth(&data(vec![l0, l2], vec![]));
        assert!(output.contains("## Level 0"));
        assert!(!output.contains("## Level 1"));
        assert!(output.contains("## Level 2"));
    }

    #[test]
    fn empty_level_is_skipped_no_subheading() {
        let level = Level {
            depth: 0,
            bars: vec![],
        };
        let output = render_gantt_by_depth(&data(vec![level], vec![]));
        assert_eq!(output, "# Gantt by depth\n\n");
        assert!(!output.contains("## Level"));
    }

    #[test]
    fn unplaced_footer_renders_below_all_levels() {
        let level = Level {
            depth: 0,
            bars: vec![bar("a", None, ymd(2026, 1, 1), ymd(2026, 1, 5))],
        };
        let unplaced = vec![UnplacedCard {
            card: card("z", Some("Zeta")),
            reason: UnplacedReason::MissingValue {
                field: "start".into(),
            },
        }];
        let output = render_gantt_by_depth(&data(vec![level], unplaced));
        let block_at = output.find("```mermaid").unwrap();
        let footer_at = output.find("> _1 items dropped:_").unwrap();
        assert!(block_at < footer_at);
        assert!(output.contains("> _- missing 'start': \"Zeta\"_\n"));
    }

    #[test]
    fn full_output_snapshot_two_levels_with_unplaced() {
        let l0 = Level {
            depth: 0,
            bars: vec![bar("root", Some("Root"), ymd(2026, 1, 1), ymd(2026, 1, 5))],
        };
        let l1 = Level {
            depth: 1,
            bars: vec![bar(
                "child",
                Some("Child"),
                ymd(2026, 1, 6),
                ymd(2026, 1, 9),
            )],
        };
        let unplaced = vec![UnplacedCard {
            card: card("z", Some("Zeta")),
            reason: UnplacedReason::InvalidRange {
                start_field: "start".into(),
                end_field: "end".into(),
            },
        }];
        let output = render_gantt_by_depth(&data(vec![l0, l1], unplaced));
        let expected = "# Gantt by depth\n\n\
            ## Level 0\n\n\
            ```mermaid\n\
            gantt\n    \
            dateFormat YYYY-MM-DD\n    \
            Root :root, 2026-01-01, 2026-01-05\n\
            ```\n\n\
            ## Level 1\n\n\
            ```mermaid\n\
            gantt\n    \
            dateFormat YYYY-MM-DD\n    \
            Child :child, 2026-01-06, 2026-01-09\n\
            ```\n\n\
            > _1 items dropped:_\n\
            > _- invalid range: \"Zeta\"_\n";
        assert_eq!(output, expected);
    }

    #[test]
    fn empty_levels_with_only_unplaced_emits_heading_and_footer() {
        let unplaced = vec![UnplacedCard {
            card: card("a", Some("A")),
            reason: UnplacedReason::MissingValue {
                field: "start".into(),
            },
        }];
        let output = render_gantt_by_depth(&data(vec![], unplaced));
        assert!(output.starts_with("# Gantt by depth\n\n"));
        assert!(!output.contains("```mermaid"));
        assert!(output.contains("> _1 items dropped:_\n"));
    }
}
