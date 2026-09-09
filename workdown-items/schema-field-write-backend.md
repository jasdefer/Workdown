---
id: schema-field-write-backend
status: to_do
parent: schema-editor-web
depends_on: [schema-definition-api]
title: Write one field definition into schema.yaml without touching the rest
---

## In plain words

The server must be able to add, replace, remove and reorder a single
field in `schema.yaml` while leaving every other byte of the file as
it was — comments, blank lines, the order of everything else. It must
refuse a write that would leave the project unable to load, and tell
the caller, before writing, how many items and views a change would
newly put a warning on.

Decisions 3, 4, 6, 9 and 11 of [[schema-editor-web-design]].

## What is needed

- **`operations/schema_write.rs`** in core, beside `view_write.rs`.
  Four operations: add a field (appended at the end of `fields:`),
  replace a field's definition, remove a field, reorder fields.
- **Splice, not re-serialize.** Locate the field's mapping entry by
  byte span and replace only that. `serde_yaml` exposes no spans and is
  archived upstream; a span-aware parser (`saphyr` or similar) finds
  the entry, `serde_yaml` keeps parsing everything else. The replaced
  entry is serialized from the model, so comments inside it are lost;
  the entry's comment above it and everything outside survive. A test
  round-trips the shipped default schema through an edit of one field
  and asserts every other line is byte-identical.
- **Two tiers.** Build the candidate file text, run it through
  `parse_schema`. A load failure rejects the write with the parse error
  and nothing on disk changes. A loadable candidate is loaded against
  the store, views and resources in memory, and its diagnostics diffed
  against the current load; the diff is the preview.
- **Preview and commit as two calls.** The endpoints accept a
  `preview` mode that returns the diff without writing, and a write
  mode that writes. The client shows the preview and, on confirm,
  sends the write. Both carry the content hash from
  [[schema-definition-api]]; a mismatch is `409` and nothing is written.
- **Guards.** `id` cannot be removed. A field named by a role in
  `config.yaml` (board, tree, graph, display, effort) cannot be removed,
  because config is read at boot and the break would surface only on
  restart.
- **Endpoints.** `POST /api/schema/fields`,
  `PUT /api/schema/fields/{name}`, `DELETE /api/schema/fields/{name}`,
  and a reorder endpoint taking the full ordered name list. Same-origin
  guard and envelope as every other mutation (ADR-013).

## Not in scope

- Rename. Its consequences span items, views, rules and expressions;
  [[schema-field-rename]].
- Rewriting items on remove. Remove is remove; items keep the key and
  get the existing unknown-field warning.
- Any UI. [[schema-field-editor-shell]].
