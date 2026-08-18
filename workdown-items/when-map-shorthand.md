---
id: when-map-shorthand
status: to_do
title: "`map:` — lookup-table shorthand over the `when:` evaluator"
parent: schema-expressions
depends_on: [conditional-field-value]
---

## In plain words

Offer a short table for simple "this value means that value"
configuration, instead of one hand-written condition per case.

The tool can already express this as a list of conditions, so nothing
new becomes possible — but a plain table is shorter to write and easier
to read. The real gain is that a table can be checked for
completeness: the tool can warn when one of the possible values has
been forgotten. **Example:** mapping each status to a colour — in
progress means blue, done means green, everything else grey — as three
table lines rather than three separate conditions, with a warning if a
fourth status exists and was left out. Only worth building if writing
the long form turns out to be tedious in practice; so far it has not.

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
