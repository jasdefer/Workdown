//! Line chart renderer — turns [`LineChartData`] into a Markdown document
//! with an inline SVG produced by `plotters`.
//!
//! Output shape: `# Line chart: <y> over <x>` heading, optional description,
//! a single `<svg>` block (or `_(no items)_` when there are no points), and
//! a `## Unplaced` footer when the extractor dropped any items.
//!
//! Multi-series: the extractor partitions points into series and fixes
//! their order; this renderer draws them as received. It supplies only
//! presentation — the no-value series is named by `no_value_label`, and
//! colors come from the Okabe-Ito palette (color-blind-safe), assigned
//! in received order and recycled past 8 groups for determinism.
//!
//! Axis units: x and y are formatted per their underlying [`ChartValue`]
//! variant (the y-side [`workdown_core::view_data::SizeValue`] converts
//! into it). Numeric values use `format_number`; date values
//! use `YYYY-MM-DD`; duration values pick the largest fitting unit
//! (`w`/`d`/`h`/`min`/`s`) so axis ticks render as plain numbers and the
//! axis label names the unit (e.g. `estimate (hours)`). Mixed axes
//! shouldn't happen in practice — every point on one axis comes from the
//! same schema field — and the renderer panics if it sees one.

use plotters::prelude::*;

use workdown_core::view_data::{ChartValue, LineChartData, LinePoint};

use crate::render::markdown::{emit_description, emit_unplaced_section, no_value_label};
use crate::render::svg_chart::{
    axis_kind_for, axis_label, format_axis_tick, hex_to_rgb, numeric_extent, pad_extent,
    strip_svg_blank_lines, value_to_f64, AxisKind, OKABE_ITO,
};

const SVG_WIDTH: u32 = 800;
const SVG_HEIGHT: u32 = 400;

/// Render a `LineChartData` as a Markdown string.
///
/// `item_link_base` is the relative path from the rendered view file to
/// the work items directory — same parameter as `render_treemap`.
/// `description` is the one-line caption emitted below the heading.
pub fn render_line_chart(data: &LineChartData, item_link_base: &str, description: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# Line chart: {y} over {x}\n\n",
        y = data.y_field,
        x = data.x_field,
    ));
    emit_description(description, &mut out);

    if data.series.is_empty() && data.unplaced.is_empty() {
        out.push_str("_(no items)_\n");
        return out;
    }

    if !data.series.is_empty() {
        let svg = render_svg(data);
        out.push_str(&svg);
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
    }

    emit_unplaced_section(&data.unplaced, item_link_base, &mut out);

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
    // Axis units are chosen across every point, so flatten the series
    // back out for that decision only.
    let points: Vec<&LinePoint> = data
        .series
        .iter()
        .flat_map(|series| series.points.iter())
        .collect();
    let x_kind = axis_kind_for(points.iter().map(|point| point.x));
    let y_kind = axis_kind_for(points.iter().map(|point| ChartValue::from(point.y)));

    let series = build_series(data, x_kind, y_kind);

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
    strip_svg_blank_lines(&buf)
}

/// Turn the extractor's series into drawable ones: label text, a palette
/// color, and f64 plot coordinates.
///
/// The partition and its order are the extractor's (see
/// `view_data::line_chart`); this only dresses them. Color is
/// `OKABE_ITO[i % 8]` over the received order, so the same view always
/// picks the same colors, and recycles past 8 groups.
///
/// Labels: a grouped series shows its value; the no-value series is
/// named by `no_value_label`. An ungrouped chart's single series gets an
/// empty label, and `render_svg` skips the legend so it doesn't show.
fn build_series(data: &LineChartData, x_kind: AxisKind, y_kind: AxisKind) -> Vec<Series> {
    data.series
        .iter()
        .enumerate()
        .map(|(index, series)| Series {
            label: match (&series.group, &data.group_field) {
                (Some(value), _) => value.clone(),
                (None, Some(field)) => no_value_label(field),
                (None, None) => String::new(),
            },
            color: hex_to_rgb(OKABE_ITO[index % OKABE_ITO.len()]),
            points: series
                .points
                .iter()
                .map(|point| {
                    (
                        value_to_f64(point.x, x_kind),
                        value_to_f64(ChartValue::from(point.y), y_kind),
                    )
                })
                .collect(),
        })
        .collect()
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::test_fixtures::unplaced_missing;
    use super::*;

    use crate::render::svg_chart::{SECONDS_PER_DAY, SECONDS_PER_HOUR};
    use chrono::NaiveDate;
    use std::collections::HashMap;
    use workdown_core::model::WorkItemId;
    use workdown_core::view_data::{LineChartData, LinePoint, LineSeries, SizeValue, UnplacedCard};

    // ── Render fixtures ─────────────────────────────────────────────

    fn point(id: &str, x: ChartValue, y: SizeValue) -> LinePoint {
        LinePoint {
            id: WorkItemId::from(id.to_owned()),
            x,
            y,
        }
    }

    fn series(group: Option<&str>, points: Vec<LinePoint>) -> LineSeries {
        LineSeries {
            group: group.map(str::to_owned),
            points,
        }
    }

    /// The one no-group series an ungrouped chart receives.
    fn single(points: Vec<LinePoint>) -> Vec<LineSeries> {
        vec![series(None, points)]
    }

    fn data(
        x_field: &str,
        y_field: &str,
        group_field: Option<&str>,
        series: Vec<LineSeries>,
        unplaced: Vec<UnplacedCard>,
    ) -> LineChartData {
        LineChartData {
            x_field: x_field.to_owned(),
            y_field: y_field.to_owned(),
            group_field: group_field.map(str::to_owned),
            series,
            items: HashMap::new(),
            unplaced,
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
            point("a", ChartValue::Number(1.0), SizeValue::Number(2.0)),
            point("b", ChartValue::Number(2.0), SizeValue::Number(4.0)),
            point("c", ChartValue::Number(3.0), SizeValue::Number(6.0)),
        ];
        let output = render_line_chart(
            &data("x", "y", None, single(points), vec![]),
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
            point("a", ChartValue::Number(1.0), SizeValue::Number(2.0)),
            point("b", ChartValue::Number(2.0), SizeValue::Number(4.0)),
        ];
        let output = render_line_chart(
            &data("x", "y", None, single(points), vec![]),
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
    //
    // The extractor decides the partition and its order; these fixtures
    // hand the renderer series already in draw order and check only what
    // the renderer adds — color, label, legend.

    #[test]
    fn palette_walks_the_received_series_order() {
        let output = render_line_chart(
            &data(
                "x",
                "y",
                Some("team"),
                vec![
                    series(
                        Some("eng"),
                        vec![
                            point("a", ChartValue::Number(1.0), SizeValue::Number(2.0)),
                            point("c", ChartValue::Number(3.0), SizeValue::Number(6.0)),
                        ],
                    ),
                    series(
                        Some("ops"),
                        vec![point("b", ChartValue::Number(2.0), SizeValue::Number(4.0))],
                    ),
                ],
                vec![],
            ),
            "../workdown-items",
            "",
        );
        // First series received gets OKABE_ITO[0], second [1].
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
        let output = render_line_chart(
            &data(
                "x",
                "y",
                Some("team"),
                vec![
                    series(
                        Some("eng"),
                        vec![point("a", ChartValue::Number(1.0), SizeValue::Number(2.0))],
                    ),
                    series(
                        Some("ops"),
                        vec![point("b", ChartValue::Number(2.0), SizeValue::Number(4.0))],
                    ),
                ],
                vec![],
            ),
            "../workdown-items",
            "",
        );
        assert!(output.contains("eng"), "expected legend label 'eng'");
        assert!(output.contains("ops"), "expected legend label 'ops'");
    }

    #[test]
    fn no_group_series_is_named_from_the_group_field() {
        let output = render_line_chart(
            &data(
                "x",
                "y",
                Some("team"),
                vec![
                    series(
                        Some("eng"),
                        vec![point("a", ChartValue::Number(1.0), SizeValue::Number(2.0))],
                    ),
                    series(
                        None,
                        vec![point("b", ChartValue::Number(2.0), SizeValue::Number(4.0))],
                    ),
                ],
                vec![],
            ),
            "../workdown-items",
            "",
        );
        // Core ships a structural `None`; the wording is this renderer's,
        // via `no_value_label`.
        assert!(
            output.contains("(no team)"),
            "expected '(no team)' label for the no-group series"
        );
    }

    #[test]
    fn nine_groups_recycle_first_color() {
        // 9 series → the 9th gets OKABE_ITO[0] again.
        let labels = ["a", "b", "c", "d", "e", "f", "g", "h", "i"];
        let all_series: Vec<LineSeries> = labels
            .iter()
            .enumerate()
            .map(|(index, label)| {
                series(
                    Some(label),
                    vec![point(
                        label,
                        ChartValue::Number(index as f64),
                        SizeValue::Number((index * 2) as f64),
                    )],
                )
            })
            .collect();
        let output = render_line_chart(
            &data("x", "y", Some("team"), all_series, vec![]),
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
                ChartValue::Date(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()),
                SizeValue::Number(1.0),
            ),
            point(
                "b",
                ChartValue::Date(NaiveDate::from_ymd_opt(2026, 1, 10).unwrap()),
                SizeValue::Number(2.0),
            ),
        ];
        let output = render_line_chart(
            &data("day", "score", None, single(points), vec![]),
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
                ChartValue::Number(1.0),
                SizeValue::Duration(2 * SECONDS_PER_DAY),
            ),
            point(
                "b",
                ChartValue::Number(2.0),
                SizeValue::Duration(4 * SECONDS_PER_DAY),
            ),
        ];
        let output = render_line_chart(
            &data("x", "estimate", None, single(points), vec![]),
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
                ChartValue::Duration(2 * SECONDS_PER_HOUR),
                SizeValue::Number(1.0),
            ),
            point(
                "b",
                ChartValue::Duration(4 * SECONDS_PER_HOUR),
                SizeValue::Number(2.0),
            ),
        ];
        let output = render_line_chart(
            &data("estimate", "y", None, single(points), vec![]),
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
        let points = vec![point("a", ChartValue::Number(1.0), SizeValue::Number(2.0))];
        let output = render_line_chart(
            &data(
                "x",
                "y",
                None,
                single(points),
                vec![
                    unplaced_missing("missing-x", Some("Missing X"), "x"),
                    unplaced_missing("missing-y", Some("Missing Y"), "y"),
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
            point("a", ChartValue::Number(1.0), SizeValue::Number(2.0)),
            point("b", ChartValue::Number(2.0), SizeValue::Number(4.0)),
        ];
        let output = render_line_chart(
            &data("x", "y", None, single(points), vec![]),
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
                vec![unplaced_missing("orphan", Some("Orphan"), "x")],
            ),
            "../workdown-items",
            "",
        );
        assert!(!output.contains("<svg"));
        assert!(output.contains("## Unplaced\n"));
        assert!(output.contains("[Orphan](../workdown-items/orphan.md) — missing `x`"));
    }
}
