//! View configuration types.
//!
//! Deserialized from `.workdown/views.yaml`. Declares persisted views that
//! `workdown render` produces static files for, and that `workdown serve`
//! exposes as live bookmarks. See `docs/views.md` for the design note.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A parsed and validated `views.yaml` file.
///
/// Produced by [`crate::parser::views::parse_views`]. Invalid shapes
/// (missing required slots, duplicate ids) are rejected at parse time.
#[derive(Debug, Clone)]
pub struct Views {
    /// Directory (relative to project root) where `workdown render`
    /// writes the rendered view files. Sourced from the optional
    /// `directory:` key in `views.yaml`; defaults to `"views"`.
    pub output_dir: PathBuf,
    pub views: Vec<View>,
}

/// A single view entry: id, optional filters, and type-specific config.
#[derive(Debug, Clone)]
pub struct View {
    pub id: String,

    /// AND-combined filter expressions. Each string uses the
    /// `workdown query --where` grammar, parsed by
    /// [`crate::query::parse::parse_where`].
    pub where_clauses: Vec<String>,

    /// Schema field name whose value is used as each item's display title
    /// on cards, rows, nodes, and bars. When `None`, renderers fall back to
    /// the item id. Cross-cutting: applies uniformly to every view type.
    pub title: Option<String>,

    pub kind: ViewKind,
}

/// Type-specific configuration. Each variant carries only the slots valid
/// for that view type — invalid combinations are unrepresentable.
#[derive(Debug, Clone)]
pub enum ViewKind {
    Board {
        field: String,
    },
    Tree {
        field: String,
    },
    Graph {
        field: String,
        /// Optional `Link` field whose chain becomes Mermaid `subgraph`
        /// nesting when the renderer emits the view. Cardinality must be
        /// single-target (one parent per item) so each item lives in
        /// exactly one box. Inverse names are rejected by `views_check`.
        group_by: Option<String>,
    },
    Table {
        columns: Vec<String>,
    },
    Gantt {
        start: String,
        /// Date field naming the bar's end. Mutually exclusive with
        /// `duration`: exactly one is set per view, enforced by
        /// `views_check`.
        end: Option<String>,
        /// Duration field used to compute the bar's end as
        /// `start + duration`. Mutually exclusive with `end`.
        duration: Option<String>,
        group: Option<String>,
    },
    BarChart {
        group_by: String,
        value: Option<String>,
        aggregate: Aggregate,
    },
    LineChart {
        x: String,
        y: String,
    },
    Workload {
        start: String,
        end: String,
        effort: String,
    },
    Metric {
        label: Option<String>,
        value: Option<String>,
        aggregate: Aggregate,
    },
    Treemap {
        group: String,
        size: String,
    },
    Heatmap {
        x: String,
        y: String,
        value: Option<String>,
        aggregate: Aggregate,
        bucket: Option<Bucket>,
    },
}

impl ViewKind {
    /// The [`ViewType`] discriminant for this view configuration.
    pub fn view_type(&self) -> ViewType {
        match self {
            Self::Board { .. } => ViewType::Board,
            Self::Tree { .. } => ViewType::Tree,
            Self::Graph { .. } => ViewType::Graph,
            Self::Table { .. } => ViewType::Table,
            Self::Gantt { .. } => ViewType::Gantt,
            Self::BarChart { .. } => ViewType::BarChart,
            Self::LineChart { .. } => ViewType::LineChart,
            Self::Workload { .. } => ViewType::Workload,
            Self::Metric { .. } => ViewType::Metric,
            Self::Treemap { .. } => ViewType::Treemap,
            Self::Heatmap { .. } => ViewType::Heatmap,
        }
    }
}

/// The v1 view types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewType {
    Board,
    Tree,
    Graph,
    Table,
    Gantt,
    BarChart,
    LineChart,
    Workload,
    Metric,
    Treemap,
    Heatmap,
}

impl std::fmt::Display for ViewType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Board => "board",
            Self::Tree => "tree",
            Self::Graph => "graph",
            Self::Table => "table",
            Self::Gantt => "gantt",
            Self::BarChart => "bar_chart",
            Self::LineChart => "line_chart",
            Self::Workload => "workload",
            Self::Metric => "metric",
            Self::Treemap => "treemap",
            Self::Heatmap => "heatmap",
        };
        f.write_str(s)
    }
}

/// Aggregation functions used by chart / metric views.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Aggregate {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}

impl std::fmt::Display for Aggregate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Count => "count",
            Self::Sum => "sum",
            Self::Avg => "avg",
            Self::Min => "min",
            Self::Max => "max",
        };
        f.write_str(s)
    }
}

/// Date bucketing for heatmap axes bound to date fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Bucket {
    Day,
    Week,
    Month,
}

impl std::fmt::Display for Bucket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
        };
        f.write_str(s)
    }
}
