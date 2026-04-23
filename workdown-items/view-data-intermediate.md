---
id: view-data-intermediate
type: issue
status: to_do
title: Design ViewData and extractors
parent: renderers
---

Define the shared intermediate representation renderers consume, plus the extractor functions that build it from items + a view config.

## Proposed shape

```rust
pub enum ViewData {
    Board(BoardView),
    Tree(TreeView),
    Graph(GraphView),
    Table(TableView),
    Gantt(GanttView),
    BarChart(BarChartView),
    LineChart(LineChartView),
    Workload(WorkloadView),
    Metric(MetricView),
    Treemap(TreemapView),
    Heatmap(HeatmapView),
}

pub struct CardSummary {
    pub id: String,
    pub title: String,
    pub fields: BTreeMap<String, String>,
}

pub enum Aggregate { Count, Sum, Avg, Min, Max }

pub struct BoardView { pub field: String, pub columns: Vec<BoardColumn> }
pub struct BoardColumn { pub value: String, pub cards: Vec<CardSummary> }

pub struct TreeView { pub field: String, pub roots: Vec<TreeNode> }
pub struct TreeNode { pub item: CardSummary, pub children: Vec<TreeNode> }

pub struct GraphView {
    pub field: String,
    pub nodes: Vec<CardSummary>,
    pub edges: Vec<(String, String)>,
}

pub struct TableView {
    pub columns: Vec<String>,
    pub rows: Vec<CardSummary>,
}

pub struct GanttView {
    pub start_field: String,
    pub end_field: String,
    pub group_field: Option<String>,
    pub bars: Vec<GanttBar>,
}
pub struct GanttBar {
    pub item: CardSummary,
    pub start: chrono::NaiveDate,
    pub end: chrono::NaiveDate,
    pub group: Option<String>,
}

pub struct BarChartView {
    pub group_by: String,
    pub value_field: Option<String>,   // None when aggregate == Count
    pub aggregate: Aggregate,
    pub bars: Vec<(String, f64)>,      // (group value, aggregated number)
}

pub struct LineChartView {
    pub x_field: String,
    pub y_field: String,
    pub points: Vec<LinePoint>,
}
pub struct LinePoint { pub item_id: String, pub x: f64, pub y: f64 }

pub struct WorkloadView {
    pub start_field: String,
    pub end_field: String,
    pub effort_field: String,
    pub series: Vec<(chrono::NaiveDate, f64)>,  // daily buckets
}

pub struct MetricView {
    pub label: String,
    pub value: f64,
    pub aggregate: Aggregate,
    pub value_field: Option<String>,
}

pub struct TreemapView {
    pub group_field: String,
    pub size_field: String,
    pub root: TreemapNode,
}
pub struct TreemapNode {
    pub item: Option<CardSummary>,    // None for synthetic roots
    pub size: f64,
    pub children: Vec<TreemapNode>,
}

pub struct HeatmapView {
    pub x_field: String,
    pub y_field: String,
    pub value_field: Option<String>,
    pub aggregate: Aggregate,
    pub cells: Vec<HeatmapCell>,
}
pub struct HeatmapCell { pub x: String, pub y: String, pub value: f64 }
```

## Scope

- Data structs (above, refined during implementation)
- One extractor per variant: `fn extract_board(items, view_cfg) -> BoardView`, etc.
- Apply the view's `where` filter before extraction: parse each string with `core::query::parse::parse_where`, AND-combine, evaluate against items
- Unit tests with small fixtures per variant

## Out of scope

- Rendering — each format is its own issue (per-view-type)
- Multi-hop relation traversal — one-hop (`parent.status`) is all v1 supports
- `where`-clause OR nesting — deferred until needed
