---
id: pull-copy-type-conversion
title: Copy into a field of a different type (`pull` over a `link` with conversion)
status: to_do
parent: schema-expressions
depends_on: [pull-copy-over-link]
tags: [enhancement]
---

## In plain words

[[pull-copy-over-link]] lets a field copy its value from the item a
`link` points at, as long as source and target have the same type
(plus the integer-into-float widening the reductions already allow).
This item would let the copy cross types where the conversion is
well defined: a `choice` into a `string`, an `integer` into a
`string`, a `float` into an `integer`.

Deferred from [[pull-copy-over-link]] on 2026-11-05 because every
conversion is a small design on its own and the reporter of
[workdown#62](https://github.com/planningio/workdown/issues/62) needs
same-type copies only. Pick this up when a project actually needs a
cross-type copy; which conversion it needs decides the first row.

## Open questions

1. **Lossy conversions.** Float into integer: round, truncate, or
   reject a non-whole value? `compute` has `round:` for the same
   problem; the answer should match it.
2. **Text targets.** Anything into `string` needs one canonical text
   per source type: `2026-11-05` or the display format for a date,
   `2d` or `16h` for a duration, `true` or `yes` for a boolean. The
   lossless ones (`choice`, `integer`) need no such decision and
   could go first.
3. **The target's own rules.** A copied value lands in a field that
   may carry `pattern`, `min`/`max` or `values`. Either the config
   check proves the copy always fits, or a violation surfaces as an
   item diagnostic on every descendant.
4. **Symmetry with reductions.** Over `links` the reductions convert
   nothing but integer into float. A copy that converts more makes
   the two halves of `pull` behave differently; decide whether that
   is acceptable or whether the reductions should widen the same way.

Touches: the pull config check in `crates/core/src/compute_check.rs`,
the copy branch of the pull evaluation in `crates/core/src/store/derive.rs`,
the `pull_sources_by_type` table served to the schema editor, and the
docs for `pull`.
