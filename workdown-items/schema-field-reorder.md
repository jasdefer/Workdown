---
id: schema-field-reorder
status: to_do
parent: schema-editor-web
depends_on: [schema-page-read-view, schema-field-write-backend]
title: Drag fields into a new order on the schema page
---

## In plain words

Field order in `schema.yaml` is the order of the create form, the
detail panel and the board's columns. Today changing it means cutting
and pasting YAML blocks. This item adds a drag handle to each row of
the fields table; dropping a row writes the new order.

Decision 11 of [[schema-editor-web-design]].

## What is needed

- A drag handle per row, using the drag-and-drop helpers the board
  already has.
- On drop, call the order endpoint from
  [[schema-field-write-backend]] with the full ordered list. The
  server reorders the entries under `fields:` in the YAML tree;
  comments are not preserved (revised 2026-09-22).
- `id` stays first; the handle on its row is disabled.
- No conflict handling: last write wins, and the page refetches on the
  watcher's ping after the write.

## Not in scope

- Reordering rules, or values inside a choice field (the latter is
  [[schema-field-editor-values]]).
