---
id: conditional-field-value
status: done
title: "`when:` — derive a field value by first matching condition"
parent: polish
depends_on: [expression-predicates, evaluation-time-now]
---

With predicates in the grammar ([[expression-predicates]]) and a readable
current date ([[evaluation-time-now]]), the remaining piece is a field config
that picks a value by condition. This is the user-facing feature the other
two issues exist to make possible.

```yaml
urgency_color:
  type: color
  when:
    - if: status == "done"
      then: green
    - if: end_date < $today
      then: red
  default: gray
```

First match wins, top to bottom. `default:` applies when no branch matches;
without one the field is simply left unset, exactly as an unsatisfiable
`compute` leaves it.

## Why a list rather than a nested expression

A conditional *expression* (`if a then b else if c then d else e`) would put the
whole thing on one line and need no new YAML shape. It reads badly for the case
this exists to serve — three or four colour branches — and it puts branch
structure inside a string, where the schema JSON Schema can't describe it and an
editor can't help. A list of `if` / `then` mappings keeps each branch on its own
line, makes ordering visible, and lets `schema.schema.json` type the `then`
values. The cost is one more config shape beside `compute:` and `aggregate:`.

## Scope

- The `when:` config: a list of `if` (expression, must type-check as boolean)
  and `then` (a literal coercing to the field's declared type), plus optional
  `default`.
- Load-time validation in the same place and to the same standard as
  `compute_check.rs`: every referenced field and constant exists, every `if`
  is boolean, every `then` and the `default` coerce to the declared type. A
  broken config surfaces once against `schema.yaml`, never once per item, and
  the field is disabled for evaluation the way `disabled_compute_fields`
  already works.
- Evaluation inside the existing derive pass (`store/derive.rs`), which already
  orders fields topologically over reference edges and runs compute before
  aggregate per field. `when:` participates in that ordering on the same terms,
  so a `when:` condition may read a computed field and vice versa.
- A hand-written frontmatter value always wins, mirroring [[computed-fields]].
  Never written back to the file.
- `schema.schema.json` definition so editors autocomplete it.
- `docs/schema.md`, and a worked colour example — this repo's own
  `.workdown/schema.yaml` and a view using `display.color` is the obvious
  dogfood.

## Decisions taken (2026-07-30)

- **`compute` + `when` on one field is a schema parse error** — two answers
  to "what is this value", rejected like `compute` + `default`. `when` +
  `aggregate` is supported: conditions fill leaves, the rollup fills
  ancestors, exactly the ADR-009 pattern.
- **One dependency graph.** `when:` condition references join the same
  graph as compute references, for both cycle detection and evaluation
  order — a condition may read a computed field and vice versa.
- **`then` is literal-only.** `$today` in `then:` (the "ongoing gantt bar"
  case) is the first candidate if then-expressions ever come; recorded,
  not built.
- **Absent condition inputs mean fall-through.** A branch that cannot be
  answered does not match; evaluation moves to the next branch, mirroring
  the rule engine's absent-operand skip. Unset + `required` is a per-item
  diagnostic naming the unmatched branches and the absent inputs; with a
  `default:` present, `required` can never fire.
- **`default:` next to `when:` is the evaluated fallback** — same keyword
  as add-time defaults, different mechanism, because a stamped default
  would permanently shadow every branch (hand-written values win). It
  must be a plain literal; generator tokens are rejected. It never
  reaches `workdown add`.

## Acceptance

- The example above, in this repo's schema, tints board cards and gantt bars
  through `display.color` with no colour hand-written on any item.
- A `when:` whose `if` isn't boolean, or whose `then` doesn't fit the field
  type, is one diagnostic against `schema.yaml` naming the field and the branch
  index — not one per item.
- A hand-written value in frontmatter beats every branch.
- Nothing is written back into any work item file.

## Out of scope

- A lookup-table shorthand. [[field-value-map]] is superseded by this issue;
  its record keeps the reasoning, and the exhaustiveness check that decision
  gives up. `map:` may return later as sugar over the same evaluator.
- Conditions over *other* items' fields.
