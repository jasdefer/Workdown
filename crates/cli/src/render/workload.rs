//! Workload renderer — turns [`WorkloadData`] into a Markdown document
//! with an inline vertical-bar SVG (plotters-backed) and a `## Values`
//! summary table for the per-day totals.
//!
//! Output shape:
//! - `# Workload: <effort_field> per working day` heading
//! - One-line description from `description.rs`
//! - SVG: one bar per working-day bucket, x-axis is a categorical list
//!   of dates so non-working days don't visually appear at all (matches
//!   how Jira / Asana resource views render). Tick labels thin out for
//!   long ranges to keep the axis legible.
//! - `## Values` table with `| Date | <effort> |` per non-zero bucket.
//! - `## Unplaced` footer covering `MissingValue`, `InvalidRange`, and
//!   `NoWorkingDays` reasons.
//!
//! Effort unit:
//! - `WorkloadUnit::Number` — bucket totals are raw `f64`. Y-axis label
//!   is the bare effort field name. Values table cells use
//!   [`format_number`] to drop trailing `.0` for integer sums.
//! - `WorkloadUnit::Duration` — bucket totals are canonical seconds.
//!   The renderer picks a duration unit (hours, days, …) via
//!   [`pick_duration_unit`] over the max bucket total, divides bar
//!   heights by `divisor_seconds`, appends the unit to the y-axis
//!   label, and formats values-table cells with [`format_duration_seconds`].

use std::fmt::Write as _;

use plotters::coord::ranged1d::SegmentValue;
use plotters::prelude::*;

use workdown_core::model::duration::format_duration_seconds;
use workdown_core::view_data::{UnplacedReason, WorkloadData, WorkloadUnit};

use crate::render::chart_common::{
    format_compact_number, hex_to_rgb, pad_extent, pick_duration_unit, strip_svg_blank_lines,
    OKABE_ITO,
};
use crate::render::common::{card_link, emit_description, format_number};

/// Plotters' segmented coords append one extra "Last" segment past the
/// data range; we need to budget pixel width to draw `n + 1` bar slots
/// so the actual bars come out at the configured per-bucket width.
const SVG_PER_BUCKET: u32 = 32;
const SVG_MIN_WIDTH: u32 = 480;
const SVG_MAX_WIDTH: u32 = 1600;
const SVG_HEIGHT: u32 = 360;
const SVG_BASE_OVERHEAD: u32 = 200;

/// At most this many x-axis tick labels — beyond it we thin the labels
/// to every Nth bucket so they don't run together. Empirical: ~12 reads
/// well at the typical 800–1200px width range we end up at.
const MAX_TICK_LABELS: usize = 12;

/// Render a [`WorkloadData`] as a Markdown string.
///
/// `item_link_base` is the relative path from the rendered view file to
/// the work items directory (same parameter every other renderer takes).
/// `description` is the one-line caption emitted below the heading.
pub fn render_workload(data: &WorkloadData, item_link_base: &str, description: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}", heading(data));
    out.push('\n');
    emit_description(description, &mut out);

    if data.buckets.is_empty() && data.unplaced.is_empty() {
        out.push_str("_(no items)_\n");
        return out;
    }

    if !data.buckets.is_empty() {
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
            let link = card_link(&unplaced.card, item_link_base);
            match &unplaced.reason {
                UnplacedReason::MissingValue { field } => {
                    let _ = writeln!(out, "- {link} — missing `{field}`");
                }
                UnplacedReason::InvalidRange {
                    start_field,
                    end_field,
                } => {
                    let _ = writeln!(
                        out,
                        "- {link} — start `{start_field}` after end `{end_field}`",
                    );
                }
                UnplacedReason::NoWorkingDays {
                    start_field,
                    end_field,
                } => {
                    let _ = writeln!(
                        out,
                        "- {link} — interval `{start_field}..{end_field}` falls entirely on non-working days",
                    );
                }
                _ => {
                    // Workload extractor only emits the three reasons above;
                    // skip anything else as defense-in-depth.
                }
            }
        }
    }

    out
}

fn heading(data: &WorkloadData) -> String {
    format!("# Workload: {} per working day", data.effort_field)
}

// ── Values table ────────────────────────────────────────────────────

fn emit_values_table(data: &WorkloadData, out: &mut String) {
    out.push_str("## Values\n\n");
    let _ = writeln!(out, "| Date | {} |", data.effort_field);
    out.push_str("| --- | --- |\n");
    for bucket in &data.buckets {
        if bucket.total == 0.0 {
            continue;
        }
        let _ = writeln!(
            out,
            "| {date} | {value} |",
            date = bucket.date.format("%Y-%m-%d"),
            value = format_total(bucket.total, data.unit),
        );
    }
}

/// Format a bucket total per the data's unit. Numbers drop trailing `.0`
/// for integer sums; durations render as the canonical `Wd Xh` shorthand.
fn format_total(total: f64, unit: WorkloadUnit) -> String {
    match unit {
        WorkloadUnit::Number => format_number(total),
        WorkloadUnit::Duration => format_duration_seconds(total.round() as i64),
    }
}

// ── SVG rendering ───────────────────────────────────────────────────

/// One step in our axis-encoding: the divisor that turns a bucket's
/// stored total into the y-axis number, and the unit label suffix that
/// gets appended to the axis title.
struct YAxis {
    divisor: f64,
    unit_label: Option<&'static str>,
}

impl YAxis {
    fn for_data(data: &WorkloadData) -> Self {
        match data.unit {
            WorkloadUnit::Number => YAxis {
                divisor: 1.0,
                unit_label: None,
            },
            WorkloadUnit::Duration => {
                // Pick a unit so the largest bar reads as a small whole-ish
                // number (`2`, `4.5`) rather than `7200` raw seconds. The
                // helper already handles zero / negative ranges defensively.
                let max_seconds = data
                    .buckets
                    .iter()
                    .map(|bucket| bucket.total.abs() as i64)
                    .max()
                    .unwrap_or(0);
                let unit = pick_duration_unit(max_seconds);
                YAxis {
                    divisor: unit.divisor_seconds as f64,
                    unit_label: Some(unit.label),
                }
            }
        }
    }
}

fn render_svg(data: &WorkloadData) -> String {
    let n = data.buckets.len();
    let y_axis = YAxis::for_data(data);

    let encoded: Vec<f64> = data
        .buckets
        .iter()
        .map(|bucket| bucket.total / y_axis.divisor)
        .collect();

    // Force-include 0 so single-positive-value charts read correctly
    // (otherwise plotters would baseline at the smallest bar). Mirrors
    // the bar_chart renderer's policy.
    let raw_min = encoded.iter().copied().fold(f64::INFINITY, f64::min);
    let raw_max = encoded.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let (raw_min, raw_max) = if !raw_min.is_finite() || !raw_max.is_finite() {
        (0.0, 1.0)
    } else {
        (raw_min.min(0.0), raw_max.max(0.0))
    };
    let (y_min, y_max) = pad_extent(raw_min, raw_max);

    let label_step = label_step_for(n);
    let labels: Vec<String> = data
        .buckets
        .iter()
        .map(|bucket| bucket.date.format("%Y-%m-%d").to_string())
        .collect();
    let y_axis_label = y_axis_label(data, &y_axis);

    let width = (SVG_BASE_OVERHEAD + (n.max(1) as u32 + 1) * SVG_PER_BUCKET)
        .clamp(SVG_MIN_WIDTH, SVG_MAX_WIDTH);
    let height = SVG_HEIGHT;

    let bar_color = hex_to_rgb(OKABE_ITO[0]);

    let mut buf = String::new();
    {
        let root = SVGBackend::with_string(&mut buf, (width, height)).into_drawing_area();
        root.fill(&WHITE).expect("fill white background");

        let mut chart = ChartBuilder::on(&root)
            .margin(20)
            .x_label_area_size(60)
            .y_label_area_size(70)
            .build_cartesian_2d((0..n as i32).into_segmented(), y_min..y_max)
            .expect("build cartesian 2d");

        chart
            .configure_mesh()
            .y_desc(y_axis_label)
            .y_label_formatter(&|value: &f64| format_compact_number(*value))
            .x_label_formatter(&|seg: &SegmentValue<i32>| match seg {
                SegmentValue::Exact(i) | SegmentValue::CenterOf(i) => {
                    let index = *i as usize;
                    // Thin labels on long ranges. Always keep the first
                    // and last bucket label so the time span is readable.
                    if index >= labels.len() {
                        return String::new();
                    }
                    if index == 0 || index == labels.len() - 1 || index.is_multiple_of(label_step) {
                        labels[index].clone()
                    } else {
                        String::new()
                    }
                }
                SegmentValue::Last => String::new(),
            })
            .x_labels(n.max(1))
            .x_label_style(("sans-serif", 12).into_font())
            .draw()
            .expect("draw mesh");

        chart
            .draw_series(encoded.iter().enumerate().map(|(i, &value)| {
                Rectangle::new(
                    [
                        (SegmentValue::Exact(i as i32), 0.0),
                        (SegmentValue::Exact(i as i32 + 1), value),
                    ],
                    bar_color.filled(),
                )
            }))
            .expect("draw bars");

        root.present().expect("present svg");
    }
    strip_svg_blank_lines(&buf)
}

fn y_axis_label(data: &WorkloadData, y_axis: &YAxis) -> String {
    match y_axis.unit_label {
        Some(unit) => format!("{} ({unit})", data.effort_field),
        None => data.effort_field.clone(),
    }
}

/// Pick a tick step so we emit at most [`MAX_TICK_LABELS`] visible
/// labels. Always includes the first and last bucket independently, so
/// the actual visible label count can exceed this bound by one — that's
/// intentional, the time-span readability matters more than a hard cap.
fn label_step_for(n: usize) -> usize {
    if n <= MAX_TICK_LABELS {
        return 1;
    }
    n.div_ceil(MAX_TICK_LABELS).max(1)
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use chrono::NaiveDate;
    use workdown_core::model::WorkItemId;
    use workdown_core::view_data::{
        Card, UnplacedCard, UnplacedReason, WorkloadBucket, WorkloadData, WorkloadUnit,
    };

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

    fn data(
        unit: WorkloadUnit,
        buckets: Vec<WorkloadBucket>,
        unplaced: Vec<UnplacedCard>,
    ) -> WorkloadData {
        WorkloadData {
            start_field: "start".to_owned(),
            end_field: "end".to_owned(),
            effort_field: "effort".to_owned(),
            unit,
            buckets,
            unplaced,
        }
    }

    fn bucket(date: NaiveDate, total: f64) -> WorkloadBucket {
        WorkloadBucket { date, total }
    }

    fn unplaced_missing(id: &str, title: Option<&str>, field: &str) -> UnplacedCard {
        UnplacedCard {
            card: card(id, title),
            reason: UnplacedReason::MissingValue {
                field: field.to_owned(),
            },
        }
    }

    fn unplaced_invalid_range(id: &str) -> UnplacedCard {
        UnplacedCard {
            card: card(id, None),
            reason: UnplacedReason::InvalidRange {
                start_field: "start".to_owned(),
                end_field: "end".to_owned(),
            },
        }
    }

    fn unplaced_no_working_days(id: &str) -> UnplacedCard {
        UnplacedCard {
            card: card(id, None),
            reason: UnplacedReason::NoWorkingDays {
                start_field: "start".to_owned(),
                end_field: "end".to_owned(),
            },
        }
    }

    // ── Heading / empty / description ───────────────────────────────

    #[test]
    fn heading_uses_effort_field_name() {
        let output = render_workload(
            &data(WorkloadUnit::Number, vec![], vec![]),
            "../workdown-items",
            "",
        );
        assert!(output.starts_with("# Workload: effort per working day\n"));
    }

    #[test]
    fn empty_view_emits_no_items_marker_and_no_svg() {
        let output = render_workload(
            &data(WorkloadUnit::Number, vec![], vec![]),
            "../workdown-items",
            "",
        );
        assert!(output.contains("_(no items)_"));
        assert!(!output.contains("<svg"));
    }

    #[test]
    fn description_emitted_under_heading() {
        let output = render_workload(
            &data(WorkloadUnit::Number, vec![], vec![]),
            "../workdown-items",
            "Daily load.",
        );
        assert!(output.contains("# Workload: effort per working day\n\nDaily load.\n\n"));
    }

    // ── Number-valued buckets ───────────────────────────────────────

    #[test]
    fn number_buckets_emit_svg_with_first_palette_color() {
        let buckets = vec![bucket(ymd(2026, 1, 5), 2.0), bucket(ymd(2026, 1, 6), 4.0)];
        let output = render_workload(
            &data(WorkloadUnit::Number, buckets, vec![]),
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
    fn number_y_axis_label_omits_unit() {
        let buckets = vec![bucket(ymd(2026, 1, 5), 2.0)];
        let output = render_workload(
            &data(WorkloadUnit::Number, buckets, vec![]),
            "../workdown-items",
            "",
        );
        // For Number unit the y-axis label is the bare field name with
        // no parenthesised duration suffix — `effort (hours)`, `effort
        // (days)`, etc. Checking the unique-to-Duration form is absent
        // is more reliable than asserting on plotters' split <text>
        // serialization of the bare label.
        assert!(
            !output.contains("effort ("),
            "Number unit must not produce a parenthesised unit suffix, got: {output}"
        );
    }

    // ── Duration-valued buckets ─────────────────────────────────────

    #[test]
    fn duration_y_axis_label_includes_unit() {
        // Two buckets of 2 hours each → max is 7200 seconds → "hours".
        let buckets = vec![
            bucket(ymd(2026, 1, 5), 7200.0),
            bucket(ymd(2026, 1, 6), 7200.0),
        ];
        let output = render_workload(
            &data(WorkloadUnit::Duration, buckets, vec![]),
            "../workdown-items",
            "",
        );
        assert!(
            output.contains("effort (hours)"),
            "expected y-axis label 'effort (hours)' in {output}"
        );
    }

    #[test]
    fn duration_picks_days_for_larger_ranges() {
        // 1 day per bucket → "days".
        let buckets = vec![
            bucket(ymd(2026, 1, 5), 86_400.0),
            bucket(ymd(2026, 1, 6), 86_400.0),
        ];
        let output = render_workload(
            &data(WorkloadUnit::Duration, buckets, vec![]),
            "../workdown-items",
            "",
        );
        assert!(output.contains("effort (days)"));
    }

    // ── Values table ────────────────────────────────────────────────

    #[test]
    fn values_table_lists_each_non_zero_bucket() {
        let buckets = vec![
            bucket(ymd(2026, 1, 5), 2.0),
            bucket(ymd(2026, 1, 6), 0.0),
            bucket(ymd(2026, 1, 7), 5.0),
        ];
        let output = render_workload(
            &data(WorkloadUnit::Number, buckets, vec![]),
            "../workdown-items",
            "",
        );
        assert!(output.contains("## Values\n"));
        assert!(output.contains("| Date | effort |"));
        assert!(output.contains("| 2026-01-05 | 2 |"));
        assert!(output.contains("| 2026-01-07 | 5 |"));
        // Zero-total buckets are intentionally omitted from the table to
        // keep long ranges readable; they still appear on the SVG axis.
        assert!(!output.contains("| 2026-01-06 |"));
    }

    #[test]
    fn values_table_formats_durations_as_shorthand() {
        let buckets = vec![bucket(ymd(2026, 1, 5), 86_400.0 + 3_600.0)];
        let output = render_workload(
            &data(WorkloadUnit::Duration, buckets, vec![]),
            "../workdown-items",
            "",
        );
        assert!(
            output.contains("| 2026-01-05 | 1d 1h |"),
            "expected duration shorthand cell, got: {output}"
        );
    }

    #[test]
    fn values_table_drops_trailing_zero_decimal_for_integer_totals() {
        let buckets = vec![bucket(ymd(2026, 1, 5), 7.0)];
        let output = render_workload(
            &data(WorkloadUnit::Number, buckets, vec![]),
            "../workdown-items",
            "",
        );
        assert!(output.contains("| 2026-01-05 | 7 |"));
    }

    // ── Unplaced footer ─────────────────────────────────────────────

    #[test]
    fn unplaced_footer_covers_all_three_reasons() {
        let buckets = vec![bucket(ymd(2026, 1, 5), 1.0)];
        let unplaced = vec![
            unplaced_missing("a-missing", Some("Missing"), "effort"),
            unplaced_invalid_range("b-bad-range"),
            unplaced_no_working_days("c-weekend"),
        ];
        let output = render_workload(
            &data(WorkloadUnit::Number, buckets, unplaced),
            "../workdown-items",
            "",
        );
        assert!(output.contains("## Unplaced\n"));
        assert!(
            output.contains("[Missing](../workdown-items/a-missing.md) — missing `effort`"),
            "missing-value line missing in: {output}"
        );
        assert!(
            output.contains(
                "[b-bad-range](../workdown-items/b-bad-range.md) — start `start` after end `end`"
            ),
            "invalid-range line missing in: {output}"
        );
        assert!(
            output.contains("[c-weekend](../workdown-items/c-weekend.md) — interval `start..end` falls entirely on non-working days"),
            "no-working-days line missing in: {output}"
        );
    }

    #[test]
    fn no_unplaced_section_when_clean() {
        let buckets = vec![bucket(ymd(2026, 1, 5), 1.0)];
        let output = render_workload(
            &data(WorkloadUnit::Number, buckets, vec![]),
            "../workdown-items",
            "",
        );
        assert!(!output.contains("Unplaced"));
    }

    #[test]
    fn only_unplaced_emits_footer_without_svg_or_table() {
        let output = render_workload(
            &data(
                WorkloadUnit::Number,
                vec![],
                vec![unplaced_missing("orphan", Some("Orphan"), "start")],
            ),
            "../workdown-items",
            "",
        );
        assert!(!output.contains("<svg"));
        assert!(!output.contains("## Values"));
        assert!(output.contains("## Unplaced\n"));
        assert!(output.contains("[Orphan](../workdown-items/orphan.md) — missing `start`"));
    }

    // ── Tick thinning ───────────────────────────────────────────────

    #[test]
    fn label_step_keeps_full_density_for_short_ranges() {
        assert_eq!(label_step_for(0), 1);
        assert_eq!(label_step_for(5), 1);
        assert_eq!(label_step_for(MAX_TICK_LABELS), 1);
    }

    #[test]
    fn label_step_thins_out_for_long_ranges() {
        // 24 buckets / 12 cap → step 2.
        assert_eq!(label_step_for(24), 2);
        // 60 buckets / 12 cap → step 5.
        assert_eq!(label_step_for(60), 5);
    }
}
