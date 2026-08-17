---
id: store-diagnostics-consistency
status: done
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

## Decisions taken (2026-07-30)

1. **`query` loads via `load_project`** and surfaces the full project
   diagnostics, same as `render`. Consistency-by-construction: one
   loader, one surfacing helper; query's hand-rolled schema/resources/
   store loading is deleted. View diagnostics appearing in query output
   is accepted — diagnostics are empty in a healthy repo.
2. **Severity-faithful printing.** Error-severity diagnostics print
   with the error glyph, warnings with the warning glyph, instead of
   everything-as-warning. Both still continue.
3. **Warn-and-continue, exit 0.** Each command has one job; `validate`
   is the gate and exits non-zero on errors. No `--strict` flag until a
   concrete need appears.
4. **`--quiet` untouched.** It keeps affecting only the `tracing`
   level. Wiring it into `output::*` would change every command's
   success/info lines — separate question, if ever.
5. **Shared helper in `cli::output`** (`surface_diagnostics`), called
   by `render` and `query`; future read-only commands inherit the
   policy by calling it.
6. **No `$today` reproducibility notice in `query`.** The render notice
   exists because committed output changing without a commit is
   surprising; query output is ephemeral and date-dependence is
   expected there.

## Out of scope

- Restructuring the diagnostic split between `Store::load` and
  `validate.rs`. The current split (per-item in store, cross-cutting in
  validate) is intentional.
- Changing `validate` output.
- Mutation commands (`set`, `unset`, `move`, `body`, `rename`, `add`):
  their post-write policy (surface reload warnings, fail only when the
  mutation caused one) is deliberate and stays.
- `serve` — surfaces diagnostics through the web banner.
