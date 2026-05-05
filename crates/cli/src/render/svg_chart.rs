//! Shared plotters/SVG helpers for renderers that emit charts —
//! `bar_chart`, `line_chart`, `heatmap`, `workload`.
//!
//! Color palette, axis kinds, duration-unit picker, range padding, tick
//! number formatting, and hex-to-RGB conversion. Renderer-specific layout
//! (chart dimensions, axis placement) stays in each renderer module.

use chrono::{Datelike, NaiveDate};
use plotters::style::RGBColor;

use workdown_core::model::duration::format_duration_seconds;
use workdown_core::view_data::AggregateValue;

use super::markdown::format_number;

/// Color-blind-safe categorical palette by Okabe & Ito (2008).
///
/// Eight distinct hues that remain distinguishable for the most common
/// forms of color-vision deficiency. Used in series-sort order and
/// recycled (i.e. `OKABE_ITO[i % 8]`) when a view has more than 8 groups.
pub const OKABE_ITO: [&str; 8] = [
    "#E69F00", // orange
    "#56B4E9", // sky blue
    "#009E73", // bluish green
    "#F0E442", // yellow
    "#0072B2", // blue
    "#D55E00", // vermillion
    "#CC79A7", // reddish purple
    "#000000", // black
];

/// How an axis converts source values to f64 plot coordinates and back
/// to display strings.
#[derive(Debug, Clone, Copy)]
pub enum AxisKind {
    /// Numeric — pass through as f64; tick labels use `format_compact_number`.
    Number,
    /// Date — `(date - 0001-01-01).num_days() as f64`; tick labels use `YYYY-MM-DD`.
    Date,
    /// Duration — divide canonical seconds by `divisor`; tick labels are the
    /// quotient with at most two decimals; axis label appends the unit.
    Duration { divisor: i64, label: &'static str },
}

pub const SECONDS_PER_MINUTE: i64 = 60;
pub const SECONDS_PER_HOUR: i64 = 3_600;
pub const SECONDS_PER_DAY: i64 = 86_400;
pub const SECONDS_PER_WEEK: i64 = 604_800;

/// Choice of unit for an axis backed by a `Duration` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurationUnit {
    pub divisor_seconds: i64,
    pub label: &'static str,
}

/// Pick the largest unit whose count for `max_seconds` is at least 1.
///
/// Goal: render axis ticks as small whole numbers in a single, consistent
/// unit — `0`, `1`, `2`, `3` weeks, not `0`, `604800`, `1209600` seconds.
/// Callers divide every duration value by `divisor_seconds` (as f64) to
/// plot, and append `label` to the axis title.
///
/// `0` and negative ranges fall through to seconds — the chart still
/// plots, and the renderer doesn't have to special-case the empty axis.
pub fn pick_duration_unit(max_seconds: i64) -> DurationUnit {
    let abs = max_seconds.unsigned_abs() as i64;
    if abs >= SECONDS_PER_WEEK {
        DurationUnit {
            divisor_seconds: SECONDS_PER_WEEK,
            label: "weeks",
        }
    } else if abs >= SECONDS_PER_DAY {
        DurationUnit {
            divisor_seconds: SECONDS_PER_DAY,
            label: "days",
        }
    } else if abs >= SECONDS_PER_HOUR {
        DurationUnit {
            divisor_seconds: SECONDS_PER_HOUR,
            label: "hours",
        }
    } else if abs >= SECONDS_PER_MINUTE {
        DurationUnit {
            divisor_seconds: SECONDS_PER_MINUTE,
            label: "minutes",
        }
    } else {
        DurationUnit {
            divisor_seconds: 1,
            label: "seconds",
        }
    }
}

/// Compute (min, max) over an iterator of finite f64s. Falls back to
/// `(0.0, 1.0)` when empty so plotters always has a non-zero range.
pub fn numeric_extent<I: Iterator<Item = f64>>(values: I) -> (f64, f64) {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for value in values {
        if value.is_finite() {
            if value < min {
                min = value;
            }
            if value > max {
                max = value;
            }
        }
    }
    if !min.is_finite() || !max.is_finite() {
        return (0.0, 1.0);
    }
    (min, max)
}

/// Pad a numeric extent so points don't sit on the chart edges. Uses 5%
/// of the range on each side; collapses degenerate ranges to a unit
/// window so plotters has somewhere to draw ticks.
pub fn pad_extent(min: f64, max: f64) -> (f64, f64) {
    if (max - min).abs() < f64::EPSILON {
        // All values identical — give each side a 0.5 unit margin.
        return (min - 0.5, max + 0.5);
    }
    let pad = (max - min) * 0.05;
    (min - pad, max + pad)
}

/// Days since CE epoch, used as the f64 encoding for date axes.
pub fn date_to_f64(date: NaiveDate) -> f64 {
    date.num_days_from_ce() as f64
}

/// Format a single axis tick. Bridge from plot-space f64 back to a human
/// label per the axis kind.
///
/// Numeric and duration ticks both pass through `format_compact_number` so
/// that plotters' fractional tick generation (`2.6` round-tripping as
/// `2.5999999999999996`) doesn't leak into the rendered SVG. Date ticks
/// round to the nearest whole day.
pub fn format_axis_tick(value: f64, kind: AxisKind) -> String {
    match kind {
        AxisKind::Number | AxisKind::Duration { .. } => format_compact_number(value),
        AxisKind::Date => {
            let days = value.round() as i32;
            match NaiveDate::from_num_days_from_ce_opt(days) {
                Some(date) => date.format("%Y-%m-%d").to_string(),
                None => String::new(),
            }
        }
    }
}

/// Compose the axis title shown to the user. Number/date axes show the
/// raw field name; duration axes append the chosen unit so readers know
/// what the tick numbers mean.
pub fn axis_label(field: &str, kind: AxisKind) -> String {
    match kind {
        AxisKind::Number | AxisKind::Date => field.to_owned(),
        AxisKind::Duration { label, .. } => format!("{field} ({label})"),
    }
}

/// Format a tick-friendly number: integers drop their decimal, non-
/// integers round to two decimals and trim trailing zeros.
///
/// Plotters generates floating-point tick values that don't always have
/// clean decimal expansions (`2.6` arrives as `2.5999999999999996`).
/// `format_number` from `render::markdown` is for treemap-style sums and
/// passes those through verbatim — fine there, ugly on a chart axis.
pub fn format_compact_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value:.2}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    }
}

/// Pick the axis kind for a stream of [`AggregateValue`]s.
///
/// Variant comes from the first value; for `Duration`, the unit is
/// chosen so the largest absolute magnitude becomes a small whole
/// number. Single-pass over the iterator. Panics on an empty stream
/// — every caller has a non-empty data set by the time it asks.
pub fn axis_kind_for(values: impl Iterator<Item = AggregateValue>) -> AxisKind {
    let mut iter = values;
    let first = iter.next().expect("axis_kind_for called with no values");
    match first {
        AggregateValue::Number(_) => AxisKind::Number,
        AggregateValue::Date(_) => AxisKind::Date,
        AggregateValue::Duration(seconds_first) => {
            let max = std::iter::once(seconds_first)
                .chain(iter.filter_map(|value| match value {
                    AggregateValue::Duration(seconds) => Some(seconds),
                    _ => None,
                }))
                .map(|seconds| seconds.unsigned_abs() as i64)
                .max()
                .unwrap_or(0);
            let unit = pick_duration_unit(max);
            AxisKind::Duration {
                divisor: unit.divisor_seconds,
                label: unit.label,
            }
        }
    }
}

/// Convert an [`AggregateValue`] to the f64 plot-space coordinate that
/// matches `kind`. Mismatched variant + kind is a programming error
/// (every caller derives `kind` from the same value stream) and panics.
pub fn value_to_f64(value: AggregateValue, kind: AxisKind) -> f64 {
    match (value, kind) {
        (AggregateValue::Number(n), AxisKind::Number) => n,
        (AggregateValue::Date(date), AxisKind::Date) => date_to_f64(date),
        (AggregateValue::Duration(seconds), AxisKind::Duration { divisor, .. }) => {
            seconds as f64 / divisor as f64
        }
        (value, kind) => panic!("mixed aggregate value types: value {value:?} with kind {kind:?}"),
    }
}

/// Format an [`AggregateValue`] for display in a Markdown table cell.
///
/// Numbers go through [`format_number`] (drops trailing `.0`), dates
/// render as ISO `YYYY-MM-DD`, durations as the canonical `Wd Xh Ym Zs`
/// shorthand from [`format_duration_seconds`].
pub fn format_aggregate_value(value: &AggregateValue) -> String {
    match value {
        AggregateValue::Number(n) => format_number(*n),
        AggregateValue::Date(d) => d.format("%Y-%m-%d").to_string(),
        AggregateValue::Duration(seconds) => format_duration_seconds(*seconds),
    }
}

/// Strip blank lines from a plotters SVG buffer.
///
/// Inline `<svg>` in our Markdown output is treated as a CommonMark
/// HTML block of type 7, which **terminates at the first blank line**.
/// Plotters emits blank lines inside empty `<text>` elements (e.g.
/// ticks whose label formatter returned `""` for the segmented coord's
/// `Last` slot, or for any tick beyond the data range). Without this
/// scrub, the markdown parser would close the HTML block early and
/// re-render the rest of the SVG's text content — axis labels, tick
/// numbers, colorbar ticks — as plain text *below* the figure.
pub fn strip_svg_blank_lines(svg: &str) -> String {
    let mut out = String::with_capacity(svg.len());
    for line in svg.lines() {
        if line.trim().is_empty() {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Convert a `#RRGGBB` palette entry into the plotters `RGBColor` form.
///
/// Panics on a malformed entry — `OKABE_ITO` is the only caller and a
/// test asserts every entry parses, so a panic here is a programming
/// error caught at test time.
pub fn hex_to_rgb(hex: &str) -> RGBColor {
    let bytes = hex
        .strip_prefix('#')
        .expect("palette color should start with '#'");
    assert_eq!(bytes.len(), 6, "palette color should be #RRGGBB");
    let r = u8::from_str_radix(&bytes[0..2], 16).expect("valid hex r");
    let g = u8::from_str_radix(&bytes[2..4], 16).expect("valid hex g");
    let b = u8::from_str_radix(&bytes[4..6], 16).expect("valid hex b");
    RGBColor(r, g, b)
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Palette ─────────────────────────────────────────────────────

    #[test]
    fn okabe_ito_has_eight_distinct_colors() {
        let mut sorted: Vec<&str> = OKABE_ITO.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 8, "palette should have 8 distinct colors");
    }

    #[test]
    fn okabe_ito_entries_are_valid_hex() {
        for color in OKABE_ITO {
            let parsed = hex_to_rgb(color);
            // Smoke test: any panic during parsing fails the test.
            let _ = parsed;
        }
    }

    // ── Duration unit picker ────────────────────────────────────────

    #[test]
    fn pick_duration_unit_seconds_for_small_values() {
        assert_eq!(pick_duration_unit(45).label, "seconds");
    }

    #[test]
    fn pick_duration_unit_minutes_at_60_seconds() {
        assert_eq!(pick_duration_unit(60).label, "minutes");
    }

    #[test]
    fn pick_duration_unit_hours_at_one_hour() {
        assert_eq!(pick_duration_unit(SECONDS_PER_HOUR).label, "hours");
    }

    #[test]
    fn pick_duration_unit_days_at_one_day() {
        assert_eq!(pick_duration_unit(SECONDS_PER_DAY).label, "days");
    }

    #[test]
    fn pick_duration_unit_weeks_at_one_week() {
        assert_eq!(pick_duration_unit(SECONDS_PER_WEEK).label, "weeks");
    }

    #[test]
    fn pick_duration_unit_weeks_for_large_values() {
        assert_eq!(pick_duration_unit(4 * SECONDS_PER_WEEK).label, "weeks");
    }

    #[test]
    fn pick_duration_unit_zero_falls_through_to_seconds() {
        assert_eq!(pick_duration_unit(0).label, "seconds");
    }

    #[test]
    fn pick_duration_unit_negative_uses_absolute_magnitude() {
        assert_eq!(pick_duration_unit(-2 * SECONDS_PER_DAY).label, "days");
    }

    // ── Tick formatter ──────────────────────────────────────────────

    #[test]
    fn format_axis_tick_number_drops_decimal_for_integers() {
        assert_eq!(format_axis_tick(3.0, AxisKind::Number), "3");
    }

    #[test]
    fn format_axis_tick_date_renders_iso() {
        let date = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
        let tick = format_axis_tick(date_to_f64(date), AxisKind::Date);
        assert_eq!(tick, "2026-01-05");
    }

    #[test]
    fn format_axis_tick_duration_drops_decimal_for_integers() {
        let unit = pick_duration_unit(2 * SECONDS_PER_DAY);
        let kind = AxisKind::Duration {
            divisor: unit.divisor_seconds,
            label: unit.label,
        };
        // 2.0 days → "2"
        assert_eq!(format_axis_tick(2.0, kind), "2");
    }

    #[test]
    fn axis_label_appends_unit_for_duration() {
        let unit = pick_duration_unit(2 * SECONDS_PER_DAY);
        let kind = AxisKind::Duration {
            divisor: unit.divisor_seconds,
            label: unit.label,
        };
        assert_eq!(axis_label("estimate", kind), "estimate (days)");
    }

    #[test]
    fn axis_label_for_number_is_field_name() {
        assert_eq!(axis_label("score", AxisKind::Number), "score");
    }

    #[test]
    fn axis_label_for_date_is_field_name() {
        assert_eq!(axis_label("day", AxisKind::Date), "day");
    }

    // ── Padding ─────────────────────────────────────────────────────

    #[test]
    fn pad_extent_collapses_zero_range_to_unit_window() {
        let (min, max) = pad_extent(5.0, 5.0);
        assert!(max - min > 0.5, "should give a 1-unit window");
    }

    #[test]
    fn pad_extent_adds_5_percent_padding() {
        let (min, max) = pad_extent(0.0, 100.0);
        // 5% of 100 = 5
        assert!((min - -5.0).abs() < f64::EPSILON);
        assert!((max - 105.0).abs() < f64::EPSILON);
    }
}
