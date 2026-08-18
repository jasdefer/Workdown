---
id: evaluation-time-now
status: done
title: Resolve the current date at evaluation time, reproducibly
parent: polish
---

`$today` exists, but only as a **default generator**: `generators.rs` resolves
it via `chrono::Local::now()` while `workdown add` runs and stamps the literal
date into the file. Nothing can reference "now" later. So no rule and no
computed field can say anything about the present — not "this end date has
passed", not "this `to_do` was meant to start last week".

This issue introduces *now* as something evaluation can read, and settles what
that does to reproducibility. It is the shared prerequisite for
[[conditional-field-value]] and [[rules-current-date-reference]], and the
cheapest of the three, so it goes first.

## The reproducibility problem

This is the part that needs a decision, not just code.

`workdown render` writes committed files under `views/`. If any derived value
depends on today's date, re-rendering tomorrow produces a diff **with no item
edits** — an item silently crosses its `end_date` and its bar changes colour.
Rendered output stops being a function of the repository and becomes a function
of the repository *and the calendar*. Concretely: a CI job that renders and
checks for a clean working tree starts failing on days nobody touched the repo,
and `git blame` on a view file stops meaning anything.

That is not an argument against the feature — the board genuinely did change,
and surfacing it is the whole point. It is an argument for being able to pin
the clock. ADR-001's snapshot stance already says validation judges current
state; this extends the same idea to "current" including the date, and makes
the date an explicit input rather than an ambient one.

## Scope

- A reference resolvable during evaluation, for both the expression grammar
  and the rule engine. Naming and spelling to be decided below.
- A project-wide override so a given commit renders and validates identically
  on any day — a CLI flag on the commands that evaluate (`render`, `validate`,
  `query`, `serve`), defaulting to the real current date.
- Whichever surfaces consume it are wired in by the dependent issues; this one
  delivers the primitive and the override.
- Likely an ADR: this is the first value entering evaluation from outside the
  repository, which is a departure from "the repo is the single source of
  truth" worth recording explicitly.

## Decisions to make

- **Spelling.** Reuse `$today` for both the add-time generator and the
  evaluation-time reference, or give the evaluation-time one a distinct name?
  Reuse reads better and users will expect it; the cost is one token with two
  resolution moments, which is a genuine source of confusion when explaining
  the model. Consider whether `$now` should exist alongside for a future
  timestamp type, or be reserved.
- **Type.** A date, presumably — the field type system has `date` and no
  timestamp type. Confirm that a date is enough for the cases in
  [[conditional-field-value]] and [[rules-current-date-reference]] before
  committing.
- **Override shape.** A flag (`--as-of 2026-07-29`), an environment variable,
  a config key, or more than one? A flag is explicit and scriptable; an env var
  is easier to set once for a CI job. Consider whether a config key would be
  actively harmful — a committed pinned date that silently freezes everyone's
  view is worse than no override.
- **Time zone.** `Local::now()` today, which means the same commit can render
  differently for two collaborators in different zones. Is local right, or
  should evaluation be UTC, or configurable? Local matches user intuition about
  "today"; UTC makes collaborators agree.
- **Whether the render pipeline should warn.** When output depends on the
  clock, it may be worth saying so in the render summary, so a surprising diff
  has an explanation attached.

## It has a consumer before the other issues land

Worth knowing before starting: this does not need
[[expression-predicates]] to be demonstrable. `typecheck.rs` already types
`(Date, Date)` subtraction as a `Duration`, so the moment `$today` resolves at
evaluation time, this type-checks under today's arithmetic:

```yaml
days_remaining:
  type: duration
  compute: end_date - $today
```

That makes the primitive independently shippable and independently useful —
and it is the obvious dogfood for this repo's own schema. Test against it
rather than waiting for a predicate to exist.

## Acceptance

- A rule and a computed field can both reference the current date, and both see
  the same value in one run.
- `compute: end_date - $today` on a `duration` field resolves per item, with no
  grammar change.
- Two renders of the same commit with the same pinned date produce byte
  identical output.
- With no override, the value is the real current date, and the add-time
  `$today` generator keeps behaving exactly as it does now.

## Out of scope

- Any specific rule or condition that uses it — see the dependent issues.
- Caching. If reading the date turns out to be hot, that is a later problem.
