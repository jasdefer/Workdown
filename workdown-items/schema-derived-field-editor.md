---
id: schema-derived-field-editor
status: on_hold
parent: schema-editor-web
depends_on: [schema-field-editor-shell]
title: Edit compute, when, pull and aggregate blocks in the field editor
---

## In plain words

Some fields work out their own value: a computed number, a colour
picked by condition, a total rolled up from children. The plain field
editor shows those blocks but sends you to `schema.yaml` to change
them. This item makes them editable in the panel.

**Parked until** the plain-field editors ([[schema-field-editor-shell]]
through [[schema-field-editor-relation]]) have shipped and shown what
the panel's UX is. Decided in [[schema-editor-web-design]]: the simple
blocks go first so the derived ones are designed against a real panel,
not a sketch.

## What has to be settled when it is picked up

- **Expressions.** A `compute` line and a `when` condition are text in
  a typed expression language. Free text with the type checker's
  diagnostics shown inline, or a structured builder like the filter
  editor already has for `where:`. The filter builder is the obvious
  starting point; whether it covers arithmetic is the question.
- **`when` branches.** An ordered list of condition → value pairs plus
  a default. A list editor with a condition input and the type's value
  editor per row.
- **`aggregate` and `pull`.** Function picker from the per-type table,
  link-field picker for `pull`.
- **Unlocking the type.** Once the expression is editable, the type
  lock from the shell can be lifted for derived fields, with the type
  check as the guard.

## Not in scope

- New expression grammar. That is [[schema-expressions]].
