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
   field picker and an asc/desc toggle, with a button for further keys.
   The keys cross the wire structured (`SortSpec`, generated TypeScript),
   beside the definition like the filter's clauses, and core serializes
   them into the slot — the key grammar has one parser and one writer,
   both in Rust. (Revised in review from "serialized into the grammar by
   the form", which would have been a second parser to keep in step.)
7. **`workdown query` comes along:** `--sort` rejects an unknown field
   or direction the same way a view's `sort:` is rejected, asking the
   same core functions. `--where` now does the same for an unknown field
   instead of matching nothing, so the two flags and the two slots share
   one rule.
8. **Id order everywhere:** the sorter breaks every remaining tie by id,
   including when there are no keys at all, so bare `workdown query`
   lists by id rather than by file name. One tiebreaker, implemented
   once, instead of a special case per surface.
9. **Which kinds show the order is a method, not prose:**
   `ViewKind::lists_in_view_order`, an exhaustive match the caption
   reads, and a row in the add-a-view-kind checklist — so a new kind has
   to answer the question.
10. **A sort key the grammar rejects is dropped from the edit-form
    seed** (`ViewDefinition::from_view`), not carried as a raw row the
    way an unparsable filter clause is. Such a view is already an error
    at validation and renders nothing; saving the form without the key
    is the repair. Accepted in review as a deliberate asymmetry: the
    grammar is two dropdowns wide, and a raw escape hatch would cost
    more editor than the mistake it would show.
11. **The footer re-sort is unconditional.** Every extractor restores
    the view order on its unplaced list via `ViewOrder`, including the
    single-pass extractors whose list already is in order. The call
    states the invariant rather than depending on how many passes an
    extractor happens to make, so a later second pass cannot break the
    footer order silently.
