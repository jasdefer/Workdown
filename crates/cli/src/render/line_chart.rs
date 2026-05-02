//! Line chart renderer — turns [`LineChartData`] into a Markdown document
//! with an inline SVG produced by `plotters`.
//!
//! Output shape: `# Line chart: <y> over <x>` heading, optional description,
//! a single `<svg>` block (or `_(no items)_` when there are no points), and
//! a `## Unplaced` footer when the extractor dropped any items.
//!
//! Multi-series: when the view has `group:` set, points partition into one
//! series per distinct group value. Items missing the group value land in
//! a synthetic `(no <field>)` series. Series colors come from the
//! Okabe-Ito palette (color-blind-safe), assigned in series-sort order
//! and recycled past 8 groups for determinism.
//!
//! Axis units: x and y are formatted per their underlying [`AxisValue`] /
//! [`SizeValue`] variant. Numeric values use `format_number`; date values
//! use `YYYY-MM-DD`; duration values pick the largest fitting unit
//! (`w`/`d`/`h`/`min`/`s`) so axis ticks render as plain numbers and the
//! axis label names the unit (e.g. `estimate (hours)`). Mixed axes
//! shouldn't happen in practice — every point on one axis comes from the
//! same schema field — and the renderer panics if it sees one.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use plotters::prelude::*;

use workdown_core::view_data::{AxisValue, LineChartData, LinePoint, SizeValue, UnplacedReason};

use crate::render::chart_common::{
    axis_label, date_to_f64, format_axis_tick, hex_to_rgb, numeric_extent, pad_extent,
    pick_duration_unit, AxisKind, OKABE_ITO,
};
use crate::render::common::{card_link, emit_description};

const SVG_WIDTH: u32 = 800;
const SVG_HEIGHT: u32 = 400;

/// Render a `LineChartData` as a Markdown string.
///
/// `item_link_base` is the relative path from the rendered view file to
/// the work items directory — same parameter as `render_treemap`.
/// `description` is the one-line caption emitted below the heading.
pub fn render_line_chart(data: &LineChartData, item_link_base: &str, description: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "# Line chart: {y} over {x}",
        y = data.y_field,
        x = data.x_field,
    );
    out.push('\n');
    emit_description(description, &mut out);

    if data.points.is_empty() && data.unplaced.is_empty() {
        out.push_str("_(no items)_\n");
        return out;
    }

    if !data.points.is_empty() {
        let svg = render_svg(data);
        out.push_str(&svg);
        if !out.ends_with('\n') {
            out.push('\n');
        }
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

// ── SVG rendering ───────────────────────────────────────────────────

/// One drawable series: a sorted list of (x, y) numeric points, the
/// label shown in the legend, and the assigned palette color.
struct Series {
    label: String,
    color: RGBColor,
    points: Vec<(f64, f64)>,
}

fn render_svg(data: &LineChartData) -> String {
    let x_kind = axis_kind_x(&data.points);
    let y_kind = axis_kind_y(&data.points);

    let series = build_series(&data.points, data.group_field.as_deref(), x_kind, y_kind);

    let (x_min, x_max) = numeric_extent(series.iter().flat_map(|s| s.points.iter().map(|p| p.0)));
    let (y_min, y_max) = numeric_extent(series.iter().flat_map(|s| s.points.iter().map(|p| p.1)));
    let (x_min, x_max) = pad_extent(x_min, x_max);
    let (y_min, y_max) = pad_extent(y_min, y_max);

    let multi_series = data.group_field.is_some();
    let x_axis_label = axis_label(&data.x_field, x_kind);
    let y_axis_label = axis_label(&data.y_field, y_kind);

    let mut buf = String::new();
    {
        let root = SVGBackend::with_string(&mut buf, (SVG_WIDTH, SVG_HEIGHT)).into_drawing_area();
        root.fill(&WHITE).expect("fill white background");

        let mut chart = ChartBuilder::on(&root)
            .margin(20)
            .x_label_area_size(50)
            .y_label_area_size(70)
            .build_cartesian_2d(x_min..x_max, y_min..y_max)
            .expect("build cartesian 2d");

        chart
            .configure_mesh()
            .x_desc(x_axis_label)
            .y_desc(y_axis_label)
            .x_label_formatter(&|value: &f64| format_axis_tick(*value, x_kind))
            .y_label_formatter(&|value: &f64| format_axis_tick(*value, y_kind))
            .draw()
            .expect("draw mesh");

        for s in &series {
            let color = s.color;
            let line_color = color.stroke_width(2);
            let series_points = s.points.clone();
            let label = s.label.clone();
            chart
                .draw_series(LineSeries::new(series_points.clone(), line_color))
                .expect("draw line series")
                .label(label)
                .legend(move |(x, y)| {
                    PathElement::new(vec![(x, y), (x + 16, y)], color.stroke_width(2))
                });
            chart
                .draw_series(
                    series_points
                        .iter()
                        .map(|point| Circle::new(*point, 3, color.filled())),
                )
                .expect("draw point series");
        }

        if multi_series {
            chart
                .configure_series_labels()
                .background_style(WHITE.mix(0.85))
                .border_style(BLACK)
                .draw()
                .expect("draw legend");
        }

        root.present().expect("present svg");
    }
    buf
}

/// Pick the f64 axis encoding from the first point's variant. Every
/// other point on the same axis must agree — extractor invariant since
/// each axis is bound to one schema field — and we panic on mismatch
/// so a future regression doesn't silently mis-render.
fn axis_kind_x(points: &[LinePoint]) -> AxisKind {
    match points
        .first()
        .map(|p| p.x)
        .expect("axis_kind_x called with empty points")
    {
        AxisValue::Number(_) => AxisKind::Number,
        AxisValue::Date(_) => AxisKind::Date,
        AxisValue::Duration(_) => {
            let max = points
                .iter()
                .filter_map(|p| match p.x {
                    AxisValue::Duration(seconds) => Some(seconds.unsigned_abs() as i64),
                    _ => None,
                })
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

fn axis_kind_y(points: &[LinePoint]) -> AxisKind {
    match points
        .first()
        .map(|p| p.y)
        .expect("axis_kind_y called with empty points")
    {
        SizeValue::Number(_) => AxisKind::Number,
        SizeValue::Duration(_) => {
            let max = points
                .iter()
                .filter_map(|p| match p.y {
                    SizeValue::Duration(seconds) => Some(seconds.unsigned_abs() as i64),
                    _ => None,
                })
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

/// Convert an `AxisValue` to its plot-space f64 using the chosen axis kind.
fn axis_to_f64(value: AxisValue, kind: AxisKind) -> f64 {
    match (value, kind) {
        (AxisValue::Number(n), AxisKind::Number) => n,
        (AxisValue::Date(date), AxisKind::Date) => date_to_f64(date),
        (AxisValue::Duration(seconds), AxisKind::Duration { divisor, .. }) => {
            seconds as f64 / divisor as f64
        }
        (value, kind) => panic!("mixed axis types on x: value {value:?} with kind {kind:?}"),
    }
}

/// Convert a `SizeValue` to its plot-space f64 using the chosen axis kind.
fn size_to_f64(value: SizeValue, kind: AxisKind) -> f64 {
    match (value, kind) {
        (SizeValue::Number(n), AxisKind::Number) => n,
        (SizeValue::Duration(seconds), AxisKind::Duration { divisor, .. }) => {
            seconds as f64 / divisor as f64
        }
        (value, kind) => panic!("mixed axis types on y: value {value:?} with kind {kind:?}"),
    }
}

/// Group points into series, assign palette colors, and convert values
/// to f64 plot coordinates.
///
/// Single-series case (no group field): one series labelled empty. The
/// renderer skips the legend in that case so an empty label doesn't show.
///
/// Multi-series case: one series per distinct group, plus a synthetic
/// `(no <field>)` series for points whose group value is missing. Series
/// sort by group label ascending; the synthetic series sorts last
/// regardless. Color is `OKABE_ITO[i % 8]` over that sort order — order-
/// stable so the same view always picks the same colors.
fn build_series(
    points: &[LinePoint],
    group_field: Option<&str>,
    x_kind: AxisKind,
    y_kind: AxisKind,
) -> Vec<Series> {
    if group_field.is_none() {
        let series_points: Vec<(f64, f64)> = points
            .iter()
            .map(|p| (axis_to_f64(p.x, x_kind), size_to_f64(p.y, y_kind)))
            .collect();
        return vec![Series {
            label: String::new(),
            color: hex_to_rgb(OKABE_ITO[0]),
            points: series_points,
        }];
    }

    let group_field = group_field.unwrap();
    let synthetic = format!("(no {group_field})");

    let mut grouped: BTreeMap<String, Vec<(f64, f64)>> = BTreeMap::new();
    let mut synthetic_points: Vec<(f64, f64)> = Vec::new();

    for point in points {
        let xy = (axis_to_f64(point.x, x_kind), size_to_f64(point.y, y_kind));
        match &point.group {
            Some(label) => grouped.entry(label.clone()).or_default().push(xy),
            None => synthetic_points.push(xy),
        }
    }

    let mut series: Vec<Series> = Vec::with_capacity(grouped.len() + 1);
    let mut color_index = 0usize;
    for (label, pts) in grouped {
        series.push(Series {
            label,
            color: hex_to_rgb(OKABE_ITO[color_index % OKABE_ITO.len()]),
            points: pts,
        });
        color_index += 1;
    }
    if !synthetic_points.is_empty() {
        series.push(Series {
            label: synthetic,
            color: hex_to_rgb(OKABE_ITO[color_index % OKABE_ITO.len()]),
            points: synthetic_points,
        });
    }
    series
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use crate::render::chart_common::{SECONDS_PER_DAY, SECONDS_PER_HOUR};
    use chrono::NaiveDate;
    use workdown_core::model::WorkItemId;
    use workdown_core::view_data::{Card, LineChartData, LinePoint, UnplacedCard, UnplacedReason};

    // ── Render fixtures ─────────────────────────────────────────────

    fn card(id: &str, title: Option<&str>) -> Card {
        Card {
            id: WorkItemId::from(id.to_owned()),
            title: title.map(str::to_owned),
            fields: vec![],
            body: String::new(),
        }
    }

    fn point(id: &str, x: AxisValue, y: SizeValue, group: Option<&str>) -> LinePoint {
        LinePoint {
            id: WorkItemId::from(id.to_owned()),
            x,
            y,
            group: group.map(str::to_owned),
        }
    }

    fn data(
        x_field: &str,
        y_field: &str,
        group_field: Option<&str>,
        points: Vec<LinePoint>,
        unplaced: Vec<UnplacedCard>,
    ) -> LineChartData {
        LineChartData {
            x_field: x_field.to_owned(),
            y_field: y_field.to_owned(),
            group_field: group_field.map(str::to_owned),
            points,
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
    fn heading_uses_y_over_x() {
        let output = render_line_chart(
            &data("estimate", "actual", None, vec![], vec![]),
            "../workdown-items",
            "",
        );
        assert!(output.starts_with("# Line chart: actual over estimate\n"));
    }

    #[test]
    fn empty_view_emits_no_items_marker_and_no_svg() {
        let output = render_line_chart(
            &data("estimate", "actual", None, vec![], vec![]),
            "../workdown-items",
            "",
        );
        assert!(output.contains("_(no items)_"));
        assert!(!output.contains("<svg"));
    }

    #[test]
    fn description_emitted_under_heading() {
        let output = render_line_chart(
            &data("estimate", "actual", None, vec![], vec![]),
            "../workdown-items",
            "Estimate vs actual effort.",
        );
        assert!(
            output.contains("# Line chart: actual over estimate\n\nEstimate vs actual effort.\n\n")
        );
    }

    // ── Single series ───────────────────────────────────────────────

    #[test]
    fn single_series_emits_svg_with_first_palette_color() {
        let points = vec![
            point("a", AxisValue::Number(1.0), SizeValue::Number(2.0), None),
            point("b", AxisValue::Number(2.0), SizeValue::Number(4.0), None),
            point("c", AxisValue::Number(3.0), SizeValue::Number(6.0), None),
        ];
        let output = render_line_chart(
            &data("x", "y", None, points, vec![]),
            "../workdown-items",
            "",
        );
        assert!(output.contains("<svg"));
        // First palette color drives the single series.
        assert!(
            output.contains("stroke=\"#E69F00\""),
            "expected first palette color in stroke, got: {output}"
        );
    }

    #[test]
    fn single_series_skips_legend() {
        let points = vec![
            point("a", AxisValue::Number(1.0), SizeValue::Number(2.0), None),
            point("b", AxisValue::Number(2.0), SizeValue::Number(4.0), None),
        ];
        let output = render_line_chart(
            &data("x", "y", None, points, vec![]),
            "../workdown-items",
            "",
        );
        // configure_series_labels is only called in multi-series mode;
        // its background opacity attribute is the marker we look for.
        assert!(
            !output.contains("opacity=\"0.85\""),
            "single series shouldn't draw a legend background"
        );
    }

    // ── Multi-series ────────────────────────────────────────────────

    #[test]
    fn multi_series_uses_distinct_palette_colors_per_group() {
        let points = vec![
            point(
                "a",
                AxisValue::Number(1.0),
                SizeValue::Number(2.0),
                Some("eng"),
            ),
            point(
                "b",
                AxisValue::Number(2.0),
                SizeValue::Number(4.0),
                Some("ops"),
            ),
            point(
                "c",
                AxisValue::Number(3.0),
                SizeValue::Number(6.0),
                Some("eng"),
            ),
        ];
        let output = render_line_chart(
            &data("x", "y", Some("team"), points, vec![]),
            "../workdown-items",
            "",
        );
        // BTreeMap orders eng before ops → eng gets OKABE_ITO[0], ops [1].
        assert!(
            output.contains("stroke=\"#E69F00\""),
            "expected first palette color (eng), got: {output}"
        );
        assert!(
            output.contains("stroke=\"#56B4E9\""),
            "expected second palette color (ops), got: {output}"
        );
    }

    #[test]
    fn multi_series_includes_group_labels() {
        let points = vec![
            point(
                "a",
                AxisValue::Number(1.0),
                SizeValue::Number(2.0),
                Some("eng"),
            ),
            point(
                "b",
                AxisValue::Number(2.0),
                SizeValue::Number(4.0),
                Some("ops"),
            ),
        ];
        let output = render_line_chart(
            &data("x", "y", Some("team"), points, vec![]),
            "../workdown-items",
            "",
        );
        assert!(output.contains("eng"), "expected legend label 'eng'");
        assert!(output.contains("ops"), "expected legend label 'ops'");
    }

    #[test]
    fn missing_group_value_lands_in_synthetic_series() {
        let points = vec![
            point(
                "a",
                AxisValue::Number(1.0),
                SizeValue::Number(2.0),
                Some("eng"),
            ),
            point("b", AxisValue::Number(2.0), SizeValue::Number(4.0), None),
        ];
        let output = render_line_chart(
            &data("x", "y", Some("team"), points, vec![]),
            "../workdown-items",
            "",
        );
        // Synthetic series labelled "(no team)".
        assert!(
            output.contains("(no team)"),
            "expected '(no team)' synthetic series"
        );
    }

    #[test]
    fn nine_groups_recycle_first_color() {
        // 9 groups → group #9 gets OKABE_ITO[0] again.
        let groups = ["a", "b", "c", "d", "e", "f", "g", "h", "i"];
        let mut points = Vec::new();
        for (i, g) in groups.iter().enumerate() {
            points.push(point(
                g,
                AxisValue::Number(i as f64),
                SizeValue::Number((i * 2) as f64),
                Some(g),
            ));
        }
        let output = render_line_chart(
            &data("x", "y", Some("team"), points, vec![]),
            "../workdown-items",
            "",
        );
        // First color (#E69F00 → rgb(230, 159, 0)) should appear at least
        // twice — once for "a", once for "i" after recycling.
        let needle = "#E69F00";
        let count_lower = output.matches(needle).count();
        let count_upper = output.matches("rgb(230,159,0)").count();
        assert!(
            count_lower + count_upper >= 2,
            "expected first-color reuse, got lower={count_lower} upper={count_upper}",
        );
    }

    // ── Axis variants ───────────────────────────────────────────────

    #[test]
    fn date_x_axis_renders_iso_tick_label() {
        let points = vec![
            point(
                "a",
                AxisValue::Date(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()),
                SizeValue::Number(1.0),
                None,
            ),
            point(
                "b",
                AxisValue::Date(NaiveDate::from_ymd_opt(2026, 1, 10).unwrap()),
                SizeValue::Number(2.0),
                None,
            ),
        ];
        let output = render_line_chart(
            &data("day", "score", None, points, vec![]),
            "../workdown-items",
            "",
        );
        // Tick labels span the date range — at least one ISO date present.
        assert!(
            output.contains("2026-01"),
            "expected 2026-01-* tick label in {output}"
        );
    }

    #[test]
    fn duration_y_axis_label_includes_unit() {
        let points = vec![
            point(
                "a",
                AxisValue::Number(1.0),
                SizeValue::Duration(2 * SECONDS_PER_DAY),
                None,
            ),
            point(
                "b",
                AxisValue::Number(2.0),
                SizeValue::Duration(4 * SECONDS_PER_DAY),
                None,
            ),
        ];
        let output = render_line_chart(
            &data("x", "estimate", None, points, vec![]),
            "../workdown-items",
            "",
        );
        // Axis description embeds "estimate (days)" — appears in <text>.
        assert!(
            output.contains("estimate (days)"),
            "expected y-axis label 'estimate (days)' in {output}"
        );
    }

    #[test]
    fn duration_x_axis_label_includes_unit() {
        let points = vec![
            point(
                "a",
                AxisValue::Duration(2 * SECONDS_PER_HOUR),
                SizeValue::Number(1.0),
                None,
            ),
            point(
                "b",
                AxisValue::Duration(4 * SECONDS_PER_HOUR),
                SizeValue::Number(2.0),
                None,
            ),
        ];
        let output = render_line_chart(
            &data("estimate", "y", None, points, vec![]),
            "../workdown-items",
            "",
        );
        assert!(
            output.contains("estimate (hours)"),
            "expected x-axis label 'estimate (hours)' in {output}"
        );
    }

    // ── Unplaced footer ─────────────────────────────────────────────

    #[test]
    fn unplaced_footer_lists_missing_field_per_item() {
        let points = vec![point(
            "a",
            AxisValue::Number(1.0),
            SizeValue::Number(2.0),
            None,
        )];
        let output = render_line_chart(
            &data(
                "x",
                "y",
                None,
                points,
                vec![
                    unplaced("missing-x", Some("Missing X"), "x"),
                    unplaced("missing-y", Some("Missing Y"), "y"),
                ],
            ),
            "../workdown-items",
            "",
        );
        assert!(output.contains("## Unplaced\n"));
        assert!(output.contains("[Missing X](../workdown-items/missing-x.md) — missing `x`"));
        assert!(output.contains("[Missing Y](../workdown-items/missing-y.md) — missing `y`"));
    }

    #[test]
    fn no_unplaced_section_when_clean() {
        let points = vec![
            point("a", AxisValue::Number(1.0), SizeValue::Number(2.0), None),
            point("b", AxisValue::Number(2.0), SizeValue::Number(4.0), None),
        ];
        let output = render_line_chart(
            &data("x", "y", None, points, vec![]),
            "../workdown-items",
            "",
        );
        assert!(!output.contains("Unplaced"));
    }

    #[test]
    fn only_unplaced_emits_footer_without_svg() {
        let output = render_line_chart(
            &data(
                "x",
                "y",
                None,
                vec![],
                vec![unplaced("orphan", Some("Orphan"), "x")],
            ),
            "../workdown-items",
            "",
        );
        assert!(!output.contains("<svg"));
        assert!(output.contains("## Unplaced\n"));
        assert!(output.contains("[Orphan](../workdown-items/orphan.md) — missing `x`"));
    }
}
