---
id: schema-default-coercion-check
status: to_do
parent: schema-editor-web
title: Schema parser rejects a literal default the field cannot hold
---

## In plain words

`default: 1.5` on an integer field and `default: tomorrow` on a date
field both load today. The schema is accepted, and the mistake only
surfaces when `workdown add` tries to write the default into a new item
and fails. The parser checks a literal default by its YAML kind — a
number on a numeric field, a string on a string-like field — not by
converting it the way an item value is converted. A schema should fail
at load with the field named, not later at the first `add`.

Found on 2026-09-10 while building [[schema-definition-api]], whose
payload works around the gap by reporting such a default as `invalid`
with the conversion's reason.

## What is needed

- In `validate_default_compatibility`
  (`crates/core/src/parser/schema.rs`), run every literal default
  through the same coercion items go through for that field type,
  instead of matching on the YAML kind. Known holes today: a float on
  an integer field, and any string on a date field. Whatever else the
  coercion rejects that the kind check let through is fixed by the
  same move.
- The rejection names the field and carries the coercion's reason,
  in the style of the existing default messages.
- One parser test per rejected case that loads today.
- `crates/core/tests` integration coverage is the existing schema-load
  path; the server needs no new test.
- Keep the `invalid` variant of the definition payload's default as a
  fallback that can no longer fire from a loaded schema. Removing it
  is not worth a wire change.

## Not in scope

- Widening what a default may be. A string default on a duration or
  list field is rejected today by the kind check; whether coercion
  should accept `default: 2h` on a duration is a separate question and
  stays as it is unless the coercion move settles it for free.
