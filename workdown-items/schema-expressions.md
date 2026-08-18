---
id: schema-expressions
status: to_do
title: Derived field expressions
---

## In plain words

The home for the machinery that lets a field work out its own value
instead of having it typed in by hand.

A project can already say things like "this field is that field
multiplied by two" or "if the status is done, colour it green". This
item collects everything that calculation engine still needs, in one
place, instead of leaving the leftovers scattered across unrelated
milestones. **Example:** its children cover writing a condition that
combines two tests in one line, a shorter way to write simple
value-to-value lookup tables, and fixing two cases where comparing
unusual numbers gives the wrong answer. Command-level plumbing around
evaluation, and rules that exist for a specific scheduling convention,
belong elsewhere.

The engine that lets a field's value be derived rather than typed: the
expression grammar itself, the `compute:` / `when:` / `pull:` /
`aggregate:` mechanisms that evaluate against it, and the type checking
that keeps them honest at load time.

The shipped pieces of this engine were delivered inside whatever release
batch they happened to land in — `conditional-field-value` and
`expression-predicates` under `polish`, `aggregate-rollup` under
`renderers`, `computed-fields` under `time-tracking` — which left the
follow-on work with no home and floating at the root. This item is that
home: what the engine still needs, in one place, whatever release it
eventually ships in.

## Shape of the work

- **Grammar reach** — logical combinators (`and` / `or` / `not`), and
  `then:` values that go beyond literals to fields, `$today` and full
  expressions.
- **Ergonomic shorthands** — `map:` as a lookup table over the `when:`
  evaluator, rather than a hand-written condition per case.
- **Evaluation correctness** — the corner cases the current comparison
  code gets wrong (integer precision, NaN).

## Out of scope

- Command plumbing around evaluation that isn't the evaluator itself
  (clock reads per invocation live in `misc-work`).
