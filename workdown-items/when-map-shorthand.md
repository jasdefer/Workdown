---
id: when-map-shorthand
type: issue
status: to_do
title: "`map:` — lookup-table shorthand over the `when:` evaluator"
depends_on: [conditional-field-value]
---

The lookup table [[field-value-map]] was superseded by
[[conditional-field-value]] rather than shipped alongside it, with an
explicit note that `map:` may return later **as sugar over the same
evaluator** — one way to evaluate, two ways to author.

```yaml
status_color:
  type: color
  map:
    field: status
    values:
      in_progress: blue
      done: green
    default: gray
```

Desugars to a `when:` with one equality branch per key. What the sugar
adds over hand-written branches:

- **Exhaustiveness checking** — the one capability the supersession gave
  up: when the source is a `choice` field, warn about declared values
  the table leaves unhandled (and reject keys that aren't declared
  values at all).
- Less repetition for the pure value-to-value case.

Build only if multi-branch `when:` authoring proves tedious in practice;
the dogfooded colour config (four branches) has not.
