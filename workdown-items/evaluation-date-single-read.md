---
status: to_do
parent: misc-work
title: One clock read per invocation, writes included
depends_on: [evaluation-time-now]
---

## In plain words

Some commands look at the clock twice while doing a single job, so the
two halves of the work can disagree about what day it is.
**Example:** run `workdown set` at 23:59:59 — the computed
`days_remaining` fields are evaluated on Monday, but by the time the
date rules run it is Tuesday, so one command judges the same item
against two different "today"s. Related: `workdown serve --as-of
2026-01-01` is a time machine for *reading* — but *saving* an item
still checks rules against the real date, so the same server can show
no warning when you open an item and then warn when you save it
unchanged. The fix is to look at the clock once per command and pass
that one date everywhere — and to decide, explicitly, what the time
machine should do about writes.

ADR-010 promises the evaluation date is "resolved exactly once per
load". Read commands honor it (`load_project` resolves once and threads
the date to both store load and rule evaluation), but two write-side
holes remain:

- **Mutation commands read the clock twice.** `add`, `set`, `rename`,
  and `body` call `Store::load_with_resources` (which resolves `$today`
  internally) and then `rules::evaluate` (which resolves it again). A
  command straddling midnight evaluates computed fields on one date and
  rules on another.
- **`serve --as-of` pins reads but not writes.** GET endpoints evaluate
  at the pinned date; POST paths (`run_add`/`run_set`) evaluate rules at
  the real current date, so the same server can show no warning on read
  and return an overdue save-with-warning on write of the same item.

## Scope

- Thread one `current_local_date()` (or the pinned override) through
  the operation entry points, mirroring `load_project`.
- Decide the `serve --as-of` write semantics explicitly — pin
  everything, or document that `--as-of` is read-only — and record the
  decision here. Either way `AppState.evaluation_date_override`'s doc
  should state it.
