//! View data extraction.
//!
//! Reads work items + a view configuration and produces a [`ViewData`]
//! struct that both Markdown renderers and the live web server consume.
//! This is the single piece of business logic for visualization; formatters
//! and endpoints above this layer are pure presentation over the extracted
//! struct.
//!
//! The caller is responsible for running `views_check` first — field
//! references, slot/type mismatches, and `where`-clause syntax are all
//! validated there. Extraction assumes those invariants hold; violating
//! them is a programming error and panics.
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
mod test_support;

use serde::Serialize;

use crate::model::calendar::WorkingCalendar;
use crate::model::schema::Schema;
use crate::model::views::{View, ViewKind};
use crate::store::Store;

pub use bar_chart::{BarChartBar, BarChartData};
pub use board::{BoardColumn, BoardData};
pub use common::{
    build_card, resolve_title, AggregateValue, AxisValue, Card, CardField, SizeValue, UnplacedCard,
    UnplacedReason,
};
pub use gantt::{GanttBar, GanttData};
pub use gantt_by_depth::{GanttByDepthData, Level};
pub use gantt_by_initiative::{GanttByInitiativeData, Initiative};
pub use graph::{Edge, GraphData};
pub use heatmap::{HeatmapCell, HeatmapData};
pub use line_chart::{LineChartData, LinePoint};
pub use metric::{MetricData, MetricRowData};
pub use table::{TableData, TableRow};
pub use tree::{TreeData, TreeNode};
pub use treemap::{TreemapData, TreemapNode};
pub use workload::{WorkloadBucket, WorkloadData, WorkloadUnit};

/// Extracted, fully-resolved data for a single view.
#[derive(Debug, Clone, Serialize)]
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

/// Extract view data for rendering or JSON serialization.
///
/// Infallible by design — structural problems (invalid slot, bad field
/// reference, malformed `where` clause) are caught by `views_check`;
/// data-level problems (missing dates, invalid ranges, non-numeric
/// aggregate inputs) live in each variant's `unplaced` list.
///
/// `config_calendar` is the project-wide working calendar from
/// `config.yaml`. Workload views fall back to it when they don't set
/// their own `working_days:` override; other view kinds ignore it.
pub fn extract(
    view: &View,
    store: &Store,
    schema: &Schema,
    config_calendar: &WorkingCalendar,
) -> ViewData {
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
        ViewKind::Table { .. } => ViewData::Table(table::extract_table(view, store, schema)),
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
