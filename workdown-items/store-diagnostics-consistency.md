---
id: store-diagnostics-consistency
type: issue
status: to_do
title: Make store-diagnostic surfacing consistent across commands
parent: polish
---

`Store::load` collects per-item resolution diagnostics (file parse errors,
broken links, missing requireds, coercion failures, and — once aggregate
rollup lands — chain conflicts and missing-aggregate values). Every
non-`init` command loads the store, but they handle these diagnostics
differently:

- `workdown validate` aggregates and reports them.
- `workdown render` (`commands/render.rs:44-46`) prints all of them as
  warnings and continues.
- `workdown query` (`commands/query.rs`) ignores them silently and just
  runs the query against whatever loaded.

A user running `workdown query` against a project with broken links or
chain conflicts gets results with no indication that something is wrong.
That's surprising, especially compared to `render`'s behaviour on the
same store.

## Scope

Decide a single policy for surfacing `store.diagnostics()` and apply it
to all commands. Likely either:

- Always warn to stderr (matches `render` today).
- Warn to stderr unless `--quiet`; on errors, optionally fail-fast with
  an opt-in flag.

Apply the chosen policy uniformly to `query`, `render`, and any future
read-only command. `validate` keeps its dedicated reporting path.

## Out of scope

- Restructuring the diagnostic split between `Store::load` and
  `validate.rs`. The current split (per-item in store, cross-cutting in
  validate) is intentional.
- Changing `validate` output.
