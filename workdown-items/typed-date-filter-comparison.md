---
id: typed-date-filter-comparison
status: done
title: Compare dates in filters as dates, not as text
parent: misc-work
---

## In plain words

The filter engine compares date fields as text: `due_date > 2026-03-01`
works only because the ISO format happens to sort alphabetically in
date order. A malformed right-hand side (`2026-3-1`, `03/01/2026`)
silently compares as arbitrary text instead of failing or being parsed.

Give dates a typed evaluator (the ordered-comparison helper from
[[query-value-consolidation]] fits directly): parse the right-hand side
as a date, compare real dates, and let an unparseable right-hand side
never match — mirroring how unparseable numbers behave for numeric
fields today.

This is a deliberate semantic change (malformed inputs stop matching
things by textual accident), which is why it was excluded from
[[query-value-consolidation]] — see its decision 3.

## Decisions taken (2026-09-09)

1. **A malformed date matches nothing; it is not a query error.** The
   evaluator already answers every typed field this way (integer,
   float, duration, color, boolean), and errors are reserved for
   problems that are wrong regardless of any item, such as dot notation
   on a non-relation. Telling the user is the operand checker's job
   (`crates/core/src/where_check.rs`), which already warns on an
   unparseable date operand under `=`, `!=` and the ordering operators
   — and claimed, in its module doc, to be derived from what the
   evaluator does. Dates were the one type where that claim was false:
   the checker said "never matches" while the evaluator matched by
   textual accident. This item makes the evaluator honour the checker.
   Rejected: a query error — a third category no other type has, which
   for consistency would have to spread to numbers, durations, colours
   and booleans and would make the warning layer redundant.
2. **`contains` and `matches` stay textual on dates.** Only the six
   comparison operators parse the operand. `due_date ~ 2026-03` is the
   one way a filter says "in March 2026"; the views display exactly
   that text, and the operand checker already treats `contains` as
   substring on every type and never flags it.

## Notes

- The sorter already compares dates natively (`compare_dates` in
  `crates/core/src/query/sort.rs`); this brings filtering in line.
- Well-formed ISO comparisons give identical results before and after.
  Two things change: an operand that is not a date (`03/01/2026`,
  `yesterday`) matches nothing instead of something arbitrary, and an
  unpadded one (`2026-3-1`) compares correctly as March 1st. Found while
  testing: the date parser the whole app uses (frontmatter coercion,
  the operand checker, now the evaluator) accepts unpadded month and
  day, so `2026-3-1` was never malformed here — it was only compared
  wrongly, as text.
- Tests follow the three-layer rule: the behaviour is asserted once at
  the query engine in `crates/core/tests/query.rs` (an unpadded operand
  through a real project), and the operand shapes — malformed dates and
  the textual `contains` — are unit cases in `crates/core/src/query/eval.rs`.
- Review follow-up (2026-09-09): the `YYYY-MM-DD` parse was spelled out
  at seven sites (coercion, resource constants, rule conditions, the
  operand checker, the `set` date delta, and now the evaluator). They
  all go through `parse_date` in `crates/core/src/model/date.rs`, so the
  grammar the evaluator accepts is by construction the one frontmatter
  accepts.
