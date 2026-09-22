---
id: schema-default-strip-comments
status: to_do
parent: schema-editor-web
title: Move the tutorial comments out of the shipped default schema
---

## In plain words

The default `schema.yaml` that `workdown init` copies into a project
is half comments: explanations of each section and commented-out
examples for aggregated, computed and pulled fields. The web editor
writes the file back without comments ([[schema-field-write-backend]],
requirement 2), so the first browser save deletes all of that. The
knowledge should live where a save cannot delete it, and the shipped
file should contain what the program reads.

## What is needed

- Move the explanations and the commented-out examples from
  `crates/core/defaults/schema.yaml` into the README or the schema
  docs page the comments already point to, keeping the examples
  copy-pasteable.
- Leave a short header comment pointing at that page; nothing else.
- Field descriptions carry any per-field explanation from now on.
- Check the resources and views defaults for the same pattern and
  note whether they need the same treatment; do not do it here.

## Not in scope

- Any change to what the schema contains.
- The web editor itself.
