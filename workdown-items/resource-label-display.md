---
id: resource-label-display
status: removed
title: Show a resource entry's label where views display its id
parent: polish
depends_on: [resource-option-lists]
---

> **Removed 2026-08-03, not worth the cost.** Kept as a record of the
> question, not as work to do.
>
> The gap is cosmetic: editors already show `name ?? id`, the stored ids
> are short and readable, and nothing breaks by showing them. Closing it
> properly would touch five wire structs (board columns, bar-chart bars,
> heatmap axes, gantt and line-chart groups all conflate grouping key
> and rendered label, and board drag-drop writes the key back), plus a
> label sidecar for typed table/tree cells and both CLI renderers — the
> CLI shares `ViewData` with the server but has no schema or resources
> in scope, so a UI-only shortcut would split `render` and `serve`
> behavior. Revisit if dogfooding makes raw ids actually hurt.
>
> One factual correction for that future reader: the workload example
> below is wrong — workload buckets are dates, it has no field-value
> axis to relabel. The item detail also needs nothing: it is purely an
> editor and its picker already shows the label.

[[resource-option-lists]] made the editors offer `name ?? id` — you pick
"Alice Smith" and the item stores `alice`. Every read-side surface still
shows the stored id: a table cell renders `alice`, a board card's field
row renders `alice`, a workload axis groups by `alice`. So the same
person reads one way while you edit and another way everywhere else.

## The question

Whether a resource-backed value should render as its label, and if so
where. Filed rather than folded into [[resource-option-lists]] because
it is a display-role question, not a validation one: the answer
interacts with `defaults.display` and with how a view names the value
it groups by.

Worth deciding together:

- Which surfaces relabel — cells and card field rows are the obvious
  ones; grouping keys (board columns, chart axes) are less obvious,
  since the label is not the value being grouped and two entries could
  share a `name`.
- Whether it is automatic or opt-in per view.
- What "the label" means once resource entries can declare more than
  `name` — the `resource-option-lists` scope note already deferred
  display-field customization ("which attribute is the label") to here.

## Out of scope

- Editing `resources.yaml` from the UI — stays a text-editor job.
