---
id: virtual-id-in-structural-slots
type: issue
status: done
title: Reject the virtual `id` in structural slots that read item fields
parent: polish
---

`views_check`'s slot checker accepts the virtual `id` in every slot
(early return before the existence and type checks). That is correct
for the text display roles — extraction resolves `id` specially — but
the *structural* slots read `item.fields`, where `id` never appears, so
a view like `field: id` on a board, `x: id` on a heatmap, or
`group_by: id` on a bar chart validates cleanly and is silently dead at
extraction (every item lands in unplaced / no bucket).

The `color:` display role already rejects `id` with a type-mismatch
diagnostic ("field 'id' has type string, expected color") since
[[view-display-config]]'s cleanup pass; this item extends the same
treatment to the structural slots.

## What we want

- `check_slot` (and the existence-only structural uses like heatmap
  `x`/`y`, gantt `group`, bar chart `group_by`) reject `id` with a
  diagnostic instead of silently accepting a dead config.
- Text display roles (`display.title`, `display.subtitle`,
  `display.fields`) keep accepting `id` — extraction handles it.
- `docs/views.md`'s "the virtual `id` field is always accepted" note
  updated to name the exception.

## Why not trivial

The distinction is per slot, not per type-restriction: some
existence-only slots read `item.fields` (heatmap `x`) while others are
display-resolved (`display.fields`). The checker needs to know which
slots are virtual-`id`-aware rather than inferring it from whether a
type list was passed.

## Decisions taken (2026-07-30)

1. The "why not trivial" concern above dissolved on inspection: display
   roles are checked in `display_check.rs`, a separate path — every
   `check_slot` caller in `views_check.rs` is already structural. The
   fix removes the virtual-id early return there and in the aggregate
   `value` helpers.
2. **New diagnostic kinds** `ViewVirtualIdNotAllowed { view_id, slot }`
   and `ViewMetricRowVirtualIdNotAllowed { view_id, metric_index }`
   instead of reusing existing ones: "unknown field 'id'" would be a
   lie, and the type-mismatch phrasing self-contradicts on slots that
   accept strings ("'id' has type string, expected … or string").
3. **Error severity**, consistent with every other slot diagnostic —
   the view is unrenderable-by-construction and render skips it.
4. The link-walk slots (`graph.field`, `group_by`, `after`,
   `root_link`, `depth_link`) already rejected `id` but as a misleading
   "unknown field" — they now emit the new diagnostic too.
5. **`where:` clauses keep accepting `id`** — filtering by id is
   legitimate. That evaluation currently resolves nothing (discovered
   while verifying this item) is filed as
   [[virtual-id-in-query-eval]].
