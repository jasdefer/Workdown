---
id: typed-date-filter-comparison
status: to_do
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

## Notes

- The sorter already compares dates natively (`compare_dates` in
  `crates/core/src/query/sort.rs`); this brings filtering in line.
- Existing behavior to preserve: well-formed ISO comparisons must give
  identical results before and after.
