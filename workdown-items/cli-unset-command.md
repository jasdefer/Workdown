---
id: cli-unset-command
type: issue
status: to_do
title: workdown unset — clear a field
parent: item-mutations
depends_on: [cli-set-command]
---

Companion to `set` that removes a field from an item's frontmatter.

```
workdown unset <id> <field>
```

## Initial idea

- Two positional arguments. The field key is removed entirely from frontmatter (rather than written as an explicit `null`) so the file stays clean.
- Internally forwards to the same core function family as [[cli-set-command]] — the result has `new_value: None`.
- All cross-cutting conventions from [[item-mutations]] apply: save-with-warning on schema violation (clearing a required field still saves, surfaces the missing-required warning, exits non-zero), always show all warnings, whole-store load.
- Setting `field == "id"` is rejected — `id` is not a regular field. Clearing it is meaningless.
- Output: `task-1: priority: high → (cleared)` (or `(absent)` if it was already absent — should that be an error or a no-op? Open question).

## Acceptance

- `workdown unset task-1 priority` removes `priority` from the file's frontmatter.
- Clearing a required field still saves but emits a `missing-required` warning and exits non-zero.
- Unknown item id / unknown field name errors cleanly.
- Body content unchanged.

## Open questions to think about during implementation

- If the field is already absent: silent no-op, info message, or error? My current lean: silent success, since the post-state is what the user asked for. But it could mask typos in the field name — though "unknown field name" already covers that path.
- Should `unset` reject fields with `required: true` outright (refuse to save)? Or always save-with-warning? The milestone convention says save-with-warning; this issue should follow that unless there's a strong reason not to.
- Aggregate fields: clearing a manual aggregate on a leaf should let the rollup pass refill from descendants on next load. Verify this works.
