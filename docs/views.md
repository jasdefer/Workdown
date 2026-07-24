# views.yaml design

`views.yaml` declares persisted views that `workdown render` produces static files for, and that `workdown serve` exposes as live bookmarks. It is the companion to `schema.yaml` (which defines fields) and `resources.yaml` (which defines named entities).

## Top-level shape

```yaml
directory: views    # optional, defaults to "views"
views:
  - { id: <unique>, type: <view-type>, ... }
  - { id: <unique>, type: <view-type>, ... }
```

- `views:` is a list. Order is preserved.
- Each entry has a unique `id` (used as the output filename and the web-app bookmark URL).
- Each entry has a `type` that discriminates the type-specific slots.
- `directory:` (optional) — output directory where `workdown render` writes view files, relative to project root. Defaults to `"views"`. See [Output paths](#output-paths).

Every view entry also accepts two optional cross-cutting slots: `where:` (filters) and `display:` (display roles). Both are described below.

Unknown top-level fields are rejected. Unknown per-view slots are rejected.

## View types (v1)

Every view type also accepts the cross-cutting optional slots `where:` and `display:` on top of what the table below lists.

`workdown render` writes one Markdown file per view (`<directory>/<id>.md`); the table lists what each renderer emits today.

| Type | Required slots | Optional slots | Renderer status | Output |
|---|---|---|---|---|
| `board` | `field` | — | shipped | Sectioned bullet list |
| `tree` | `field` | — | shipped | Nested bullet list |
| `graph` | `field` | `group_by` | shipped | Mermaid `flowchart TD` |
| `table` | — | — | shipped | GFM table (columns from the `fields` display role) |
| `gantt` | `start` + one of (`end`, `duration`, `after`+`duration`) | `group` | shipped | Mermaid `gantt` block |
| `gantt_by_initiative` | `start` + one input mode, `root_link` | — | shipped | One Mermaid `gantt` block per initiative |
| `gantt_by_depth` | `start` + one input mode, `depth_link` | — | shipped | One Mermaid `gantt` block per non-empty depth level |
| `bar_chart` | `group_by`, `aggregate` | `value` | shipped | Inline SVG (horizontal bars) + `## Values` table |
| `line_chart` | `x`, `y` | `group` | shipped | Inline SVG (multi-series points + lines) |
| `workload` | `start`, `end`, `effort` | `working_days` | shipped | Inline SVG (vertical bars, one per working day) + `## Values` table |
| `metric` | `metrics` (list of rows; each row needs `aggregate`, optionally `value`, `label`, `where`) | — | shipped | GFM table, one row per metric |
| `treemap` | `group`, `size` | — | shipped | Nested bullet list with size + share-of-parent annotations |
| `heatmap` | `x`, `y`, `aggregate` | `value`, `bucket` | shipped | Inline SVG (color grid + colorbar) + pivoted `## Values` table |

Slot semantics:
- **`field`** — a single schema field name. Type per view: `choice`/`multichoice`/`string` for board, `link` for tree, `links` for graph.
- **`start` / `end`** — `date` fields. **`duration`** — `duration` field; mutually exclusive with `end`. **`after`** — `link`/`links` field naming each item's predecessors (predecessor mode); requires `duration`, forbids `end`. Predecessor fields must have `allow_cycles: false` and not be inverse names.
- **`group_by`** — categorical field for bar chart grouping; `link` field for graph subgraph nesting. **`group`** — field for in-chart sectioning (gantt only). **`root_link`** — single `link` field whose chain identifies each item's top-level ancestor (`gantt_by_initiative`). **`depth_link`** — single `link` field whose chain depth places each item in a level (`gantt_by_depth`). Both must have `allow_cycles: false` and not be inverse names.
- **`value`** — numeric field to aggregate. Omitted when `aggregate: count`.
- **`aggregate`** — one of `count`, `sum`, `avg`, `min`, `max`.
- **`x` / `y`** — field names for axis values (numeric or date for line chart; categorical or date for heatmap).
- **`bucket`** — date bucketing for a heatmap axis bound to a date field: `day`, `week`, or `month`.
- **`metrics`** — list of stat rows for a metric view. Each row sets its own `aggregate` (required), `value` (numeric, date, or duration field — required for non-count aggregates), optional `label` (auto-derived from aggregate + field when omitted), and optional per-row `where` AND-combined with the view-level filter.
- **`working_days`** — list of weekday names (`monday`, `tuesday`, …, `sunday`; full lowercase, no abbreviations) that count as work days for a `workload` view. Effort spreads only across listed days inside `[start..=end]`; days outside the list never produce buckets, and items whose interval falls entirely outside them drop into the `## Unplaced` footer. Optional — when omitted, falls back to the project-level `working_days` from `config.yaml`, which itself defaults to `[monday, tuesday, wednesday, thursday, friday]` when not set there either.

Type compatibility between a slot and a schema field (e.g. `board.field` must resolve to a `choice` field) is checked in `workdown validate`. See the "Cross-file validation" section below for the full list of checks.

### Description line below the heading

Every shipped renderer emits a one-sentence caption between the `# Heading` and the chart/list/table content. The sentence is built from the view config and includes the schema field names it draws from, so a reader opening a rendered file in GitHub knows what they're looking at without flipping back to `views.yaml`. Renaming a field in the schema is reflected on the next render.

## Filters — `where:`

A list of strings. Each string is a single expression using the `workdown query --where` grammar (`core::query::parse::parse_where`). Multiple strings are AND-combined — identical to how the CLI combines multiple `--where` flags.

```yaml
where:
  - "type=issue"
  - "status!=removed"
  - "parent.status=in_progress"
```

The same grammar covers equality, inequality, numeric comparison, substring match, regex, presence, and single-hop relation traversal (`parent.status`). See the documentation of `parse_where` for the full expression reference.

Field references inside `where:` expressions are validated against `schema.yaml`: local field names must be defined in the schema (or be `id`), and relation names must resolve to a `link`/`links` field or a known inverse name (e.g. `children` resolving to the inverse of `parent`).

When the view renders, items are filtered by the combined predicate before any aggregation or extraction runs.

OR nesting is not supported in v1 (the CLI's inline `status=open,in_progress` form covers the common case). A structured `or:` branch can be added later without breaking existing configs.

## Display roles — `display:`

Which schema field fills each presentation role. One closed vocabulary across every view kind; each kind renders the roles in its own idiom and ignores roles it cannot place.

| Role | Meaning | Board | Table/Tree | Graph | Gantt |
|---|---|---|---|---|---|
| `title` | Primary label | card heading | row title | node label | bar label |
| `subtitle` | Secondary line | card second line | — | node second line | — |
| `fields` | Ordered detail fields | card badges | columns | tooltip lines | tooltip |
| `color` | Surface tint | card wash | row wash | node fill | bar fill |

```yaml
views:
  - id: status-board
    type: board
    field: status         # structural — defines the view
    display:              # presentation — decorates it
      title: title
      subtitle: assignee
      fields: [type, effort]
```

Resolution per role, first match wins:

1. Runtime per-session override in the web app (the Display bar; stored per browser, never persisted).
2. The view's own `display:` block in `views.yaml`.
3. Project-wide `defaults.display` in `config.yaml` — define roles once instead of repeating them on every view.
4. Per-kind hardcoded fallback: `title` → the item `id`; `fields` → every schema field in declaration order; `subtitle` → nothing; `color` → the first `color`-typed field in schema order.

- Every role is optional everywhere; a view with no `display:` block inherits entirely.
- `fields` distinguishes absent from empty: omitting the key inherits down the ladder, while an explicit `fields: []` means "show no fields" (a quiet card, a zero-column table) and shadows lower rungs like any other set value.
- Role fields must resolve in `schema.yaml`. The text roles accept any field type — every value renders as text — and also the virtual `id`. The `color` role requires a `color`-typed field (`id` is rejected — it can never feed a tint), or the sentinel `none` to render the view untinted at any rung (`none` is reserved — `schema.yaml` rejects it as a field name).
- Aggregate/chart kinds (`bar_chart`, `line_chart`, `heatmap`, `metric`, `treemap`, `workload`) accept the block uniformly but ignore item-level roles at render time.
- `defaults.display` role fields are validated against the schema (surfaced through `workdown validate` and the serve banner, pointing at `config.yaml`). Unlike a per-view role error — which makes that one view unrenderable — a bad project-wide default is reported but non-blocking: every view keeps rendering on its fallback, since one config typo shouldn't blank all views at once.

## Output paths

Every view writes a single Markdown file. Filenames are `<id>.md`, written into the directory named by the top-level `directory:` key (default `"views"`):

```
<directory>/<id>.md
```

Filenames are not customizable — they always derive from `id`. The directory is. `workdown render` creates the directory if it does not exist. Re-running without item changes produces identical files (CI-diff clean).

The live server does not consume these files — it re-runs the renderers against the current working tree. Static files are for committed, shareable snapshots (READMEs, GitHub previews, CI artifacts).

## Example

```yaml
views:
  - id: status-board
    type: board
    field: status
    where:
      - "type=issue"
      - "status!=removed"
  - id: hierarchy
    type: tree
    field: parent
  - id: deps
    type: graph
    field: depends_on
  - id: all-items
    type: table
    display:
      fields: [id, title, type, status, start_date, end_date]
  - id: roadmap
    type: gantt
    start: start_date
    end: end_date
    group: parent
  - id: roadmap-by-initiative
    type: gantt_by_initiative
    start: start_date
    end: end_date
    root_link: parent
  - id: roadmap-by-depth
    type: gantt_by_depth
    start: start_date
    end: end_date
    depth_link: parent
  - id: effort-by-status
    type: bar_chart
    group_by: status
    value: effort
    aggregate: sum
  - id: estimate-vs-actual
    type: line_chart
    x: estimate
    y: actual_effort
  - id: capacity
    type: workload
    start: start_date
    end: end_date
    effort: effort
  - id: project-stats
    type: metric
    where: ["status!=removed"]
    metrics:
      - label: Total
        aggregate: count
      - label: In progress
        aggregate: count
        where: ["status=in_progress"]
      - label: Sum points
        aggregate: sum
        value: points
  - id: effort-by-milestone
    type: treemap
    group: parent
    size: effort
  - id: activity
    type: heatmap
    x: end_date
    y: assignee
    aggregate: count
    bucket: week
```

## Cross-file validation

`workdown validate` runs a set of checks that compare `views.yaml` against `schema.yaml`. All findings are errors in v1 (no warnings):

- **Reference resolution** — every field name referenced by a view slot must exist in `schema.fields` (the virtual `id` field is always accepted).
- **Type compatibility** — the slot dictates the allowed field type(s). For example: `board.field` must be `choice`, `multichoice`, or `string`; `tree.field` must be `link`; `graph.field` must be `links`; `gantt.start`/`gantt.end` must be `date`, `gantt.duration` must be `duration`; numeric slots accept `integer` or `float`, plus `duration` where the renderer can format it (`treemap.size`, `line_chart.x`/`y`, `workload.effort`, and aggregation slots `bar_chart.value`, `heatmap.value`, `metric.value`). The text display roles (`display.title`, `display.subtitle`, `display.fields[*]`) are existence-only — any field type is accepted; `display.color` must name a `color`-typed field (or be the sentinel `none`).
- **Gantt input modes** — every gantt-family view (`gantt`, `gantt_by_initiative`, `gantt_by_depth`) must declare `start` plus exactly one of: `end`, `duration`, or `after`+`duration`. `end` and `duration` together is rejected; `after` requires `duration` and forbids `end`.
- **Predecessor / partition link slots** — `gantt.after`, `gantt_by_initiative.root_link`, and `gantt_by_depth.depth_link` must point at a `link`/`links` field (single-target only for `root_link`/`depth_link`) with `allow_cycles: false`, and not at an inverse relation name (e.g. `children` when `parent.inverse: children`).
- **Heatmap bucket coupling** — if `bucket:` is set, at least one of `x` or `y` must resolve to a `date` field.
- **Metric row count + value** — within a metric row, `aggregate: count` combined with `value:` is an error (count takes no value field). Diagnostics carry the row index so messages pinpoint which row failed.
- **Where-clause parsing** — each string in a view's `where:` list must parse as a valid `--where` expression.
- **Where-clause field references** — local field names must exist in `schema.fields` (or be `id`); relation names (left side of a dot) must resolve to a `link`/`links` field or a known inverse name.

Load-time failures surface through the same diagnostic stream: read/YAML errors reuse the generic `FileError` (pointing at `views.yaml`), while duplicate ids and missing required slots get dedicated variants (`ViewDuplicateId`, `ViewMissingSlot`) so callers like the live server can highlight specific problems in the UI.

`views.yaml` is optional — if the file is absent, these checks are skipped and no view-level diagnostics are produced. The companion `views.schema.json` shipped with the CLI provides editor autocomplete only and is not loaded at validate-time; see [ADR-005](adr/005-json-schema-editor-only.md).

## Extensibility

Adding a new view type:

1. Add a variant to `ViewType` and `ViewKind` in `crates/core/src/model/views.rs`.
2. Add `RawView` field(s) and a `convert_view` arm in `crates/core/src/parser/views.rs`.
3. Add a `views_check` arm in `crates/core/src/views_check.rs` (slot-type checks; cross-slot rules; new diagnostic kinds in `crates/core/src/model/diagnostic.rs` if needed, plus their entries in the three exhaustive `DiagnosticKind` matches in `validate.rs`, `commands/render.rs`, and `operations/add.rs`).
4. Add a `ViewData::Foo` variant + extractor module under `crates/core/src/view_data/`, then export and dispatch from `view_data/mod.rs`.
5. Add a renderer module under `crates/cli/src/render/`, an arm in `render::description::description_for`, and a dispatch arm in `commands/render.rs`.
6. Add a `oneOf` ref + definition in `crates/core/defaults/views.schema.json` for editor autocomplete.

Existing configurations are unaffected — the change is purely additive.

## Considered but deferred

- **`output:` customization** — fixed paths per view id keep v1 simple. Revisit if users need custom locations.
- **`dashboard`** — composition of multiple metrics / charts on a single page. Useful once `metric` has a few users.
- **`calendar`** — one event per item placed on a date. Not widely needed for engineering-project workflows; add when asked for.
- **OR nesting in `where`** — structured `or:`/`not:` branches. Today's AND-of-strings plus the inline `=a,b,c` form covers the common case.
- **Multi-hop relation traversal** — `grandparent.status` etc. Parser-level change, orthogonal to views.yaml shape.
