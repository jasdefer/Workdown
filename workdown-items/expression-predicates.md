---
id: expression-predicates
status: done
title: Comparisons, equality and booleans in the expression grammar
parent: polish
---

The compute expression grammar is arithmetic and nothing else. Verified
against the source rather than assumed:

- `expression/ast.rs` — `Expression` is `FieldReference`, `ConstantReference`,
  `IntegerLiteral`, `FloatLiteral`, `Negate`, `Binary`. `BinaryOperator` is
  `Add`, `Subtract`, `Multiply`, `Divide`.
- `expression/typecheck.rs` — `ExpressionType` is `Integer`, `Float`, `Date`,
  `Duration`. Described in its own doc comment as "a deliberate subset of the
  field type system: only types with meaningful arithmetic participate."

So there is no way to write a condition: no comparison operators, no boolean
type, and no string literal to compare a `choice` field against. This issue
adds exactly that, and nothing that isn't needed for it.

## Scope

- Comparison operators over the types that already order: `<`, `<=`, `>`, `>=`
  on numbers, dates and durations.
- Equality and inequality, extended to the types where equality is the only
  meaningful comparison — `choice`, `string`, `boolean`, and `color` (compared
  on resolved hex, matching how the query evaluator already handles it).
- A string literal form, so `status == done` parses. Quoting rules to decide.
- `Boolean` in `ExpressionType`, produced by comparisons and consumed by the
  conditional construct in [[conditional-field-value]]. The type-checker must
  reject an arithmetic operator applied to a boolean and a boolean assigned to
  a non-boolean field, with the same quality of message the algebra has now.
- Whatever `compute_check.rs` needs so a bad predicate surfaces once against
  `schema.yaml`, not per item — the existing contract for compute configs.

## Decisions taken (2026-07-30)

The four open decisions, resolved:

- **String literals are quoted** (`status == "done"`), never bare. Either
  side of a comparison can be a field reference, so position cannot
  disambiguate a bare word — and treating unknown words as literals would
  turn a typo'd field name into a silently-false condition instead of an
  unknown-field error. `true` / `false` are reserved words.
- **No `and` / `or` / `not`.** Every comparison has a complement operator,
  so first-match ordering in `when:` expresses conjunctions (bail out on
  complements first) and disjunctions (two branches, same `then`).
  Combinators are ergonomics, not expressiveness; addable later without
  breakage. One comparison per expression — `a < b < c` is a parse error.
- **Boolean-valued `compute:` is allowed** — forbidding it would leave a
  result type no field may hold; `is_overdue: end_date < $today` is the
  natural dogfood.
- **Strict type pairings, reused from the arithmetic.** Ordering exists
  where types order among themselves (numbers cross-promote, date vs
  date, duration vs duration); `duration < 5` is an error, not a guess.
  Mixed units need no convention here: durations are canonical seconds
  by evaluation time, so `3h < 1w` orders unambiguously. Equality additionally covers text (string/choice),
  boolean, and color (resolved hex, so `tint == "red"` matches the name
  or the hex).

## Acceptance

- `status == done`, `end_date > start_date` and `effort <= duration` all
  type-check against a schema that declares those fields, and all fail to
  type-check when a field is missing or the comparison is meaningless (a
  `links` field, say).
- A comparison used where a number is expected is a load-time error naming the
  field and the expression, in the style `compute_check` already produces.
- Arithmetic behaviour is bit-for-bit unchanged: every existing
  `computed-fields` test passes untouched.

## Out of scope

- The `when:` config that consumes these predicates — [[conditional-field-value]].
- Referencing the current date — [[evaluation-time-now]].
- Cross-item references. Predicates read the same item, exactly as arithmetic
  does today.
