//! Heatmap renderer — turns [`HeatmapData`] into a Markdown document
//! with a per-cell SVG grid (plotters-backed) and a pivoted `## Values`
//! table for the exact aggregated numbers.
//!
//! Heading variants:
//! - `Aggregate::Count` → `# Heatmap: count by <x> × <y>`
//! - other → `# Heatmap: <aggregate> of <value> by <x> × <y>`
//!
//! Layout: x and y are both segmented categorical axes; cells render as
//! `Rectangle`s, one per (x, y) intersection. Cells with no data point
//! draw in a neutral gray (`#EEEEEE`) so the grid is solid and missing
//! data is visually distinct from a real zero. The y axis is flipped
//! relative to plotters' default cartesian orientation so `y_labels[0]`
//! sits at the top of the grid (table-style), matching the pivoted
//! values table beneath.
//!
//! Color scale: sequential (white → Okabe-Ito blue) when every value is
//! ≥ 0; diverging (Okabe-Ito vermillion → white → blue, anchored at 0)
//! when any value is negative. Diverging is symmetric — the gradient
//! extent on each side is `max(|min|, max)` — so equal magnitudes get
//! equal saturation regardless of whether the data leans positive or
//! negative.
//!
//! A vertical colorbar to the right of the grid maps the gradient back
//! to numeric tick labels (formatted per the value's [`AxisKind`]).
//! Plotters has no built-in colorbar widget so it's drawn as a stack of
//! thin gradient rectangles in its own split-off drawing area.
//!
//! Tick labels stay horizontal. Plotters only ships 90°/180°/270° font
//! transforms (no 45°), and rotated labels with the default
//! `text-anchor="middle"` straddle the rotation pivot — so half the
//! label drifts back into the plot area and gets covered by cells. If
//! a future heatmap needs to fit much longer x labels (e.g. wide
//! grids of ISO weeks), revisit with a custom label-positioning pass
//! rather than the built-in `FontTransform`.
//!
//! The grid uses plotters' segmented categorical coords. That coord
//! type allocates one extra "Last" segment beyond the data range, which
//! shows up as a one-cell empty band on one edge of the plot — visually
//! that lands as a thin padding row above the topmost label and a thin
//! padding column to the right of the rightmost label. Acceptable
//! cosmetic cost in exchange for plotters' built-in tick placement at
//! cell centers (`CenterOf(i)`), which avoids the empty-tick clutter
//! that a plain f64 range produces with our hand-rolled half-integer
//! formatter.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use plotters::coord::ranged1d::SegmentValue;
use plotters::coord::Shift;
use plotters::prelude::*;

use workdown_core::model::views::Aggregate;
use workdown_core::view_data::{AggregateValue, HeatmapData, UnplacedReason};

use crate::render::chart_common::{
    axis_kind_for, format_aggregate_value, format_axis_tick, hex_to_rgb, strip_svg_blank_lines,
    value_to_f64, AxisKind,
};
use crate::render::common::{card_link, emit_description};

const CELL_PX: u32 = 40;
const Y_LABEL_AREA: u32 = 120;
const X_LABEL_AREA: u32 = 50;
const TOP_MARGIN: u32 = 20;
const RIGHT_PADDING: u32 = 20;
const COLORBAR_AREA: u32 = 130;
const SVG_MAX_DIM: u32 = 1200;

const EMPTY_CELL_HEX: &str = "#EEEEEE";
/// White end of the sequential ramp + center of the diverging ramp.
const WHITE_RGB: RGBColor = RGBColor(255, 255, 255);
/// Saturated end of the sequential ramp + positive end of the diverging ramp.
const POSITIVE_HEX: &str = "#0072B2";
/// Negative end of the diverging ramp.
const NEGATIVE_HEX: &str = "#D55E00";

/// Render a `HeatmapData` as a Markdown string.
///
/// `item_link_base` is the relative path from the rendered view file to
/// the work items directory — same parameter as `render_bar_chart`.
/// `description` is the one-line caption emitted below the heading.
pub fn render_heatmap(data: &HeatmapData, item_link_base: &str, description: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("{}\n\n", heading(data)));
    emit_description(description, &mut out);

    if data.cells.is_empty() && data.unplaced.is_empty() {
        out.push_str("_(no items)_\n");
        return out;
    }

    if !data.cells.is_empty() {
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
                // The heatmap extractor only emits MissingValue today;
                // listing the rest explicitly so adding a new variant
                // fails compilation here and prompts an audit.
                UnplacedReason::InvalidRange { .. }
                | UnplacedReason::NoWorkingDays { .. }
                | UnplacedReason::NonNumericValue { .. }
                | UnplacedReason::NoAnchor
                | UnplacedReason::PredecessorUnresolved { .. }
                | UnplacedReason::Cycle { .. } => {}
            }
        }
    }

    out
}

fn heading(data: &HeatmapData) -> String {
    match data.aggregate {
        Aggregate::Count => format!("# Heatmap: count by {} × {}", data.x_field, data.y_field),
        agg => match &data.value_field {
            Some(value) => format!(
                "# Heatmap: {agg} of {value} by {} × {}",
                data.x_field, data.y_field
            ),
            None => format!("# Heatmap: {agg} by {} × {}", data.x_field, data.y_field),
        },
    }
}

// ── Values table ────────────────────────────────────────────────────

/// Emit the pivoted Markdown table: x labels as columns, y labels as
/// rows, blank cells where the (x, y) intersection has no data.
fn emit_values_table(data: &HeatmapData, out: &mut String) {
    out.push_str("## Values\n\n");

    // Header row: corner cell names both axes for readers landing on
    // the rendered file with no other context.
    let _ = write!(
        out,
        "| {} / {} |",
        escape_cell(&data.y_field),
        escape_cell(&data.x_field)
    );
    for x_label in &data.x_labels {
        let _ = write!(out, " {} |", escape_cell(x_label));
    }
    out.push('\n');

    out.push('|');
    for _ in 0..data.x_labels.len() + 1 {
        out.push_str(" --- |");
    }
    out.push('\n');

    let cell_lookup: BTreeMap<(&str, &str), &AggregateValue> = data
        .cells
        .iter()
        .map(|cell| ((cell.x.as_str(), cell.y.as_str()), &cell.value))
        .collect();

    for y_label in &data.y_labels {
        let _ = write!(out, "| {} |", escape_cell(y_label));
        for x_label in &data.x_labels {
            let cell_str = cell_lookup
                .get(&(x_label.as_str(), y_label.as_str()))
                .map(|value| format_aggregate_value(value))
                .unwrap_or_default();
            let _ = write!(out, " {cell_str} |");
        }
        out.push('\n');
    }
}

/// Neutralize `|` (would end the cell) and newlines (would end the row)
/// inside a Markdown table cell. Mirrors the bar chart renderer's escape.
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

fn render_svg(data: &HeatmapData) -> String {
    let kind = axis_kind_for(data.cells.iter().map(|c| c.value));
    let cell_lookup = encode_cells(data, kind);

    let n_x = data.x_labels.len();
    let n_y = data.y_labels.len();

    let (vmin, vmax) = compute_extent(cell_lookup.values().copied());
    let scheme = ColorScheme::pick(vmin, vmax);

    // Segmented coord adds one extra "Last" segment beyond the data
    // range, so the chart inner needs `(n + 1) * CELL_PX` for cells to
    // come out CELL_PX each (rather than getting compressed into a
    // smaller share of the plot).
    let chart_w = (((n_x as u32 + 1) * CELL_PX) + Y_LABEL_AREA + RIGHT_PADDING).min(SVG_MAX_DIM);
    let total_w = (chart_w + COLORBAR_AREA).min(SVG_MAX_DIM);
    let total_h = (((n_y as u32 + 1) * CELL_PX) + TOP_MARGIN + X_LABEL_AREA).min(SVG_MAX_DIM);

    let mut buf = String::new();
    {
        let root = SVGBackend::with_string(&mut buf, (total_w, total_h)).into_drawing_area();
        root.fill(&WHITE).expect("fill white background");
        let (chart_area, colorbar_area) = root.split_horizontally(chart_w);

        draw_grid(&chart_area, data, &cell_lookup, n_x, n_y, scheme);
        draw_colorbar(&colorbar_area, scheme, kind);

        root.present().expect("present svg");
    }
    strip_svg_blank_lines(&buf)
}

/// Index every cell by its (x_label_index, y_label_index) so the grid
/// renderer can do O(1) lookups during the full Cartesian product walk.
fn encode_cells(data: &HeatmapData, kind: AxisKind) -> BTreeMap<(usize, usize), f64> {
    let x_index: BTreeMap<&str, usize> = data
        .x_labels
        .iter()
        .enumerate()
        .map(|(i, label)| (label.as_str(), i))
        .collect();
    let y_index: BTreeMap<&str, usize> = data
        .y_labels
        .iter()
        .enumerate()
        .map(|(i, label)| (label.as_str(), i))
        .collect();
    data.cells
        .iter()
        .map(|cell| {
            let xi = *x_index
                .get(cell.x.as_str())
                .expect("cell x label must be in x_labels");
            let yi = *y_index
                .get(cell.y.as_str())
                .expect("cell y label must be in y_labels");
            ((xi, yi), value_to_f64(cell.value, kind))
        })
        .collect()
}

fn compute_extent(values: impl Iterator<Item = f64>) -> (f64, f64) {
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

#[derive(Clone, Copy)]
enum ColorScheme {
    /// Single-hue ramp from white (at `min`) to `POSITIVE_HEX` (at `max`).
    Sequential { min: f64, max: f64 },
    /// Two-hue ramp anchored at zero. Each side spans `[0, abs_max]` so
    /// equal magnitudes get equal saturation regardless of asymmetry.
    Diverging { abs_max: f64 },
}

impl ColorScheme {
    fn pick(min: f64, max: f64) -> Self {
        if min < 0.0 {
            ColorScheme::Diverging {
                abs_max: max.max(-min),
            }
        } else {
            ColorScheme::Sequential { min, max }
        }
    }

    fn color_for(self, value: f64) -> RGBColor {
        match self {
            ColorScheme::Sequential { min, max } => {
                let t = if (max - min).abs() < f64::EPSILON {
                    1.0
                } else {
                    ((value - min) / (max - min)).clamp(0.0, 1.0)
                };
                interp(WHITE_RGB, hex_to_rgb(POSITIVE_HEX), t)
            }
            ColorScheme::Diverging { abs_max } => {
                if abs_max < f64::EPSILON {
                    return WHITE_RGB;
                }
                if value >= 0.0 {
                    let t = (value / abs_max).clamp(0.0, 1.0);
                    interp(WHITE_RGB, hex_to_rgb(POSITIVE_HEX), t)
                } else {
                    let t = (-value / abs_max).clamp(0.0, 1.0);
                    interp(WHITE_RGB, hex_to_rgb(NEGATIVE_HEX), t)
                }
            }
        }
    }

    /// Numeric extent for the colorbar: matches what `color_for` saturates
    /// at on each side. Sequential uses the data's [min, max]; diverging
    /// uses the symmetric [-abs_max, +abs_max].
    fn axis_extent(self) -> (f64, f64) {
        match self {
            ColorScheme::Sequential { min, max } => (min, max),
            ColorScheme::Diverging { abs_max } => (-abs_max, abs_max),
        }
    }
}

fn interp(a: RGBColor, b: RGBColor, t: f64) -> RGBColor {
    let lerp = |x: u8, y: u8| -> u8 {
        let value = (x as f64) * (1.0 - t) + (y as f64) * t;
        value.round().clamp(0.0, 255.0) as u8
    };
    RGBColor(lerp(a.0, b.0), lerp(a.1, b.1), lerp(a.2, b.2))
}

fn draw_grid(
    area: &DrawingArea<SVGBackend, Shift>,
    data: &HeatmapData,
    cell_lookup: &BTreeMap<(usize, usize), f64>,
    n_x: usize,
    n_y: usize,
    scheme: ColorScheme,
) {
    let empty_color = hex_to_rgb(EMPTY_CELL_HEX);
    let x_labels = data.x_labels.clone();
    let y_labels = data.y_labels.clone();

    let mut chart = ChartBuilder::on(area)
        .margin(TOP_MARGIN as i32)
        .x_label_area_size(X_LABEL_AREA)
        .y_label_area_size(Y_LABEL_AREA)
        .build_cartesian_2d(
            (0..n_x as i32).into_segmented(),
            (0..n_y as i32).into_segmented(),
        )
        .expect("build cartesian 2d");

    // Fix font size explicitly: plotters' default shrinks text to fit
    // the available label area, which produces unreadably small labels
    // (≈6 px) when the longest tick label is long.
    let x_label_style = ("sans-serif", 14).into_font();
    let y_label_style = ("sans-serif", 14).into_font();

    chart
        .configure_mesh()
        .disable_mesh()
        .x_label_formatter(&|seg: &SegmentValue<i32>| match seg {
            SegmentValue::CenterOf(i) | SegmentValue::Exact(i) => {
                x_labels.get(*i as usize).cloned().unwrap_or_default()
            }
            SegmentValue::Last => String::new(),
        })
        .y_label_formatter(&|seg: &SegmentValue<i32>| match seg {
            SegmentValue::CenterOf(i) | SegmentValue::Exact(i) => {
                // Flip so y_labels[0] reads at the top of the SVG.
                let flipped = n_y as i64 - 1 - *i as i64;
                if flipped < 0 {
                    return String::new();
                }
                y_labels.get(flipped as usize).cloned().unwrap_or_default()
            }
            SegmentValue::Last => String::new(),
        })
        .x_labels(n_x.max(1))
        .y_labels(n_y.max(1))
        .x_label_style(x_label_style)
        .y_label_style(y_label_style)
        .draw()
        .expect("draw mesh");

    // Solid fill across the full Cartesian product so missing cells
    // render in `EMPTY_CELL_HEX` rather than leaving a transparent gap.
    let cells_iter = (0..n_x).flat_map(|x| (0..n_y).map(move |y| (x, y)));
    chart
        .draw_series(cells_iter.map(|(x, y)| {
            let color = match cell_lookup.get(&(x, y)) {
                Some(&value) => scheme.color_for(value),
                None => empty_color,
            };
            // Flip y so y_labels[0] sits at the top.
            let y_seg = (n_y - 1 - y) as i32;
            Rectangle::new(
                [
                    (SegmentValue::Exact(x as i32), SegmentValue::Exact(y_seg)),
                    (
                        SegmentValue::Exact(x as i32 + 1),
                        SegmentValue::Exact(y_seg + 1),
                    ),
                ],
                color.filled(),
            )
        }))
        .expect("draw cells");
}

fn draw_colorbar(area: &DrawingArea<SVGBackend, Shift>, scheme: ColorScheme, kind: AxisKind) {
    let (y_min, y_max) = scheme.axis_extent();
    // Defensive against degenerate ranges: plotters needs a non-zero span.
    let y_max = if (y_max - y_min).abs() < f64::EPSILON {
        y_min + 1.0
    } else {
        y_max
    };

    let mut chart = ChartBuilder::on(area)
        .margin_top(TOP_MARGIN as i32)
        .margin_bottom(X_LABEL_AREA as i32)
        .x_label_area_size(0)
        .y_label_area_size(60)
        .build_cartesian_2d(0.0..1.0, y_min..y_max)
        .expect("build colorbar");

    chart
        .configure_mesh()
        .disable_x_mesh()
        .disable_y_mesh()
        .x_labels(0)
        .y_labels(5)
        .y_label_style(("sans-serif", 13).into_font())
        .y_label_formatter(&|value: &f64| format_axis_tick(*value, kind))
        .draw()
        .expect("draw colorbar mesh");

    // 64 thin horizontal stripes is enough resolution for the eye to
    // read the gradient as continuous at typical SVG sizes.
    const N_STRIPES: usize = 64;
    let span = y_max - y_min;
    chart
        .draw_series((0..N_STRIPES).map(|i| {
            let t0 = i as f64 / N_STRIPES as f64;
            let t1 = (i + 1) as f64 / N_STRIPES as f64;
            let y0 = y_min + t0 * span;
            let y1 = y_min + t1 * span;
            let mid = y_min + ((t0 + t1) / 2.0) * span;
            Rectangle::new([(0.0, y0), (1.0, y1)], scheme.color_for(mid).filled())
        }))
        .expect("draw colorbar stripes");
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use workdown_core::model::views::{Aggregate, Bucket};
    use workdown_core::model::WorkItemId;
    use workdown_core::view_data::{
        AggregateValue, Card, HeatmapCell, HeatmapData, UnplacedCard, UnplacedReason,
    };

    fn card(id: &str, title: Option<&str>) -> Card {
        Card {
            id: WorkItemId::from(id.to_owned()),
            title: title.map(str::to_owned),
            fields: vec![],
            body: String::new(),
        }
    }

    fn cell(x: &str, y: &str, value: AggregateValue) -> HeatmapCell {
        HeatmapCell {
            x: x.to_owned(),
            y: y.to_owned(),
            value,
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

    fn data(
        x_field: &str,
        y_field: &str,
        value_field: Option<&str>,
        aggregate: Aggregate,
        bucket: Option<Bucket>,
        x_labels: Vec<&str>,
        y_labels: Vec<&str>,
        cells: Vec<HeatmapCell>,
        unplaced: Vec<UnplacedCard>,
    ) -> HeatmapData {
        HeatmapData {
            x_field: x_field.to_owned(),
            y_field: y_field.to_owned(),
            value_field: value_field.map(str::to_owned),
            aggregate,
            bucket,
            x_labels: x_labels.into_iter().map(str::to_owned).collect(),
            y_labels: y_labels.into_iter().map(str::to_owned).collect(),
            cells,
            unplaced,
        }
    }

    /// Plotters serializes colors as either `#RRGGBB` (uppercase) or
    /// `rgb(r,g,b)` depending on the surrounding attribute. The two
    /// renderers (bar/line) match on either form; we follow the same
    /// pattern.
    fn contains_color(output: &str, hex: &str) -> bool {
        let bytes = hex.strip_prefix('#').unwrap();
        let r = u8::from_str_radix(&bytes[0..2], 16).unwrap();
        let g = u8::from_str_radix(&bytes[2..4], 16).unwrap();
        let b = u8::from_str_radix(&bytes[4..6], 16).unwrap();
        let rgb = format!("rgb({r},{g},{b})");
        let upper = format!("#{}", bytes.to_uppercase());
        let lower = format!("#{}", bytes.to_lowercase());
        output.contains(&rgb) || output.contains(&upper) || output.contains(&lower)
    }

    // ── Heading / empty / description ───────────────────────────────

    #[test]
    fn heading_count_form() {
        let output = render_heatmap(
            &data(
                "status",
                "team",
                None,
                Aggregate::Count,
                None,
                vec![],
                vec![],
                vec![],
                vec![],
            ),
            "../workdown-items",
            "",
        );
        assert!(output.starts_with("# Heatmap: count by status × team\n"));
    }

    #[test]
    fn heading_aggregate_of_value_form() {
        let output = render_heatmap(
            &data(
                "status",
                "team",
                Some("points"),
                Aggregate::Sum,
                None,
                vec![],
                vec![],
                vec![],
                vec![],
            ),
            "../workdown-items",
            "",
        );
        assert!(output.starts_with("# Heatmap: sum of points by status × team\n"));
    }

    #[test]
    fn empty_view_emits_no_items_marker_and_no_svg() {
        let output = render_heatmap(
            &data(
                "status",
                "team",
                None,
                Aggregate::Count,
                None,
                vec![],
                vec![],
                vec![],
                vec![],
            ),
            "../workdown-items",
            "",
        );
        assert!(output.contains("_(no items)_"));
        assert!(!output.contains("<svg"));
    }

    #[test]
    fn description_emitted_under_heading() {
        let output = render_heatmap(
            &data(
                "status",
                "team",
                None,
                Aggregate::Count,
                None,
                vec![],
                vec![],
                vec![],
                vec![],
            ),
            "../workdown-items",
            "Items per status × team.",
        );
        assert!(
            output.contains("# Heatmap: count by status × team\n\nItems per status × team.\n\n")
        );
    }

    // ── SVG color schemes ───────────────────────────────────────────

    #[test]
    fn sequential_scheme_renders_positive_hue_for_max_cell() {
        let cells = vec![
            cell("open", "eng", AggregateValue::Number(1.0)),
            cell("done", "eng", AggregateValue::Number(5.0)),
        ];
        let output = render_heatmap(
            &data(
                "status",
                "team",
                None,
                Aggregate::Count,
                None,
                vec!["done", "open"],
                vec!["eng"],
                cells,
                vec![],
            ),
            "../workdown-items",
            "",
        );
        assert!(output.contains("<svg"));
        assert!(
            contains_color(&output, POSITIVE_HEX),
            "expected sequential hue {POSITIVE_HEX} in: {output}"
        );
    }

    #[test]
    fn diverging_scheme_used_when_any_value_is_negative() {
        let cells = vec![
            cell("alpha", "row", AggregateValue::Number(-3.0)),
            cell("beta", "row", AggregateValue::Number(3.0)),
        ];
        let output = render_heatmap(
            &data(
                "status",
                "team",
                Some("delta"),
                Aggregate::Sum,
                None,
                vec!["alpha", "beta"],
                vec!["row"],
                cells,
                vec![],
            ),
            "../workdown-items",
            "",
        );
        assert!(
            contains_color(&output, NEGATIVE_HEX),
            "expected negative hue {NEGATIVE_HEX} in: {output}"
        );
        assert!(
            contains_color(&output, POSITIVE_HEX),
            "expected positive hue {POSITIVE_HEX} in: {output}"
        );
    }

    #[test]
    fn missing_cell_drawn_in_empty_color() {
        // 2x2 grid, only one cell populated → three cells must use the
        // empty-cell color so the grid stays solid.
        let cells = vec![cell("open", "eng", AggregateValue::Number(1.0))];
        let output = render_heatmap(
            &data(
                "status",
                "team",
                None,
                Aggregate::Count,
                None,
                vec!["done", "open"],
                vec!["eng", "ops"],
                cells,
                vec![],
            ),
            "../workdown-items",
            "",
        );
        assert!(
            contains_color(&output, EMPTY_CELL_HEX),
            "expected empty-cell color {EMPTY_CELL_HEX} in: {output}"
        );
    }

    #[test]
    fn all_same_value_renders_without_panic() {
        let cells = vec![
            cell("a", "x", AggregateValue::Number(7.0)),
            cell("b", "x", AggregateValue::Number(7.0)),
        ];
        let output = render_heatmap(
            &data(
                "g",
                "y",
                None,
                Aggregate::Count,
                None,
                vec!["a", "b"],
                vec!["x"],
                cells,
                vec![],
            ),
            "../workdown-items",
            "",
        );
        assert!(output.contains("<svg"));
    }

    // ── Pivoted values table ────────────────────────────────────────

    #[test]
    fn values_table_has_pivoted_header_with_corner_label() {
        let cells = vec![
            cell("done", "eng", AggregateValue::Number(2.0)),
            cell("open", "ops", AggregateValue::Number(5.0)),
        ];
        let output = render_heatmap(
            &data(
                "status",
                "team",
                None,
                Aggregate::Count,
                None,
                vec!["done", "open"],
                vec!["eng", "ops"],
                cells,
                vec![],
            ),
            "../workdown-items",
            "",
        );
        assert!(output.contains("## Values\n"));
        assert!(output.contains("| team / status | done | open |"));
    }

    #[test]
    fn values_table_blank_for_missing_cells() {
        // (open, eng) populated; (done, eng), (done, ops), (open, ops) blank.
        let cells = vec![cell("open", "eng", AggregateValue::Number(3.0))];
        let output = render_heatmap(
            &data(
                "status",
                "team",
                None,
                Aggregate::Count,
                None,
                vec!["done", "open"],
                vec!["eng", "ops"],
                cells,
                vec![],
            ),
            "../workdown-items",
            "",
        );
        // eng row: blank for "done", "3" for "open".
        assert!(
            output.contains("| eng |  | 3 |"),
            "expected blank then 3 for eng row, got: {output}"
        );
        // ops row: both cells blank.
        assert!(
            output.contains("| ops |  |  |"),
            "expected fully blank ops row, got: {output}"
        );
    }

    #[test]
    fn values_table_formats_dates_as_iso() {
        use chrono::NaiveDate;
        let cells = vec![cell(
            "open",
            "eng",
            AggregateValue::Date(NaiveDate::from_ymd_opt(2026, 5, 15).unwrap()),
        )];
        let output = render_heatmap(
            &data(
                "status",
                "team",
                Some("deadline"),
                Aggregate::Avg,
                None,
                vec!["open"],
                vec!["eng"],
                cells,
                vec![],
            ),
            "../workdown-items",
            "",
        );
        assert!(output.contains("| eng | 2026-05-15 |"));
    }

    #[test]
    fn values_table_formats_durations_as_shorthand() {
        use crate::render::chart_common::{SECONDS_PER_DAY, SECONDS_PER_HOUR};
        let cells = vec![cell(
            "open",
            "eng",
            AggregateValue::Duration(SECONDS_PER_DAY + SECONDS_PER_HOUR),
        )];
        let output = render_heatmap(
            &data(
                "status",
                "team",
                Some("estimate"),
                Aggregate::Sum,
                None,
                vec!["open"],
                vec!["eng"],
                cells,
                vec![],
            ),
            "../workdown-items",
            "",
        );
        assert!(output.contains("| eng | 1d 1h |"));
    }

    #[test]
    fn values_table_escapes_pipe_in_axis_label() {
        let cells = vec![cell("a | b", "row", AggregateValue::Number(1.0))];
        let output = render_heatmap(
            &data(
                "status",
                "team",
                None,
                Aggregate::Count,
                None,
                vec!["a | b"],
                vec!["row"],
                cells,
                vec![],
            ),
            "../workdown-items",
            "",
        );
        assert!(output.contains(r"a \| b"));
    }

    // ── Acceptance: categorical × date-bucketed-by-week ─────────────

    #[test]
    fn categorical_x_with_week_bucketed_y_renders_iso_week_labels() {
        // Mirror of the issue's acceptance setup: one categorical axis
        // ("team") and one date axis bucketed by week ("week"). The
        // extractor would produce ISO-week strings like "2026-W02"; we
        // pass them through directly since the renderer is axis-type-
        // agnostic and the extractor is covered by its own tests.
        let cells = vec![
            cell("eng", "2026-W02", AggregateValue::Number(2.0)),
            cell("ops", "2026-W03", AggregateValue::Number(1.0)),
        ];
        let output = render_heatmap(
            &data(
                "team",
                "week",
                None,
                Aggregate::Count,
                Some(Bucket::Week),
                vec!["eng", "ops"],
                vec!["2026-W02", "2026-W03"],
                cells,
                vec![],
            ),
            "../workdown-items",
            "",
        );
        assert!(output.contains("# Heatmap: count by team × week\n"));
        // ISO-week labels appear in the SVG axis text…
        assert!(
            output.contains("2026-W02"),
            "expected ISO-week label in output, got: {output}"
        );
        assert!(output.contains("2026-W03"));
        // …and as row labels in the pivoted values table.
        assert!(output.contains("| 2026-W02 |"));
        assert!(output.contains("| 2026-W03 |"));
    }

    // ── Unplaced footer ─────────────────────────────────────────────

    #[test]
    fn unplaced_footer_lists_missing_field_per_item() {
        let cells = vec![cell("open", "eng", AggregateValue::Number(1.0))];
        let output = render_heatmap(
            &data(
                "status",
                "team",
                None,
                Aggregate::Count,
                None,
                vec!["open"],
                vec!["eng"],
                cells,
                vec![
                    unplaced("missing-status", Some("Missing"), "status"),
                    unplaced("missing-other", None, "team"),
                ],
            ),
            "../workdown-items",
            "",
        );
        assert!(output.contains("## Unplaced\n"));
        assert!(
            output.contains("[Missing](../workdown-items/missing-status.md) — missing `status`")
        );
        assert!(
            output.contains("[missing-other](../workdown-items/missing-other.md) — missing `team`")
        );
    }

    #[test]
    fn no_unplaced_section_when_clean() {
        let cells = vec![cell("open", "eng", AggregateValue::Number(1.0))];
        let output = render_heatmap(
            &data(
                "status",
                "team",
                None,
                Aggregate::Count,
                None,
                vec!["open"],
                vec!["eng"],
                cells,
                vec![],
            ),
            "../workdown-items",
            "",
        );
        assert!(!output.contains("Unplaced"));
    }

    #[test]
    fn only_unplaced_emits_footer_without_svg_or_table() {
        let output = render_heatmap(
            &data(
                "status",
                "team",
                None,
                Aggregate::Count,
                None,
                vec![],
                vec![],
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
