---
status: done
tags: [bug]
parent: schema-expressions
title: Integer precision and NaN in comparison evaluation
depends_on: [conditional-field-value]
---

## In plain words

Two rare-value problems in schema expressions (`compute:` / `when:`
conditions). First: comparing two very large whole numbers silently
converts them to a less precise decimal format first — like rounding
two prices to whole euros before deciding which is higher.
**Example:** with `a: 9007199254740993` and `b: 9007199254740992`
(they differ by exactly 1), the expression `a == b` evaluates to
*true*. Second: the special "not a number" value that YAML spells
`.nan` slips through comparisons without any warning. **Example:**
an item with `weight: .nan` — a condition `weight > 1` silently
produces nothing, while the arithmetic `weight + 1` correctly prints
a warning. Same broken input, inconsistent honesty.

Two corner cases in `apply_comparison`
(`crates/core/src/expression/evaluate.rs`), both in the float-conversion
corner:

- **Large integers lose precision.** All number pairs are compared via
  `as_float(a).partial_cmp(&as_float(b))`, so two integer fields
  holding values above 2^53 that differ by 1 compare as equal — while
  `apply` (arithmetic) carefully special-cases `(Integer, Integer)`
  with checked `i64` math. An `(Integer, Integer)` arm using `a.cmp(b)`
  closes it.
- **NaN turns a type-correct comparison into a silent skip.** Float
  coercion accepts YAML `.nan` (NaN also bypasses `min`/`max` bounds —
  comparisons with NaN are false). `partial_cmp` then yields `None`,
  which falls through to `EvaluateError::InvalidOperation` — mapped to
  a *silent* skip in `store/compute.rs` on the assumption the type
  check already reported it. But the type check accepted this
  expression; contrast `weight + 1.0` on NaN, which yields `NotFinite`
  and a visible warning. Either reject NaN at float coercion or map the
  `None` ordering to `NotFinite`.

## Scope

- Fix both corners; add tests for large-integer equality/ordering and a
  NaN operand.
- While in there: a table-driven test over
  `ALL_TYPES × ALL_TYPES × operators` that locks the comparison algebra
  the way `assert_algebra` locks the arithmetic one, so
  `comparison_is_defined` and `apply_comparison` cannot drift apart.

## Decisions taken (2026-09-09)

1. **Integer pairs compare exactly.** `apply_comparison` gets an
   `(Integer, Integer)` arm using `i64::cmp`, mirroring the arithmetic
   side; mixed and float pairs still go through `f64`.
2. **NaN is a value failure, not a schema one.** When a float pairing
   yields no ordering, the evaluator returns `NotFinite` instead of
   falling through to `InvalidOperation`. Both `compute` and `when`
   already map `NotFinite` to an item warning, so no caller changed.
   Rejecting NaN at coercion time was the alternative; it would have
   turned the field into a "missing input" and kept the skip silent.
3. **Drift guard in the evaluator's tests.** `comparison_is_defined`
   and `ALL_TYPES` became `pub(super)` so a table-driven test over every
   type pair and operator asserts the runtime answers exactly the
   pairings the type checker defines, like `assert_algebra` does for
   arithmetic.
4. **The warning is pinned at the project loader.** No integration test
   asserted that a runtime evaluation failure reaches the user at all,
   so the changelog claim rested on unit tests alone.
   `crates/core/tests/computed_fields.rs` now loads a project with `.nan`
   in a compared float field and asserts exactly one item warning naming
   the field and branch, per the "each distinct error the user can hit, once" rule.

Not touched: a `.nan` float still passes `min`/`max` bounds in
validation. It is a validation-layer question, noted here and left
until someone meets it.
