---
id: virtual-id-in-query-eval
type: issue
status: to_do
title: Resolve the virtual `id` in query evaluation and sorting
parent: polish
---

Filtering and sorting by `id` silently does nothing. Verified against a
scratch project containing `alpha.md`:

- `workdown query --where "id=alpha"` matches **no** items —
  `eval_comparison` (`query/eval.rs`) resolves `Local` references via
  `item.fields.get(name)`, and the virtual `id` never appears in the
  fields map.
- `--sort id` only *appears* to work ascending because the deterministic
  id tie-breaker in `sort_items` (`query/sort.rs`) kicks in after every
  spec compares missing-vs-missing; `--sort id:desc` does not reverse.

The same evaluation path backs a view's `where:` clauses, so
`where: [id=…]` in `views.yaml` is equally dead. `views_check`
deliberately keeps accepting `id` in where clauses (see
[[virtual-id-in-structural-slots]]) because filtering by id is
legitimate — this item makes it actually work.

## Scope

- `query/eval.rs`: resolve `FieldReference::Local("id")` to the item's
  id (string comparison semantics).
- `query/sort.rs`: resolve the sort key `id` to the item's id.
- Column selection already special-cases `id` (`query/engine.rs`) —
  unchanged.

## Out of scope

- `id` on the right-hand side of a relation path (`parent.id`) — check
  whether `resolve_field_ref` already handles it; extend here if not.
