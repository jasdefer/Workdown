---
id: when-then-value-expressions
type: issue
status: to_do
title: "`then:` values beyond literals — `$today`, fields, expressions"
depends_on: [conditional-field-value]
---

`when:` branches take literal `then:` values only — a deliberate v1 cut
recorded in [[conditional-field-value]]. The first real case that wants
more is the "ongoing bar": a date field that grows until today while an
item is open past its end date.

```yaml
effective_end:
  type: date
  when:
    - if: end_date < $today
      then: $today        # not expressible: then is literal-only
```

The slope is real and should be climbed deliberately: `then: $today` is
one token, but the natural next asks are `then: end_date` (a field
reference) and `then: end_date + 1d` (an expression) — at which point
`then` is `compute` with extra steps. If this is built, build the full
expression form once (the grammar, type checking, and evaluation all
exist), not a `$today`-only special case that becomes its own legacy.

Wait for actual friction before scheduling. The motivating colour cases
need none of this.
