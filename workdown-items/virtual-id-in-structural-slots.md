---
id: virtual-id-in-structural-slots
type: issue
status: to_do
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
