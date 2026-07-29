---
id: expression-predicates
type: issue
status: to_do
title: Comparisons, equality and booleans in the expression grammar
parent: polish
effort: "12h"
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

## Decisions to make

- **Boolean-valued `compute:`.** Adding `Boolean` to the expression types means
  `compute: effort > duration` becomes writable on a `boolean` field. That is a
  useful feature and arguably free, but it is a new capability arriving as a
  side effect. Decide whether to embrace it (and test it) or forbid it until
  someone asks.
- **String literal syntax.** Bare words (`status == done`) are terser and match
  how choice values are written everywhere else in the schema; quotes
  (`status == "done"`) are unambiguous and leave room for values with spaces.
  Bare-word means the lexer must distinguish a value from a field reference by
  position, which is doable but subtle. Weigh honestly.
- **Combining predicates.** Do `and` / `or` / `not` belong here? The motivating
  cases in [[conditional-field-value]] need none of them, and a `when:` list
  already gives an implicit "or" across branches. Leaving them out keeps this
  issue small; the cost is that "done *and* overdue" needs two branches. Prefer
  leaving out unless it makes the acceptance cases awkward.
- **Mixed-type comparison.** Comparing a duration to an integer, or a date to a
  number — the arithmetic algebra has rules for these already. Confirm the
  comparison rules match rather than inventing a second set. Note the open
  question in [[duration-comparison-rule]] about durations whose units differ:
  do not solve it twice.

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
