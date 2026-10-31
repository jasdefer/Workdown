---
id: view-sort-slot
title: 'Per-view item order: a `sort:` slot on views'
status: in_progress
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

## Decisions taken (2026-10-31)

1. **Where the order applies:** board cards within a column, table rows,
   tree roots and siblings at every level, graph nodes, and the
   unplaced-items footer of every kind. Gantt bars keep section → start →
   id and treemap nodes keep size-descending; `sort:` reaches only their
   footer. Board column order is untouched.
2. **Checking:** an unknown field is an error that skips the view, like
   every other field reference in `views.yaml`. A direction other than
   `asc`/`desc` is an error too. Existence-only: every field type sorts,
   and the virtual `id` is accepted.
3. **Syntax:** a single string or a list of strings, `field[:asc|:desc]`,
   the `workdown query --sort` grammar. Written back as a string for one
   key and a list for several.
4. **Where it happens:** once, in the filter stage — items arrive in
   final order and each kind preserves it instead of re-sorting by id;
   the tree applies the same comparator to the children it discovers.
   An absent `sort:` is the id order of today.
5. **No ladder:** the view slot only. No `defaults.sort` in config.yaml,
   no session override; the web table's header click stays session-local
   and sits above the configured order, which becomes its secondary key.
6. **Web UI:** a cross-kind *Sort* control in the view form — per key a
   field picker and an asc/desc toggle, with a button for further keys —
   serialized into the grammar.
7. **`workdown query --sort` comes along:** the CLI rejects an unknown
   field or direction the same way, so the one grammar behaves the same
   on both surfaces.
