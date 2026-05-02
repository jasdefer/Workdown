---
id: render-metric
type: issue
status: done
title: Metric renderer
parent: renderers
depends_on: [view-data-intermediate]
---

Render `MetricView` as a Markdown stat-row table written to
`views/<id>.md`. Each view contains one or more `metrics:` entries;
each entry becomes one row in the output table.

## Shape

```yaml
- id: project-stats
  type: metric
  where:
    - "type=issue"           # base filter (optional)
  metrics:
    - label: Total           # optional — auto-generated if omitted
      aggregate: count
    - label: In progress
      aggregate: count
      where: ["status=in_progress"]
    - aggregate: sum
      value: points          # auto-label "Sum of points"
      where: ["status!=done"]
    - label: Latest deadline
      aggregate: max
      value: end_date
```

Per-row `where` AND-combines with the view-level `where`. Row order
matches definition order.

## Output

```markdown
# Metrics

| Label | Value |
| --- | --- |
| Total | 12 |
| In progress | 4 |
| Sum of points | 47 |
| Latest deadline | 2026-05-15 |
```

`AggregateValue::Number` renders plainly (integer-valued floats drop
the trailing `.0`); `Date` renders as `YYYY-MM-DD`; `Duration` uses
`format_duration_seconds` (`"1d 1h"`); `None` (avg/min/max with no
valid inputs) renders as `—`.

When any row's filter matches items missing the value field, a
blockquote footer lists them per-row, mirroring the gantt renderer's
unplaced footer.

## Notes

- Aggregates supported: `count`, `sum`, `avg`, `min`, `max`
- `count` rejects `value:` per row (cross-checked in `views_check`);
  other aggregates require it
- `sum`/`avg`/`min`/`max` over a duration field returns
  `AggregateValue::Duration(i64)` so the renderer formats it as
  shorthand instead of raw seconds
- Empty `metrics: []` renders heading-only, matching the table
  renderer's behavior on empty `columns`
- Future `dashboard` view (post-v1) will compose multiple views —
  this metric renderer only handles single-view stat tables

## Acceptance

- `render_metric(&MetricData, description) -> String` — two-arg
  signature, no `item_link_base` (values aren't item references)
- Snapshot tests per aggregate (count/sum/avg/min/max, number/date/
  duration outputs, missing data, multi-row, unplaced footer)
- Output renders correctly in GitHub preview
