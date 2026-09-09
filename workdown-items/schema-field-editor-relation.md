---
id: schema-field-editor-relation
status: to_do
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

- **Allow cycles** toggle, writing `allow_cycles`.
- **Inverse** picker: a name for the derived inverse relation. Free
  text following identifier rules, refused when it collides with an
  existing field name, since the inverse is addressable in views and
  rules as if it were a field.
- Switching `allow_cycles` from true to false on a field whose items
  already form a cycle surfaces in the save preview as the existing
  cycle diagnostic.

## Not in scope

- `pull` and `aggregate`, which relation fields feed:
  [[schema-derived-field-editor]].
