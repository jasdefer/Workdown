# views.yaml design

`views.yaml` declares persisted views that `workdown render` produces static files for, and that `workdown serve` exposes as live bookmarks. It is the companion to `schema.yaml` (which defines fields) and `resources.yaml` (which defines named entities).

## Top-level shape

```yaml
views:
  - { id: <unique>, type: <view-type>, ... }
  - { id: <unique>, type: <view-type>, ... }
```

- `views:` is a list. Order is preserved.
- Each entry has a unique `id` (used as the output filename and the web-app bookmark URL).
- Each entry has a `type` that discriminates the type-specific slots.

Every view entry also accepts an optional `where:` list (see below).

Unknown top-level fields are rejected. Unknown per-view slots are rejected.

## View types (v1)

| Type | Required slots | Optional slots | Output formats |
|---|---|---|---|
| `board` | `field` (choice) | — | html, md |
| `tree` | `field` (link) | — | html, md, mermaid |
| `graph` | `field` (links) | — | html, mermaid (md = mermaid fenced block) |
| `table` | `columns` | — | html, md |
| `gantt` | `start`, `end` | `group` | mermaid, html (md = mermaid fenced block) |
| `bar_chart` | `group_by`, `aggregate` | `value` | html, mermaid (md = mermaid fenced block) |
| `line_chart` | `x`, `y` | — | html |
| `workload` | `start`, `end`, `effort` | — | html |
| `metric` | `aggregate` | `value`, `label` | html, md |
| `treemap` | `group`, `size` | — | html |
| `heatmap` | `x`, `y`, `aggregate` | `value`, `bucket` | html |

Slot semantics:
- **`field`** — a single schema field name (referenced by type: `choice` for board, `link` for tree, `links` for graph).
- **`columns`** — ordered list of field names.
- **`start` / `end`** — `date` fields. `effort` is numeric.
- **`group_by` / `group`** — field name used for grouping (`choice` for bar chart; `link` for gantt / treemap).
- **`value`** — numeric field to aggregate. Omitted when `aggregate: count`.
- **`aggregate`** — one of `count`, `sum`, `avg`, `min`, `max`.
- **`x` / `y`** — field names for axis values (numeric or date for line chart; categorical or date for heatmap).
- **`bucket`** — date bucketing for a heatmap axis bound to a date field: `day`, `week`, or `month`.
- **`label`** — display label for a metric.

Type compatibility between a slot and a schema field (e.g. `board.field` must resolve to a `choice` field) is checked in `workdown validate` — the subject of the `views-cross-file-validation` and `views-validate-integration` issues, not the view-yaml parser itself.

## Filters — `where:`

A list of strings. Each string is a single expression using the `workdown query --where` grammar (`core::query::parse::parse_where`). Multiple strings are AND-combined — identical to how the CLI combines multiple `--where` flags.

```yaml
where:
  - "type=issue"
  - "status!=removed"
  - "parent.status=in_progress"
```

The same grammar covers equality, inequality, numeric comparison, substring match, regex, presence, and single-hop relation traversal (`parent.status`). See the documentation of `parse_where` for the full expression reference.

When the view renders, items are filtered by the combined predicate before any aggregation or extraction runs.

OR nesting is not supported in v1 (the CLI's inline `status=open,in_progress` form covers the common case). A structured `or:` branch can be added later without breaking existing configs.

## Output paths

Every view writes static files to fixed paths derived from its `id` and type:

```
views/<id>.html
views/<id>.md         # where the type supports markdown
views/<id>.mermaid    # where the type supports mermaid
```

Paths are not customizable in v1. `workdown render` creates the `views/` directory if it does not exist. Re-running without item changes produces identical files (CI-diff clean).

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
    columns: [id, title, type, status, start_date, end_date]
  - id: roadmap
    type: gantt
    start: start_date
    end: end_date
    group: parent
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
  - id: open-count
    type: metric
    aggregate: count
    label: Open items
    where: ["status=to_do,in_progress"]
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

## Extensibility

Adding a new view type:

1. Add a variant to `ViewType` and `ViewKind` in `crates/core/src/model/views.rs`.
2. Add the type-specific slot handling in `crates/core/src/parser/views.rs::convert_view`.
3. Add a variant to `ViewData` and an extractor (`view-data-intermediate` issue).
4. Add a per-view-type render issue producing the applicable output formats.
5. Update `crates/core/defaults/views.schema.json` (added in `views-json-schema`) with the new discriminator branch.

Existing configurations are unaffected — the change is purely additive.

## Considered but deferred

- **`output:` customization** — fixed paths per view id keep v1 simple. Revisit if users need custom locations.
- **`dashboard`** — composition of multiple metrics / charts on a single page. Useful once `metric` has a few users.
- **`calendar`** — one event per item placed on a date. Not widely needed for engineering-project workflows; add when asked for.
- **OR nesting in `where`** — structured `or:`/`not:` branches. Today's AND-of-strings plus the inline `=a,b,c` form covers the common case.
- **Multi-hop relation traversal** — `grandparent.status` etc. Parser-level change, orthogonal to views.yaml shape.
