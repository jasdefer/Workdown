//! Metric renderer — turns [`MetricData`] into a Markdown stat table.
//!
//! Output shape: a top-level `# Metrics` heading, an optional one-line
//! description, then a GFM table with one row per [`MetricRowData`]:
//! `| Label | Value |`. `None` values render as `—` (no data). When
//! any row has unplaced items (filter-matched but missing the value
//! field), a blockquote footer lists them per-row, grouped by row label
//! and field name. An empty `rows` list emits the heading only.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use workdown_core::model::duration::format_duration_seconds;
use workdown_core::view_data::{
    AggregateValue, MetricData, MetricRowData, UnplacedCard, UnplacedReason,
};

use crate::render::markdown::{
    emit_description, escape_blockquote_italic, escape_cell, format_number,
};

/// Render a `MetricData` as a Markdown string.
///
/// `description` is a one-line caption emitted below the heading; empty
/// string skips it. Unlike board/table/tree the metric renderer doesn't
/// take an `item_link_base` — values are scalars, not item references,
/// and the unplaced footer uses titles only (matching the gantt
/// pattern).
pub fn render_metric(data: &MetricData, description: &str) -> String {
    let mut out = String::new();
    out.push_str("# Metrics\n\n");
    emit_description(description, &mut out);

    if data.rows.is_empty() {
        return out;
    }

    out.push_str("| Label | Value |\n");
    out.push_str("| --- | --- |\n");
    for row in &data.rows {
        let _ = writeln!(
            out,
            "| {label} | {value} |",
            label = escape_cell(&row.label),
            value = format_value(&row.value),
        );
    }

    render_unplaced_footer(&data.rows, &mut out);
    out
}

fn format_value(value: &Option<AggregateValue>) -> String {
    match value {
        None => "—".to_owned(),
        Some(AggregateValue::Number(n)) => format_number(*n),
        Some(AggregateValue::Date(d)) => d.format("%Y-%m-%d").to_string(),
        Some(AggregateValue::Duration(seconds)) => format_duration_seconds(*seconds),
    }
}

/// Emit a blockquote footer summarizing items that filter-matched but
/// were missing the value field for one or more rows. Skipped entirely
/// when no row has unplaced items.
///
/// Rows without unplaced are silent. Rows with unplaced are grouped by
/// row label, then by missing field, with item titles (or ids when
/// titles are absent) listed in the order the extractor produced.
fn render_unplaced_footer(rows: &[MetricRowData], out: &mut String) {
    let total: usize = rows.iter().map(|r| r.unplaced.len()).sum();
    if total == 0 {
        return;
    }

    out.push('\n');
    let _ = writeln!(out, "> _{total} items dropped:_");
    for row in rows {
        if row.unplaced.is_empty() {
            continue;
        }
        let missing = group_by_missing_field(&row.unplaced);
        for (field, cards) in &missing {
            let _ = writeln!(
                out,
                "> _- \"{label}\" missing '{field}': {titles}_",
                label = escape_blockquote_italic(&row.label),
                titles = format_titles(cards),
            );
        }
    }
}

fn group_by_missing_field(unplaced: &[UnplacedCard]) -> BTreeMap<&str, Vec<&UnplacedCard>> {
    let mut grouped: BTreeMap<&str, Vec<&UnplacedCard>> = BTreeMap::new();
    for card in unplaced {
        if let UnplacedReason::MissingValue { field } = &card.reason {
            grouped.entry(field.as_str()).or_default().push(card);
        }
    }
    grouped
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

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::test_fixtures::unplaced_missing;
    use super::*;
    use chrono::NaiveDate;
    use workdown_core::model::views::Aggregate;
    use workdown_core::view_data::{MetricData, MetricRowData};

    fn ymd(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    fn row(label: &str, aggregate: Aggregate, value: Option<AggregateValue>) -> MetricRowData {
        MetricRowData {
            label: label.to_owned(),
            aggregate,
            value_field: None,
            value,
            unplaced: vec![],
        }
    }

    fn data(rows: Vec<MetricRowData>) -> MetricData {
        MetricData { rows }
    }

    #[test]
    fn renders_top_heading() {
        let output = render_metric(&data(vec![]), "");
        assert!(output.starts_with("# Metrics\n\n"));
    }

    #[test]
    fn empty_rows_emits_heading_only() {
        let output = render_metric(&data(vec![]), "");
        assert_eq!(output, "# Metrics\n\n");
    }

    #[test]
    fn single_count_row_renders_table() {
        let output = render_metric(
            &data(vec![row(
                "Open items",
                Aggregate::Count,
                Some(AggregateValue::Number(12.0)),
            )]),
            "",
        );
        let expected = "# Metrics\n\n| Label | Value |\n| --- | --- |\n| Open items | 12 |\n";
        assert_eq!(output, expected);
    }

    #[test]
    fn integer_number_drops_decimal() {
        let output = render_metric(
            &data(vec![row(
                "Count",
                Aggregate::Count,
                Some(AggregateValue::Number(7.0)),
            )]),
            "",
        );
        assert!(output.contains("| Count | 7 |\n"));
        assert!(!output.contains("7.0"));
    }

    #[test]
    fn fractional_number_preserves_precision() {
        let output = render_metric(
            &data(vec![row(
                "Avg",
                Aggregate::Avg,
                Some(AggregateValue::Number(3.5)),
            )]),
            "",
        );
        assert!(output.contains("| Avg | 3.5 |\n"));
    }

    #[test]
    fn date_value_formats_iso() {
        let output = render_metric(
            &data(vec![row(
                "Latest deadline",
                Aggregate::Max,
                Some(AggregateValue::Date(ymd(2026, 5, 15))),
            )]),
            "",
        );
        assert!(output.contains("| Latest deadline | 2026-05-15 |\n"));
    }

    #[test]
    fn duration_value_formats_shorthand() {
        let output = render_metric(
            &data(vec![row(
                "Total estimate",
                Aggregate::Sum,
                Some(AggregateValue::Duration(86400 + 3600)), // 1d 1h
            )]),
            "",
        );
        assert!(
            output.contains("| Total estimate | 1d 1h |\n"),
            "got: {output}"
        );
    }

    #[test]
    fn none_value_renders_em_dash() {
        let output = render_metric(&data(vec![row("Avg deadline", Aggregate::Avg, None)]), "");
        assert!(output.contains("| Avg deadline | — |\n"));
    }

    #[test]
    fn description_emitted_under_heading() {
        let output = render_metric(
            &data(vec![row(
                "Open",
                Aggregate::Count,
                Some(AggregateValue::Number(3.0)),
            )]),
            "Project stats.",
        );
        assert!(output.starts_with("# Metrics\n\nProject stats.\n\n| Label |"));
    }

    #[test]
    fn label_pipe_is_escaped() {
        let output = render_metric(
            &data(vec![row(
                "a | b",
                Aggregate::Count,
                Some(AggregateValue::Number(1.0)),
            )]),
            "",
        );
        assert!(output.contains(r"| a \| b | 1 |"));
    }

    #[test]
    fn label_newline_becomes_br() {
        let output = render_metric(
            &data(vec![row(
                "line one\nline two",
                Aggregate::Count,
                Some(AggregateValue::Number(1.0)),
            )]),
            "",
        );
        assert!(output.contains("| line one<br>line two | 1 |"));
    }

    #[test]
    fn unplaced_footer_lists_rows_with_missing_field() {
        let mut total = row(
            "Sum points",
            Aggregate::Sum,
            Some(AggregateValue::Number(3.0)),
        );
        total.unplaced = vec![
            unplaced_missing("foo", Some("Foo task"), "points"),
            unplaced_missing("bar", None, "points"),
        ];
        let output = render_metric(&data(vec![total]), "");
        assert!(output.contains("> _2 items dropped:_"));
        assert!(output.contains("> _- \"Sum points\" missing 'points': \"Foo task\", \"bar\"_"));
    }

    #[test]
    fn no_unplaced_footer_when_all_rows_clean() {
        let output = render_metric(
            &data(vec![row(
                "Open",
                Aggregate::Count,
                Some(AggregateValue::Number(3.0)),
            )]),
            "",
        );
        assert!(!output.contains("dropped"));
    }

    #[test]
    fn multiple_rows_render_in_order() {
        let output = render_metric(
            &data(vec![
                row(
                    "Total",
                    Aggregate::Count,
                    Some(AggregateValue::Number(10.0)),
                ),
                row(
                    "In progress",
                    Aggregate::Count,
                    Some(AggregateValue::Number(4.0)),
                ),
                row(
                    "Sum points",
                    Aggregate::Sum,
                    Some(AggregateValue::Number(47.0)),
                ),
            ]),
            "",
        );
        let total_at = output.find("| Total |").unwrap();
        let in_progress_at = output.find("| In progress |").unwrap();
        let sum_points_at = output.find("| Sum points |").unwrap();
        assert!(total_at < in_progress_at);
        assert!(in_progress_at < sum_points_at);
    }

    #[test]
    fn full_output_snapshot_with_unplaced() {
        let mut sum_row = row(
            "Sum points",
            Aggregate::Sum,
            Some(AggregateValue::Number(7.0)),
        );
        sum_row.unplaced = vec![unplaced_missing("missing", Some("Missing item"), "points")];
        let rows = vec![
            row("Total", Aggregate::Count, Some(AggregateValue::Number(3.0))),
            sum_row,
        ];
        let output = render_metric(&data(rows), "Project stats.");
        let expected = "\
# Metrics

Project stats.

| Label | Value |
| --- | --- |
| Total | 3 |
| Sum points | 7 |

> _1 items dropped:_
> _- \"Sum points\" missing 'points': \"Missing item\"_
";
        assert_eq!(output, expected);
    }
}
