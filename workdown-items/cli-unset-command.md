---
id: cli-unset-command
type: issue
status: done
title: workdown unset — clear a field
parent: item-mutations
depends_on: [cli-set-command]
---

Companion to `set` that removes a field from an item's frontmatter.

```
workdown unset <id> <field>
```

## Approach

Internally a new variant on the existing `SetOperation` enum, dispatched through the same `run_set` entry point built for [[cli-set-command]]. The CLI side is its own subcommand (`workdown unset`) — separate verb, single shared core function. The work splits across two layers.

### Core: extend `operations/set.rs`

- New `SetOperation::Unset` variant alongside `Replace`.
- `run_set` body refactored into three phases so unset and the future modes (`Append`, `Remove`, `Delta`) reuse the surrounding scaffolding:
  1. **Pre-flight** — schema load, store load, id / field / `id`-key checks, file read + frontmatter split. Hard errors here never touch disk.
  2. **Compute** — per-operation dispatch decides the new frontmatter map and whether anything actually changed.
  3. **Write + reload + diff** — atomic write (skipped on no-op), reload the store, surface every diagnostic.
- New `mutation_caused_warning` computation: capture `Store::load + rules::evaluate` diagnostics *before* the write, diff against the post-write set, mark any *new* diagnostic on the target item as mutation-caused. Replaces the current narrow "coerce_value failed" check. Catches `MissingRequired` from unset, broken-link from set, and any future cascade.
- `SetError::IdNotMutable` message reworded to be verb-agnostic: `cannot modify 'id' — use ` + "`" + `workdown rename` + "`" + ` to change an item's id`. Adjust the existing set test's assertion.

### Unset semantics

| Pre-state | Effect |
|---|---|
| field present | remove key from map, write file, render `priority: high → (cleared)`, exit `0` |
| field absent | no-op write, render `priority: (already absent)`, exit `0` |
| field is `id` | hard error: `IdNotMutable` (no write) |
| field not in schema | hard error: `UnknownField` (no write) |
| field is `required: true` | save anyway, surface `MissingRequired` warning, exit non-zero |
| field is an aggregate with `error_on_missing: true` on a leaf | save anyway, rollup runs on reload, surface `AggregateMissingValue` if no descendants fill it, exit non-zero |
| unrelated diagnostics elsewhere in project | surfaced as warnings, exit `0` (pre-existing, not mutation-caused) |

The "field absent → silent success" path is idempotent. Typo'd field names still error via `UnknownField`, so the only state hidden by this path is "real schema field, already absent on this item" — which isn't a typo to surface. The contract matches POSIX `rm` and the future `PATCH /items/:id/fields/:field` endpoint.

### CLI: new `commands/unset.rs`

- New `clap` subcommand `Unset { id, field }` in `cli/mod.rs`.
- `run_unset_command(config, project_root, id, field)`:
  - No schema preload (unlike `set`, there's no value string to type-shape).
  - Build `WorkItemId`, call `run_set` with `SetOperation::Unset`.
  - Render via the new shared renderer, propagate exit code from `mutation_caused_warning`.
- New shared module `commands/mutation_output.rs` holding:
  - A mode-aware `render_mutation(id, field, &outcome)` that pattern-matches on `outcome.previous_value` / `outcome.new_value` to pick the format (`→` for replace, `(cleared)` for unset of a present field, `(already absent)` for the no-op case).
  - `format_yaml_value` moved here from `commands/set.rs` (private helper, no behavior change).
  - Designed so [[cli-set-modes]] adds the append / remove / delta formats as new arms without touching unset.
- `commands/set.rs` migrated to call the new renderer — pure code movement, no behavior change.

## Acceptance

- `workdown unset task-1 priority` removes `priority` from the file's frontmatter and prints `task-1: priority: high → (cleared)`.
- Unsetting an already-absent field prints `task-1: priority: (already absent)`, exits `0`, and leaves the file byte-for-byte unchanged.
- Unsetting a `required: true` field saves the file but emits a `MissingRequired` warning; exit code non-zero.
- Unsetting a manual aggregate on a leaf with descendants saves the file; the rollup picks up the computed value from descendants on next load.
- Unsetting a manual aggregate on a leaf with `error_on_missing: true` and no descendants emits `AggregateMissingValue`; exit non-zero.
- `workdown unset task-1 id` errors with the reworded `IdNotMutable` message; no write.
- `workdown unset task-1 nonexistent-field` errors with `UnknownField`; no write.
- `workdown unset does-not-exist priority` errors with `UnknownItem`; no write.
- Body content byte-identical after a successful unset.
- Pre-existing diagnostics on unrelated items are surfaced as warnings; they do not affect exit code on their own.

## Test plan

In `operations/set.rs`:

- `unset_removes_field_and_writes_file`
- `unset_absent_field_is_noop_and_exits_zero`
- `unset_required_field_saves_with_missing_required_warning_and_flags_mutation_caused`
- `unset_aggregate_on_leaf_with_descendants_lets_rollup_refill`
- `unset_aggregate_on_leaf_without_descendants_emits_warning_when_error_on_missing`
- `unset_id_returns_idnotmutable_with_reworded_message`
- `unset_unknown_field_errors_without_writing`
- `unset_unknown_item_errors_without_writing`
- `unset_preserves_body_byte_for_byte`
- `unset_explicit_id_in_frontmatter_is_preserved`
- `unset_does_not_flip_mutation_caused_warning_for_unrelated_existing_warnings`

Updates to existing set tests for the diff-based `mutation_caused_warning`:

- `set_with_broken_link_now_flags_mutation_caused_warning` (covers a current gap)
- `setting_id_returns_error_with_rename_hint` — update for the reworded message

`frontmatter_io.rs` needs no changes; `build_frontmatter_yaml` already handles a map missing the unset key correctly.

## Step ordering

1. Refactor `run_set` body into the three phases above. No behavior change. All existing set tests stay green.
2. Implement pre / post diagnostic diff for `mutation_caused_warning`. Adjust existing set tests for the broader signal.
3. Add `SetOperation::Unset` variant + dispatch in the compute phase. Core tests for unset.
4. Reword `SetError::IdNotMutable` message; update the test assertion.
5. Extract `commands/mutation_output.rs` from `commands/set.rs`. Migrate `set` to call it.
6. Add `commands/unset.rs` + `Unset` clap variant + `main.rs` wiring.

Each step compiles and runs the suite green before the next.

## Out of scope (filed under [[item-mutations]], not this issue)

- `--append`, `--remove`, `--delta` modes → [[cli-set-modes]]
- A `--quiet` / `--only-new-warnings` flag on mutation commands — once the diff lands here it's a tiny follow-up
- Dropping the CLI's schema preload on `set` — not worth the structural complication right now
- Renaming `SetOperation` / `run_set` — keep current names
