---
id: schema-field-type-change
status: to_do
parent: schema-editor-web
depends_on: [schema-field-editor-shell, schema-definition-api]
title: Let an existing field change type, but only to a type that keeps every value valid
---

## In plain words

Changing a field's type after items hold values is how a project
breaks itself: turn a free-text field into a dropdown and every value
not on the list is suddenly wrong. So the editor forbids type changes,
with a short list of exceptions where nothing can go wrong — a whole
number is a valid decimal, anything single-valued is valid text.

Decision 12 of [[schema-editor-web-design]].

## What is needed

- The type selector for an existing field enables only the current
  type and the targets the widening table from [[schema-definition-api]]
  lists for it. Others are greyed with the hint "change in
  schema.yaml".
- Picking a new type keeps the header block, shows a confirm naming
  the type-specific properties that will be dropped, and swaps the
  block beneath.
- The save preview still runs: a widening change can still trip a
  rule or a view slot that expects the old type.
- The widening table as first shipped: integer → float; integer, float,
  date, duration, boolean, choice, color → string; multichoice, links →
  list.

## Not in scope

- Any conversion that rewrites item values. String → choice with the
  values pre-filled is [[schema-string-to-choice]].
