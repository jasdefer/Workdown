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
- On drop, call the reorder endpoint from
  [[schema-field-write-backend]] with the full ordered list. The splice
  moves whole entries, comments above them included.
- `id` stays first; the handle on its row is disabled.
- The `409` case: the file changed underneath, the page refetches and
  the drop is discarded with a notice.

## Not in scope

- Reordering rules, or values inside a choice field (the latter is
  [[schema-field-editor-values]]).
