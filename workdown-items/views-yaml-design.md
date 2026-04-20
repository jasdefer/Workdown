---
id: views-yaml-design
type: issue
status: done
title: Design views.yaml shape
parent: foundation
---

Produce an initial design for `.workdown/views.yaml`. Output is a documented example file, a Rust struct representation, and a short design note — not the formal JSON Schema (that's the next issue).

## Agreed shape

Top-level: `views:` is a list. `id` is unique across the file. Each entry is a discriminated union on `type`.

View types (v1): `board`, `tree`, `graph`, `table`, `gantt`, `bar_chart`, `line_chart`, `workload`, `metric`, `treemap`, `heatmap`.

Slots per type:
- single-field views (`board`, `tree`, `graph`): `field: <name>`
- `table`: `columns: [...]`
- `gantt`: `start`, `end`, optional `group`
- `bar_chart`: `group_by`, optional `value`, `aggregate`
- `line_chart`: `x`, `y`
- `workload`: `start`, `end`, `effort`
- `metric`: `aggregate`, optional `value`, `label`
- `treemap`: `group`, `size`
- `heatmap`: `x`, `y`, optional `value`, `aggregate`, optional `bucket` when an axis is a date

Filters: `where:` is a list of strings. Each string uses the `workdown query --where` grammar; strings are AND-combined. Parsed by `core::query::parse::parse_where`.

No `output:` field — each view writes a fixed set of files at `views/<id>.<ext>` determined by its type.

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

## Deliverables

- Example `views.yaml` (above or richer) committed as a doc fixture
- Rust structs with serde-based parsing for every variant
- Short design note in `docs/views.md` covering: shape, slots per type, `where` grammar reuse, fixed-path convention, extensibility via new `type` variants

## Out of scope

- Formal JSON Schema validation (next issue: `views-yaml-validation`)
- Rendering (`renderers` milestone — per view type)
- `where`-clause OR nesting
- Theming / styling config
