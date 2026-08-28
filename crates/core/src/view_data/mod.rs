//! View data extraction.
//!
//! Reads work items + a view configuration and produces a [`ViewData`]
//! struct that both Markdown renderers and the live web server consume.
//! This is the single piece of business logic for visualization; formatters
//! and endpoints above this layer are pure presentation over the extracted
//! struct.
//!
//! Field references, slot/type mismatches, and `where`-clause syntax
//! are validated by `views_check`, and [`extract`] takes a
//! [`CheckedView`] rather than a bare view so that having run it is a
//! precondition the caller cannot skip. Extraction assumes those
//! invariants hold; violating them is a programming error and panics.
//!
//! Items that pass the filter but can't be turned into the view's natural
//! mark (a gantt bar, a chart point, a heatmap cell) end up in per-variant
//! `unplaced: Vec<UnplacedCard>` lists, carrying the reason. Renderers
//! decide whether to surface them in a separate section or ignore them.

mod aggregate;
pub mod bar_chart;
pub mod board;
pub mod common;
pub mod filter;
pub mod gantt;
pub mod gantt_by_depth;
pub mod gantt_by_initiative;
pub mod graph;
pub mod heatmap;
pub mod line_chart;
pub mod metric;
pub mod table;
mod traverse;
pub mod tree;
pub mod treemap;
pub mod workload;

#[cfg(test)]
pub(crate) mod test_support;

use serde::Serialize;

use crate::model::calendar::WorkingCalendar;
use crate::model::diagnostic::Diagnostic;
use crate::model::schema::Schema;
use crate::model::schema::Severity;
use crate::model::views::{View, ViewKind};
use crate::store::Store;

pub use bar_chart::{BarChartBar, BarChartData};
pub use board::{BoardColumn, BoardData};
pub use common::{
    build_card, effective_fields, resolve_color_field, resolve_subtitle, resolve_title,
    resolved_background, Card, CardField, ChartValue, Column, ItemRef, SizeValue, UnplacedCard,
    UnplacedReason,
};
pub use gantt::{GanttBar, GanttData};
pub use gantt_by_depth::{GanttByDepthData, Level};
pub use gantt_by_initiative::{GanttByInitiativeData, Initiative};
pub use graph::{Edge, GraphData};
pub use heatmap::{HeatmapCell, HeatmapData};
pub use line_chart::{LineChartData, LinePoint, LineSeries};
pub use metric::{MetricData, MetricRowData};
pub use table::{TableData, TableRow};
pub use tree::{TreeData, TreeNode};
pub use treemap::{TreemapData, TreemapNode};
pub use workload::{WorkloadBucket, WorkloadData, WorkloadUnit};

/// Extracted, fully-resolved data for a single view.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ViewData {
    BarChart(BarChartData),
    Board(BoardData),
    Gantt(GanttData),
    GanttByDepth(GanttByDepthData),
    GanttByInitiative(GanttByInitiativeData),
    Graph(GraphData),
    Heatmap(HeatmapData),
    LineChart(LineChartData),
    Metric(MetricData),
    Table(TableData),
    Tree(TreeData),
    Treemap(TreemapData),
    Workload(WorkloadData),
}

/// A view that `views_check` has cleared for extraction.
///
/// The module's precondition, made structural. [`extract`] assumes the
/// check ran and pinned no error to this view — field references
/// resolve, the slot and the field's type are a legal pairing, the
/// `where:` clauses parse. Violating that is a programming error, and
/// it surfaces as a panic deep inside one of the variant extractors.
///
/// Wrapping is the only way to obtain [`extract`]'s first argument, so
/// the assumption is something the compiler asks about rather than
/// something a doc comment asks the caller to remember.
#[derive(Clone, Copy)]
pub struct CheckedView<'a>(&'a View);

impl<'a> CheckedView<'a> {
    /// Clear `view` for extraction, or `None` when the check pinned an
    /// error to it.
    ///
    /// `diagnostics` must be everything [`crate::views_check::evaluate`]
    /// produced for the file `view` came from: a caller that passes a
    /// narrowed list would clear a view whose error it simply left out.
    /// A *warning* pinned to the view clears — it describes a view that
    /// renders perfectly well.
    pub fn new(view: &'a View, diagnostics: &[Diagnostic]) -> Option<Self> {
        let cleared = !diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Error && diagnostic.view_id() == Some(view.id.as_str())
        });
        cleared.then_some(Self(view))
    }

    /// The view this cleared.
    pub fn view(self) -> &'a View {
        self.0
    }
}

/// Extract view data for rendering or JSON serialization.
///
/// Infallible by design — structural problems (invalid slot, bad field
/// reference, malformed `where` clause) are caught by `views_check`,
/// which is what a [`CheckedView`] attests to; data-level problems
/// (missing dates, invalid ranges, non-numeric aggregate inputs) live
/// in each variant's `unplaced` list.
///
/// `config_calendar` is the project-wide working calendar from
/// `config.yaml`. Workload views fall back to it when they don't set
/// their own `working_days:` override; other view kinds ignore it.
pub fn extract(
    checked: CheckedView<'_>,
    store: &Store,
    schema: &Schema,
    config_calendar: &WorkingCalendar,
) -> ViewData {
    let view = checked.view();
    match &view.kind {
        ViewKind::BarChart { .. } => {
            ViewData::BarChart(bar_chart::extract_bar_chart(view, store, schema))
        }
        ViewKind::Board { .. } => ViewData::Board(board::extract_board(view, store, schema)),
        ViewKind::Gantt { .. } => ViewData::Gantt(gantt::extract_gantt(view, store, schema)),
        ViewKind::GanttByDepth { .. } => {
            ViewData::GanttByDepth(gantt_by_depth::extract_gantt_by_depth(view, store, schema))
        }
        ViewKind::GanttByInitiative { .. } => ViewData::GanttByInitiative(
            gantt_by_initiative::extract_gantt_by_initiative(view, store, schema),
        ),
        ViewKind::Graph { .. } => ViewData::Graph(graph::extract_graph(view, store, schema)),
        ViewKind::Heatmap { .. } => {
            ViewData::Heatmap(heatmap::extract_heatmap(view, store, schema))
        }
        ViewKind::LineChart { .. } => {
            ViewData::LineChart(line_chart::extract_line_chart(view, store, schema))
        }
        ViewKind::Metric { .. } => ViewData::Metric(metric::extract_metric(view, store, schema)),
        ViewKind::Table => ViewData::Table(table::extract_table(view, store, schema)),
        ViewKind::Tree { .. } => ViewData::Tree(tree::extract_tree(view, store, schema)),
        ViewKind::Treemap { .. } => {
            ViewData::Treemap(treemap::extract_treemap(view, store, schema))
        }
        ViewKind::Workload { .. } => ViewData::Workload(workload::extract_workload(
            view,
            store,
            schema,
            config_calendar,
        )),
    }
}

#[cfg(test)]
mod checked_view_tests {
    use super::*;

    use std::path::PathBuf;

    use crate::model::diagnostic::{ConfigDiagnosticKind, ViewLocation};
    use crate::model::views::{DisplayConfig, ViewKind};

    /// Only the id matters here — clearance is decided by which
    /// diagnostics name the view, never by what it renders.
    fn view_named(id: &str) -> View {
        View {
            id: id.to_owned(),
            where_clauses: Vec::new(),
            display: DisplayConfig::default(),
            kind: ViewKind::Table,
        }
    }

    fn view_diagnostic(severity: Severity, kind: ConfigDiagnosticKind) -> Diagnostic {
        Diagnostic::config(severity, PathBuf::from("views.yaml"), kind)
    }

    /// A view whose config cannot produce output is not cleared.
    #[test]
    fn an_error_withholds_clearance() {
        let diagnostics = [view_diagnostic(
            Severity::Error,
            ConfigDiagnosticKind::ViewUnknownField {
                location: ViewLocation::view("board", "field"),
                field_name: "nonexistent".to_owned(),
            },
        )];
        assert!(CheckedView::new(&view_named("board"), &diagnostics).is_none());
    }

    /// …but a warning must not. A `where:` operand that can never match
    /// is worth reporting and no reason to withhold the view — hiding it
    /// would be a worse version of the silent empty view the warning
    /// exists to explain.
    #[test]
    fn a_warning_still_clears() {
        let diagnostics = [view_diagnostic(
            Severity::Warning,
            ConfigDiagnosticKind::ViewWhereUnknownValue {
                location: ViewLocation::view("board", "where"),
                raw: "status=nonsense".to_owned(),
                field_name: "status".to_owned(),
                detail: "'nonsense' is not one of: done, open".to_owned(),
            },
        )];
        assert!(CheckedView::new(&view_named("board"), &diagnostics).is_some());
    }

    /// A view carrying both is withheld on the error's account; the
    /// warning neither rescues it nor is lost from the report.
    #[test]
    fn an_error_wins_over_a_warning_on_the_same_view() {
        let diagnostics = [
            view_diagnostic(
                Severity::Warning,
                ConfigDiagnosticKind::ViewWhereUnknownValue {
                    location: ViewLocation::view("board", "where"),
                    raw: "status=nonsense".to_owned(),
                    field_name: "status".to_owned(),
                    detail: "…".to_owned(),
                },
            ),
            view_diagnostic(
                Severity::Error,
                ConfigDiagnosticKind::ViewGanttEndOrDurationRequired {
                    view_id: "board".to_owned(),
                },
            ),
        ];
        assert!(CheckedView::new(&view_named("board"), &diagnostics).is_none());
    }

    /// An error pinned to a *different* view says nothing about this one.
    #[test]
    fn another_views_error_does_not_withhold_clearance() {
        let diagnostics = [view_diagnostic(
            Severity::Error,
            ConfigDiagnosticKind::ViewGanttEndOrDurationRequired {
                view_id: "roadmap".to_owned(),
            },
        )];
        assert!(CheckedView::new(&view_named("board"), &diagnostics).is_some());
    }
}
