---
id: view-sort-slot
title: 'Per-view item order: a `sort:` slot on views'
status: to_do
parent: misc-work
---
Every item-presenting view orders its items by id: board columns, table rows, tree siblings and the unplaced footer are all alphabetical, and nothing in `views.yaml` can change that. A project that wants to see what was created most recently — the item it just added and did not assign to anyone — has no way to express it. The table view in the web UI lets a reader click a column header, but that state is session-local and the rendered Markdown stays id-ordered.

Requested from a user project: [henkell-freixenet-polska-workdown#1](https://github.com/planningio/henkell-freixenet-polska-workdown/issues/1).

## Requirement

A view may declare an optional `sort:` slot that fixes the order of its items, with the same grammar as `workdown query --sort`: a field name, optionally suffixed with `:asc` or `:desc`, repeatable for a multi-key sort.

```yaml
- id: recent
  type: table
  sort: created:desc
- id: board
  type: board
  field: status
  sort: [priority:desc, created]
```

- The order applies wherever a kind lists items: board columns, table rows, tree roots and siblings, graph node order, the unplaced-items footer of the aggregate kinds. Id stays the final tiebreaker, so an absent `sort:` renders exactly as today.
- Resolved in core `view_data`, so `render` and `serve` agree (ADR-006: ViewData owns order).
- `views_check` rejects a sort field the schema does not declare, like it does for display roles.
- `views.schema.json`, the view editor in the web UI and `docs/views.md` learn the slot.
