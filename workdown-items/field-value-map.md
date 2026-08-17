---
id: field-value-map
status: removed
title: Mapped fields — derive a value by lookup table
parent: schema-expressions
---

> **Superseded by [[conditional-field-value]].** Kept as a record of the
> reasoning, not as work to do.
>
> This item argued for a lookup table *instead of* conditionals, because
> expressing a lookup as if/else "would force booleans, comparisons, and string
> equality into the deliberately tiny expression grammar". That was sound while
> value-to-value mapping was the only requirement. It stopped being sound once
> a second requirement arrived — colouring by whether a date has passed — which
> forces those same three additions regardless. With the grammar extended,
> `status == done` is just another predicate and this table would be a second
> way to say the same thing.
>
> What the table had and the replacement does not: exhaustiveness checking
> against a `choice` field's declared values. That loss is accepted for now;
> if authoring multi-branch conditions proves tedious, `map:` may return
> later as sugar over the same evaluator — a far better position than
> maintaining two evaluators.

A declarative `map:` config on a field: derive this field's value by
looking up another field's value in a table. The motivating case is
color-by-status:

```yaml
status_color:
  type: color
  map:
    field: status
    values:
      in_progress: "#FBC02D"
      blocked: "#424242"
    default: "#EEEEEE"
```

This deliberately stays out of [[computed-fields]]: a lookup is not
arithmetic, and expressing it as if/else conditionals would force
booleans, comparisons, and string equality into the deliberately tiny
expression grammar. A table is easier to write, fully validatable at
load time, and trivially evaluatable.

## Scope

- `map:` config: source `field`, `values` table, optional `default`.
- Load-time validation: source field exists; when the source is a
  `choice` field, every key is one of its declared values; every value
  coerces to this field's declared type.
- Evaluation alongside the other derive passes; manual frontmatter
  value wins, mirroring [[computed-fields]] override behavior.
- `schema.schema.json` definition.

## Open questions

- Interaction with `compute`/`aggregate` on the same field (probably
  mutually exclusive in v1).
- Matching on non-choice source fields (string equality only?).

## Scheduling

Resolved: the work this was waiting for is scheduled under [[polish]], and
this item is superseded by [[conditional-field-value]] rather than scheduled
alongside it.
