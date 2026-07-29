---
id: conditional-derivation
type: milestone
status: to_do
title: Conditional & time-aware derivation
---

Derived field values can only be *arithmetic over other fields of the same
item* ([[computed-fields]], ADR-009). Two things a user reasonably expects are
therefore inexpressible: "make it green when the status is done" and "make it
blue once the end date has passed". The first needs comparison and equality,
the second additionally needs a notion of *now* during evaluation.

This milestone makes both expressible with one mechanism.

## Why one mechanism and not two

A lookup table (`status → colour`) was specced separately as
[[field-value-map]], on the explicit grounds that a table avoids "forcing
booleans, comparisons, and string equality into the deliberately tiny
expression grammar". The date case forces all three regardless. Once the
grammar has `>` and a boolean type, `status == done` is just another predicate
and a parallel table config is a second way to say the same thing — so
[[field-value-map]] is superseded here rather than shipped alongside.

The one capability a table has and a predicate list does not is exhaustiveness
checking: a table over a `choice` field can warn that a declared value went
unhandled. Accepted as a loss for now. If authoring multi-branch conditions
proves tedious, `map:` can return later as sugar over the same evaluator —
which is a far better position than maintaining two evaluators.

## Shape

First match wins, with an explicit default:

```yaml
urgency_color:
  type: color
  when:
    - if: status == done
      then: green
    - if: end_date > $today
      then: blue
  default: grey
```

Generic over field types — `color` is the motivating case, not a special one
(ADR-002: types drive behaviour, no field name or type is magic).

## Themes

- A notion of *now* available during evaluation, with rendered output that
  stays reproducible for a given commit.
- Predicates in the expression grammar: comparison, equality, booleans.
- A field config that picks a value by first matching condition.

## Boundaries

- Not about *where* a derived colour is displayed — `display.color` already
  accepts any `color`-typed field, shipped in [[view-presentation]]. This
  milestone only makes such a field derivable instead of hand-written.
- Not cross-item: conditions read the same item's fields, exactly as
  [[computed-fields]] does. Rolling a condition up a parent chain stays
  `aggregate`'s job.
