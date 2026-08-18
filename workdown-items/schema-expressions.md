---
id: schema-expressions
status: to_do
title: Derived field expressions
---

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
- Rules whose purpose is a specific scheduling convention rather than
  the grammar (`duration-comparison-rule` stays with `time-tracking`).
