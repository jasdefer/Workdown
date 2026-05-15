---
id: cli-set-modes
type: issue
status: done
title: workdown set — type-aware modes (append, remove, delta)
parent: item-mutations
depends_on: [cli-set-command]
---

Adds mode flags to `workdown set` so each field type gets natural operations beyond plain replace.

```
workdown set <id> <field> --append <value>
workdown set <id> <field> --remove <value>
workdown set <id> <field> --delta <value>
```

## Initial idea

Each mode is mutually exclusive with a bare positional `<value>`. Validity depends on the field type — invalid combinations error cleanly.

### Initial dispatch table

| Field type | Valid modes (beyond replace and unset) |
|---|---|
| string | — |
| choice | — |
| multichoice | `--append`, `--remove` |
| integer | `--delta` (e.g. `+3`, `-1`) |
| float | `--delta` |
| date | `--delta` (accepts a duration like `+1w`, `-3d`) |
| duration | `--delta` |
| boolean | (consider `--toggle` later) |
| list | `--append`, `--remove` |
| link | — (single-valued; use replace) |
| links | `--append`, `--remove` |

`--append` and `--remove` accept either a single value or a comma-separated list. Idempotency: appending an already-present element is a silent no-op (or surfaces an info message — open question); removing an absent element is the same.

### Output mirrors the mode

Renderer formats per the table in [[item-mutations]]: arrow for replace, before-plus-input-equals-after for append/remove/delta, cleared for unset.

### Implementation notes

- The mode dispatch lives in the core layer: `run_set` (or a thin sibling) accepts an enum-shaped operation. The CLI parses flags into that enum. The server later builds the same enum from a JSON body.
- Mode-type validity is one table, checked once. Invalid combinations don't reach the file system.
- Delta arithmetic for `date` reuses the existing duration parser ([[duration-field-type]]).
- For list/links/multichoice append/remove, order matters: `--append` always appends to the end; `--remove` removes the first matching element (or all? — open question, lean toward all for set-like semantics).

## Acceptance

- `workdown set task-1 tags --append qa` appends to the list.
- `workdown set task-1 tags --remove qa` removes the element if present.
- `workdown set task-1 points --delta +3` adds 3 to the integer field.
- `workdown set task-1 due_date --delta +1w` shifts the date by a week.
- `--append` on a `date` field errors with "append not valid for date fields."
- Output line shows the operation (arrow vs arithmetic vs list before/after).

## Open questions to think about during implementation

- Is `--toggle` worth shipping in this issue or deferred? Boolean fields are rare in workdown today.
- For multi-element appends: `--append a,b,c` vs `--append a --append b --append c`. Comma-separated keeps the positional consistency from `set`; repeated flag is more Unix-flavored. Decide together with list-replace syntax.
- Should removing the last element of a required list emit a stronger warning than the usual missing-required path?
- `--delta` on `date` with a negative duration is "shift backwards" — confirm the duration parser supports signed durations or whether we need a `--subtract` companion.
