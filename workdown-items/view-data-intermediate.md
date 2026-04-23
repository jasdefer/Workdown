---
id: view-data-intermediate
type: issue
status: to_do
title: Design ViewData and extractors
parent: renderers
depends_on: [field-value-native-date, views-title-slot]
---

Define the shared intermediate representation renderers and the live server
both consume, plus the extractor functions that build it from items + a view
config. This is the one piece of business logic for visualization — every
`render_*` and every server view endpoint is pure formatting over these
structs.

## Decisions

These are settled going into implementation:

### Module

New top-level module `core::view_data`, parallel to `core::query`. Single
public entry point:

```rust
pub fn extract(view: &View, store: &Store, schema: &Schema)
    -> Result<Extraction, ExtractError>;

pub struct Extraction {
    pub data: ViewData,
    pub warnings: Vec<Diagnostic>,
}
```

Extraction degrades gracefully: broken links on graph nodes, items missing
the field a board groups by, `start > end` on a gantt bar — these surface
as warnings on the returned `Extraction`, not fatal errors. Consistent with
ADR-001 (snapshot validation, save-with-warning).

### Cards carry typed values, not pre-formatted strings

Renderers consume the intermediate; the server serializes it as JSON. Both
want underlying types. Markdown renderers format at render time via a
shared `format_field_value` helper (already in `core::query::format`). JSON
for the server comes free via `serde::Serialize`.

### Each card carries an optional title

Resolved by the extractor from the view's `title:` slot in views.yaml
(added by a separate prerequisite issue). Renderers fall back to
`id.as_str()` when `title` is `None`.

### Reuse existing types

- `Aggregate` and `Bucket` — from `core::model::views`, do not redeclare
- `WorkItemId` — for all ids
- `FieldValue` — carried directly on cards; includes `Date(NaiveDate)` after
  `field-value-native-date` lands
- `Diagnostic` — warnings use the existing type

### Filtering reuses the query pipeline

Extractor applies `view.where_clauses` by:

1. Parsing each string via `core::query::parse::parse_where`
2. AND-combining into a single `Predicate`
3. Calling `core::query::engine::filter_and_sort` to get filtered items
4. Handing those to the variant-specific extractor

No second filter path. Parse failures here indicate a bug — `views_check`
already validates where-clauses at load time.

### Determinism (for CI-diff-clean static output)

- Cards/rows/nodes: sort by `id` ascending
- Board columns: order by the schema-declared values of the choice field;
  synthetic "no value" column last
- Bar chart bars / treemap siblings: sorted by key ascending
- Heatmap axis labels: chronological for date buckets, alphabetical otherwise
- Workload buckets: dense daily, chronological
- Missing-value buckets: explicit (`Option<String>` on `BoardColumn.value`,
  `Option<Card>` on `TreemapNode.card`), never a magic sentinel string

## Initial struct sketch (draft — not final)

First-pass shapes. Naming, exact field sets, and serde attributes will be
refined while writing each extractor — expect changes. The *decisions*
above are the fixed contract; the code below is a starting point.

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ViewData {
    Board(BoardData),
    Tree(TreeData),
    Graph(GraphData),
    Table(TableData),
    Gantt(GanttData),
    BarChart(BarChartData),
    LineChart(LineChartData),
    Workload(WorkloadData),
    Metric(MetricData),
    Treemap(TreemapData),
    Heatmap(HeatmapData),
}

pub struct Card {
    pub id: WorkItemId,
    pub title: Option<String>,
    pub fields: Vec<CardField>,
}
pub struct CardField { pub name: String, pub value: FieldValue }

pub struct BoardData { pub field: String, pub columns: Vec<BoardColumn> }
pub struct BoardColumn {
    pub value: Option<String>,   // None = synthetic "no value" bucket
    pub cards: Vec<Card>,
}

pub struct TreeData { pub field: String, pub roots: Vec<TreeNode> }
pub struct TreeNode { pub card: Card, pub children: Vec<TreeNode> }

pub struct GraphData {
    pub field: String,
    pub nodes: Vec<Card>,
    pub edges: Vec<Edge>,
}
pub struct Edge { pub from: WorkItemId, pub to: WorkItemId }

pub struct TableData { pub columns: Vec<String>, pub rows: Vec<TableRow> }
pub struct TableRow {
    pub id: WorkItemId,
    pub cells: Vec<Option<FieldValue>>,
}

pub struct GanttData {
    pub start_field: String,
    pub end_field: String,
    pub group_field: Option<String>,
    pub bars: Vec<GanttBar>,
}
pub struct GanttBar {
    pub card: Card,
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub group: Option<String>,
}

pub struct BarChartData {
    pub group_by: String,
    pub value_field: Option<String>,
    pub aggregate: Aggregate,
    pub bars: Vec<BarChartBar>,
}
pub struct BarChartBar { pub group: String, pub value: f64 }

pub struct LineChartData {
    pub x_field: String,
    pub y_field: String,
    pub points: Vec<LinePoint>,
}
pub struct LinePoint { pub id: WorkItemId, pub x: AxisValue, pub y: f64 }

#[derive(Serialize)]
#[serde(untagged)]
pub enum AxisValue { Number(f64), Date(NaiveDate) }

pub struct WorkloadData {
    pub start_field: String,
    pub end_field: String,
    pub effort_field: String,
    pub buckets: Vec<WorkloadBucket>,
}
pub struct WorkloadBucket { pub date: NaiveDate, pub total: f64 }

pub struct MetricData {
    pub label: Option<String>,
    pub aggregate: Aggregate,
    pub value_field: Option<String>,
    pub value: f64,
}

pub struct TreemapData {
    pub group_field: String,
    pub size_field: String,
    pub root: TreemapNode,
}
pub struct TreemapNode {
    pub card: Option<Card>,
    pub size: f64,
    pub children: Vec<TreemapNode>,
}

pub struct HeatmapData {
    pub x_field: String,
    pub y_field: String,
    pub value_field: Option<String>,
    pub aggregate: Aggregate,
    pub bucket: Option<Bucket>,
    pub x_labels: Vec<String>,
    pub y_labels: Vec<String>,
    pub cells: Vec<HeatmapCell>,
}
pub struct HeatmapCell { pub x: String, pub y: String, pub value: f64 }
```

## Scope

- Data structs (refined during implementation)
- One extractor per variant: `fn extract_board(items, &Schema, &BoardConfig)
  -> Result<BoardData, _>`, etc.
- Title resolution from the view's `title:` slot
- Graceful degradation: invalid per-item data emits warnings, not errors
- Unit tests with small fixtures per variant

## Open during implementation

Decide when writing each extractor; not blockers for landing:

- Graph edge direction: source-of-field → target for forward links;
  inverted for inverse names (`children` derived from `parent`) so
  semantics stay "dependent → dependency" regardless of which side
  declares the link
- Board on a multichoice field: card appears in each matching column
- Tree with orphaned parent link: orphan becomes a root, warning emitted
- Gantt / workload with `start > end` or missing dates: skip bar/item, warn
- Aggregate null handling: skip nulls (standard)
- Aggregates with zero matches: avg/min/max return no bar (dropped from
  result)
- Heatmap date bucketing rule: week = ISO week (Monday start); month =
  first-of-month

## Out of scope

- Rendering — each format is its own issue per view type
- Multi-hop relation traversal — one-hop (`parent.status`) is all v1 supports
- `where`-clause OR nesting — deferred until needed
- Runtime field selection (live server override) — future enhancement per
  ADR-006
