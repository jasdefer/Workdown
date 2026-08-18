---
id: when-then-value-expressions
status: to_do
title: "`then:` values beyond literals — `$today`, fields, expressions"
parent: schema-expressions
depends_on: [conditional-field-value]
---

## In plain words

Allow a conditional field to hand back a calculated result, not only a
fixed value typed into the configuration.

Conditions can currently return a constant and nothing else. The first
real need is a bar on a schedule chart that keeps growing while an item
is overdue, which requires the answer to be "today's date" rather than
one particular day. The note to ourselves here is that once today's
date is allowed, other fields and small calculations will be wanted
immediately — so this should be built properly in one go rather than as
a one-off special case that becomes its own legacy. **Example:** an
item whose planned end date has passed but which is not finished should
draw its bar up to today, stretching a little further each day, instead
of stopping short at the old date. Not scheduled until someone actually
runs into the limitation.

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
