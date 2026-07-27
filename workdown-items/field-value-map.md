---
id: field-value-map
type: issue
status: to_do
title: Mapped fields — derive a value by lookup table
---

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
