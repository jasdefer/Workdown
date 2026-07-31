---
id: resource-label-display
type: issue
status: to_do
title: Show a resource entry's label where views display its id
parent: polish
depends_on: [resource-option-lists]
effort: "3h"
---

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
