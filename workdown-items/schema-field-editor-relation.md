---
id: schema-field-editor-relation
status: done
parent: schema-editor-web
depends_on: [schema-field-editor-shell]
title: Field editor block for link and links
---

## In plain words

A relation field points at other items. Two things about it are
configurable: whether it may form a cycle, and what the relation is
called from the other side (`parent` is seen as `children` from the
child). This item adds the block that edits both.

One of the five type-specific shapes in [[schema-editor-web-design]].

## What is needed

- **Forbid cycles** box, writing `allow_cycles`.
- **Inverse** as free text: a name for the derived inverse relation,
  following identifier rules and refused when it collides with an
  existing field name or another field's inverse, since the inverse
  is addressable in views and rules as if it were a field.
- Switching to forbid cycles on a field whose items already form a
  cycle surfaces the existing cycle diagnostic as a warning after the
  save.

## Decisions (2026-10-07)

- **One box, not three states.** The file can say `false`, `true` or
  nothing, but only `false` turns the cycle check on and only `false`
  lets a `pull` or `aggregate` climb the relation; `true` and nothing
  behave the same. The block offers "Forbid cycles": checked writes
  `false`, unchecked removes the key. A field that said `true` is
  written without the key on its next save, which means the same thing.
- **The inverse is judged by the server alone.** The identifier rule
  and the two collision rules live in the schema parser, where the
  field name itself is judged; the browser checks nothing beyond
  blank-means-none, so the rule is kept in one place. The server's
  message comes back to the panel on save, the way a bad pattern does.
- **No upfront lock for dependents.** Dropping the forbid on a relation
  a recipe climbs, or the inverse a rule traverses, makes the schema
  unloadable, which the write backend rejects without writing. The
  panel does not grey the controls out ahead of that; the usage
  endpoint would first have to say whether a rule reaches the field
  through its inverse.

Landing this block retired the panel's "edited in schema.yaml for now"
note: every type-specific property now has a control.

## Not in scope

- `pull` and `aggregate`, which relation fields feed:
  [[schema-derived-field-editor]].
