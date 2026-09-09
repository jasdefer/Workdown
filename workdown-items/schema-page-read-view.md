---
id: schema-page-read-view
status: to_do
parent: schema-editor-web
depends_on: [schema-definition-api]
title: A schema page that shows the fields and rules
---

## In plain words

The web app has no page for the schema. Someone who wants to know
which fields exist and what they accept opens `schema.yaml` in an
editor. This item adds a `/schema` route that shows the schema as the
project has it, read-only. It is the surface every editing item after
it builds on, and useful by itself.

The page as defined in [[schema-editor-web-design]], without the
editing affordances.

## What is needed

- **Route and navigation.** `/schema` in the app shell, reachable
  from the navigation next to the views.
- **Fields section.** A table in declaration order: name, type,
  required, default, first line of the description, and badges for
  *computed*, *conditional*, *pulled*, *aggregated* and
  *resource-backed*. `id` is the first row. Rows are not yet clickable.
- **Rules section.** One entry per rule: name, description, and the
  `match` and `require` blocks rendered as the file has them.
- **Broken state.** When the schema does not load, the page shows the
  load diagnostic and the file path in place of both sections. Every
  other read is in the `422` tier at that moment; this page is where
  the reason is legible.
- **Live refresh.** The page refetches on the file watcher's ping like
  every other view, so an edit in a text editor shows up.
- Reads only `GET /api/schema/definition`. No type knowledge in the
  frontend beyond what the payload carries.

## Not in scope

- Clicking a row, adding, removing, reordering. Those are
  [[schema-field-editor-shell]] and [[schema-field-reorder]].
