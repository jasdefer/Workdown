//! Bar chart renderer — turns [`BarChartData`] into a Markdown document
//! with a horizontal-bar inline SVG (plotters) and a `## Values` summary
//! table for the exact aggregated numbers.
//!
//! Heading variants:
//! - `Aggregate::Count` → `# Bar chart: count by <group_by>`
//! - other → `# Bar chart: <aggregate> of <value_field> by <group_by>`
//!
//! Layout: groups are y-axis category labels via plotters' segmented
//! coords; the aggregate value runs along the x-axis. Bars are drawn in
//! the single first-palette color — categories are already named on the
//! y-axis, so per-bar coloring would be decorative noise. SVG height
//! scales with bar count so 3-bar and 30-bar charts both stay readable.
//!
//! Value-axis kinds reuse [`crate::render::chart_common::AxisKind`].
//! Number/Duration bars start from 0 (chart x-range force-includes 0
//! before padding). Date bars start from the chart's left edge — the
//! padded minimum date — so each bar's length encodes "days since the
//! earliest bar" rather than "days since CE", making comparisons
//! between dates legible without an arbitrary CE-epoch baseline.

use std::fmt::Write as _;

use plotters::coord::ranged1d::SegmentValue;
use plotters::prelude::*;

use workdown_core::model::views::Aggregate;
use workdown_core::view_data::{BarChartData, UnplacedReason};

use crate::render::chart_common::{
    axis_kind_for, axis_label, format_aggregate_value, format_axis_tick, hex_to_rgb,
    numeric_extent, pad_extent, strip_svg_blank_lines, value_to_f64, AxisKind, OKABE_ITO,
};
use crate::render::common::{card_link, emit_description};

const SVG_WIDTH: u32 = 800;
const SVG_MIN_HEIGHT: u32 = 200;
const SVG_PER_BAR: u32 = 30;
const SVG_HEIGHT_OVERHEAD: u32 = 80;

/// Render a `BarChartData` as a Markdown string.
///
/// `item_link_base` is the relative path from the rendered view file to
/// the work items directory — same parameter as `render_treemap`.
/// `description` is the one-line caption emitted below the heading.
pub fn render_bar_chart(data: &BarChartData, item_link_base: &str, description: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}", heading(data));
    out.push('\n');
    emit_description(description, &mut out);

    if data.bars.is_empty() && data.unplaced.is_empty() {
        out.push_str("_(no items)_\n");
        return out;
    }

    if !data.bars.is_empty() {
        let svg = render_svg(data);
        out.push_str(&svg);
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');

        emit_values_table(data, &mut out);
        out.push('\n');
    }

    if !data.unplaced.is_empty() {
        out.push_str("## Unplaced\n");
        for unplaced in &data.unplaced {
            let UnplacedReason::MissingValue { field } = &unplaced.reason else {
                // Extractor only ever emits MissingValue; defense in depth.
                continue;
            };
            let _ = writeln!(
                out,
                "- {link} — missing `{field}`",
                link = card_link(&unplaced.card, item_link_base),
            );
        }
    }

    out
}

fn heading(data: &BarChartData) -> String {
    match data.aggregate {
        Aggregate::Count => format!("# Bar chart: count by {}", data.group_by),
        agg => match &data.value_field {
            Some(value) => format!("# Bar chart: {agg} of {value} by {}", data.group_by),
            None => format!("# Bar chart: {agg} by {}", data.group_by),
        },
    }
}

fn emit_values_table(data: &BarChartData, out: &mut String) {
    out.push_str("## Values\n\n");
    let _ = writeln!(out, "| {} | {} |", data.group_by, value_column_header(data));
    out.push_str("| --- | --- |\n");
    for bar in &data.bars {
        let _ = writeln!(
            out,
            "| {group} | {value} |",
            group = escape_cell(&bar.group),
            value = format_aggregate_value(&bar.value),
        );
    }
}

fn value_column_header(data: &BarChartData) -> String {
    match data.aggregate {
        Aggregate::Count => "count".to_owned(),
        agg => match &data.value_field {
            Some(value) => format!("{agg} of {value}"),
            None => format!("{agg}"),
        },
    }
}

/// Neutralize `|` (would end the cell) and newlines (would end the row)
/// inside a Markdown table cell. Mirrors the metric renderer's escape.
fn escape_cell(text: &str) -> String {
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

// ── SVG rendering ───────────────────────────────────────────────────

fn render_svg(data: &BarChartData) -> String {
    let kind = axis_kind_for(data.bars.iter().map(|b| b.value));
    let encoded: Vec<f64> = data
        .bars
        .iter()
        .map(|b| value_to_f64(b.value, kind))
        .collect();

    let (vmin_raw, vmax_raw) = numeric_extent(encoded.iter().copied());
    let (vmin, vmax) = match kind {
        // For Number/Duration the natural baseline is 0 — bars need it
        // in range or their lengths misrepresent magnitude.
        AxisKind::Number | AxisKind::Duration { .. } => (vmin_raw.min(0.0), vmax_raw.max(0.0)),
        // Date: relative-to-min, no zero baseline.
        AxisKind::Date => (vmin_raw, vmax_raw),
    };
    let (x_min, x_max) = pad_extent(vmin, vmax);

    let bar_left = match kind {
        AxisKind::Number | AxisKind::Duration { .. } => 0.0,
        AxisKind::Date => x_min,
    };

    let value_axis_label = bar_value_axis_label(data, kind);
    let bar_color = hex_to_rgb(OKABE_ITO[0]);

    let n_bars = data.bars.len() as i32;
    let height = SVG_MIN_HEIGHT.max(SVG_HEIGHT_OVERHEAD + (data.bars.len() as u32) * SVG_PER_BAR);

    let labels: Vec<String> = data.bars.iter().map(|b| b.group.clone()).collect();

    let mut buf = String::new();
    {
        let root = SVGBackend::with_string(&mut buf, (SVG_WIDTH, height)).into_drawing_area();
        root.fill(&WHITE).expect("fill white background");

        let mut chart = ChartBuilder::on(&root)
            .margin(20)
            .x_label_area_size(50)
            .y_label_area_size(150)
            .build_cartesian_2d(x_min..x_max, (0..n_bars).into_segmented())
            .expect("build cartesian 2d");

        chart
            .configure_mesh()
            .x_desc(value_axis_label)
            .x_label_formatter(&|value: &f64| format_axis_tick(*value, kind))
            .y_label_formatter(&|seg: &SegmentValue<i32>| match seg {
                SegmentValue::Exact(i) | SegmentValue::CenterOf(i) => {
                    labels.get(*i as usize).cloned().unwrap_or_default()
                }
                SegmentValue::Last => String::new(),
            })
            .y_labels(data.bars.len().max(1))
            .draw()
            .expect("draw mesh");

        chart
            .draw_series(encoded.iter().enumerate().map(|(i, &value)| {
                Rectangle::new(
                    [
                        (bar_left, SegmentValue::Exact(i as i32)),
                        (value, SegmentValue::Exact(i as i32 + 1)),
                    ],
                    bar_color.filled(),
                )
            }))
            .expect("draw bars");

        root.present().expect("present svg");
    }
    strip_svg_blank_lines(&buf)
}

/// Compose the value-axis title: aggregate function over the value field,
/// suffixed with the duration unit when applicable.
fn bar_value_axis_label(data: &BarChartData, kind: AxisKind) -> String {
    let base = match data.aggregate {
        Aggregate::Count => "count".to_owned(),
        agg => match &data.value_field {
            Some(value) => format!("{agg} of {value}"),
            None => format!("{agg}"),
        },
    };
    axis_label(&base, kind)
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use chrono::NaiveDate;

    use crate::render::chart_common::{SECONDS_PER_DAY, SECONDS_PER_HOUR};
    use workdown_core::model::views::Aggregate;
    use workdown_core::model::WorkItemId;
    use workdown_core::view_data::{
        AggregateValue, BarChartBar, BarChartData, Card, UnplacedCard, UnplacedReason,
    };

    fn card(id: &str, title: Option<&str>) -> Card {
        Card {
            id: WorkItemId::from(id.to_owned()),
            title: title.map(str::to_owned),
            fields: vec![],
            body: String::new(),
        }
    }

    fn bar(group: &str, value: AggregateValue) -> BarChartBar {
        BarChartBar {
            group: group.to_owned(),
            value,
        }
    }

    fn data(
        group_by: &str,
        value_field: Option<&str>,
        aggregate: Aggregate,
        bars: Vec<BarChartBar>,
        unplaced: Vec<UnplacedCard>,
    ) -> BarChartData {
        BarChartData {
            group_by: group_by.to_owned(),
            value_field: value_field.map(str::to_owned),
            aggregate,
            bars,
            unplaced,
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

    // ── Heading / empty / description ───────────────────────────────

    #[test]
    fn heading_count_form() {
        let output = render_bar_chart(
            &data("status", None, Aggregate::Count, vec![], vec![]),
            "../workdown-items",
            "",
        );
        assert!(output.starts_with("# Bar chart: count by status\n"));
    }

    #[test]
    fn heading_aggregate_of_value_form() {
        let output = render_bar_chart(
            &data("status", Some("points"), Aggregate::Sum, vec![], vec![]),
            "../workdown-items",
            "",
        );
        assert!(output.starts_with("# Bar chart: sum of points by status\n"));
    }

    #[test]
    fn empty_view_emits_no_items_marker_and_no_svg() {
        let output = render_bar_chart(
            &data("status", None, Aggregate::Count, vec![], vec![]),
            "../workdown-items",
            "",
        );
        assert!(output.contains("_(no items)_"));
        assert!(!output.contains("<svg"));
    }

    #[test]
    fn description_emitted_under_heading() {
        let output = render_bar_chart(
            &data("status", None, Aggregate::Count, vec![], vec![]),
            "../workdown-items",
            "Items per status.",
        );
        assert!(output.contains("# Bar chart: count by status\n\nItems per status.\n\n"));
    }

    // ── Number-valued bars ──────────────────────────────────────────

    #[test]
    fn number_bars_emit_svg_with_first_palette_color() {
        let bars = vec![
            bar("done", AggregateValue::Number(1.0)),
            bar("open", AggregateValue::Number(2.0)),
        ];
        let output = render_bar_chart(
            &data("status", None, Aggregate::Count, bars, vec![]),
            "../workdown-items",
            "",
        );
        assert!(output.contains("<svg"));
        assert!(
            output.contains("#E69F00") || output.contains("rgb(230,159,0)"),
            "expected first palette color, got: {output}"
        );
    }

    #[test]
    fn negative_number_bar_keeps_zero_in_range() {
        // A single negative bar must still leave 0 inside the visible
        // x-range, so the bar reads as "below zero" not as "the whole axis".
        let bars = vec![bar("alpha", AggregateValue::Number(-5.0))];
        let output = render_bar_chart(
            &data("g", None, Aggregate::Sum, bars, vec![]),
            "../workdown-items",
            "",
        );
        // Tick labels should include a 0 — proxy for "0 is in range".
        // Plotters wraps tick text in `<text ...>\n0\n</text>`, so match
        // the trailing close rather than `>0<` which ignores whitespace.
        assert!(
            output.contains("0\n</text>"),
            "expected '0' tick label, got: {output}"
        );
    }

    #[test]
    fn all_same_value_renders_without_panic() {
        let bars = vec![
            bar("a", AggregateValue::Number(7.0)),
            bar("b", AggregateValue::Number(7.0)),
        ];
        let output = render_bar_chart(
            &data("g", None, Aggregate::Count, bars, vec![]),
            "../workdown-items",
            "",
        );
        assert!(output.contains("<svg"));
    }

    // ── Date-valued bars ────────────────────────────────────────────

    #[test]
    fn date_bars_render_iso_x_axis_ticks() {
        let bars = vec![
            bar(
                "open",
                AggregateValue::Date(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()),
            ),
            bar(
                "done",
                AggregateValue::Date(NaiveDate::from_ymd_opt(2026, 6, 1).unwrap()),
            ),
        ];
        let output = render_bar_chart(
            &data("status", Some("deadline"), Aggregate::Avg, bars, vec![]),
            "../workdown-items",
            "",
        );
        assert!(
            output.contains("2026-"),
            "expected ISO date tick label in: {output}"
        );
    }

    // ── Duration-valued bars ────────────────────────────────────────

    #[test]
    fn duration_bars_axis_label_includes_unit() {
        let bars = vec![
            bar("alpha", AggregateValue::Duration(2 * SECONDS_PER_DAY)),
            bar("beta", AggregateValue::Duration(4 * SECONDS_PER_DAY)),
        ];
        let output = render_bar_chart(
            &data("tag", Some("estimate"), Aggregate::Sum, bars, vec![]),
            "../workdown-items",
            "",
        );
        assert!(
            output.contains("sum of estimate (days)"),
            "expected x-axis label to include unit, got: {output}"
        );
    }

    #[test]
    fn duration_bars_axis_chooses_hours_for_short_ranges() {
        let bars = vec![
            bar("alpha", AggregateValue::Duration(2 * SECONDS_PER_HOUR)),
            bar("beta", AggregateValue::Duration(5 * SECONDS_PER_HOUR)),
        ];
        let output = render_bar_chart(
            &data("tag", Some("estimate"), Aggregate::Sum, bars, vec![]),
            "../workdown-items",
            "",
        );
        assert!(output.contains("sum of estimate (hours)"));
    }

    // ── Bar order / category labels ─────────────────────────────────

    #[test]
    fn category_labels_appear_in_extractor_order() {
        let bars = vec![
            bar("apples", AggregateValue::Number(3.0)),
            bar("bananas", AggregateValue::Number(5.0)),
        ];
        let output = render_bar_chart(
            &data("fruit", None, Aggregate::Count, bars, vec![]),
            "../workdown-items",
            "",
        );
        let apples_at = output
            .find("apples")
            .expect("apples should appear in output");
        let bananas_at = output
            .find("bananas")
            .expect("bananas should appear in output");
        // Category appears in y-axis label SVG text and in the values table;
        // both sources preserve extractor order, so apples < bananas in
        // either case.
        assert!(apples_at < bananas_at);
    }

    // ── Values summary table ────────────────────────────────────────

    #[test]
    fn values_table_lists_each_bar() {
        let bars = vec![
            bar("done", AggregateValue::Number(2.0)),
            bar("open", AggregateValue::Number(5.0)),
        ];
        let output = render_bar_chart(
            &data("status", None, Aggregate::Count, bars, vec![]),
            "../workdown-items",
            "",
        );
        assert!(output.contains("## Values\n"));
        assert!(output.contains("| status | count |"));
        assert!(output.contains("| done | 2 |"));
        assert!(output.contains("| open | 5 |"));
    }

    #[test]
    fn values_table_formats_durations_as_shorthand() {
        let bars = vec![bar(
            "alpha",
            AggregateValue::Duration(SECONDS_PER_DAY + SECONDS_PER_HOUR),
        )];
        let output = render_bar_chart(
            &data("tag", Some("estimate"), Aggregate::Sum, bars, vec![]),
            "../workdown-items",
            "",
        );
        assert!(
            output.contains("| alpha | 1d 1h |"),
            "expected duration shorthand, got: {output}"
        );
    }

    #[test]
    fn values_table_formats_dates_as_iso() {
        let bars = vec![bar(
            "open",
            AggregateValue::Date(NaiveDate::from_ymd_opt(2026, 5, 15).unwrap()),
        )];
        let output = render_bar_chart(
            &data("status", Some("deadline"), Aggregate::Avg, bars, vec![]),
            "../workdown-items",
            "",
        );
        assert!(output.contains("| open | 2026-05-15 |"));
    }

    #[test]
    fn values_table_escapes_pipe_in_group_name() {
        let bars = vec![bar("a | b", AggregateValue::Number(1.0))];
        let output = render_bar_chart(
            &data("status", None, Aggregate::Count, bars, vec![]),
            "../workdown-items",
            "",
        );
        assert!(output.contains(r"| a \| b | 1 |"));
    }

    // ── Unplaced footer ─────────────────────────────────────────────

    #[test]
    fn unplaced_footer_lists_missing_field_per_item() {
        let bars = vec![bar("open", AggregateValue::Number(1.0))];
        let output = render_bar_chart(
            &data(
                "status",
                None,
                Aggregate::Count,
                bars,
                vec![
                    unplaced("missing-status", Some("Missing"), "status"),
                    unplaced("missing-other", None, "status"),
                ],
            ),
            "../workdown-items",
            "",
        );
        assert!(output.contains("## Unplaced\n"));
        assert!(
            output.contains("[Missing](../workdown-items/missing-status.md) — missing `status`")
        );
        assert!(output
            .contains("[missing-other](../workdown-items/missing-other.md) — missing `status`"));
    }

    #[test]
    fn no_unplaced_section_when_clean() {
        let bars = vec![bar("open", AggregateValue::Number(1.0))];
        let output = render_bar_chart(
            &data("status", None, Aggregate::Count, bars, vec![]),
            "../workdown-items",
            "",
        );
        assert!(!output.contains("Unplaced"));
    }

    #[test]
    fn only_unplaced_emits_footer_without_svg_or_table() {
        let output = render_bar_chart(
            &data(
                "status",
                None,
                Aggregate::Count,
                vec![],
                vec![unplaced("orphan", Some("Orphan"), "status")],
            ),
            "../workdown-items",
            "",
        );
        assert!(!output.contains("<svg"));
        assert!(!output.contains("## Values"));
        assert!(output.contains("## Unplaced\n"));
        assert!(output.contains("[Orphan](../workdown-items/orphan.md) — missing `status`"));
    }
}
