---
id: schema-field-editor-numeric
status: to_do
parent: schema-editor-web
depends_on: [schema-field-editor-shell]
title: Field editor block for integer, float and duration
---

## In plain words

Numbers and durations can be bounded: an estimate cannot be negative, a
percentage stops at a hundred. In the schema that is `min` and `max`.
This item adds the block of the field editor that edits them, typed per
type — an integer bound is a whole number, a duration bound is a
duration.

One of the five type-specific shapes in [[schema-editor-web-design]].

## What is needed

- `min` and `max`, each optional, entered with the type's own editor
  so a duration bound reads `2d` and not `2`.
- Client-side check that `min` does not exceed `max`; the server's
  parse rejects it anyway, this only saves a round trip.
- Clearing a bound removes the key from the file.

## Not in scope

- `compute`, which these types also accept:
  [[schema-derived-field-editor]].
