---
id: resource-option-lists
type: issue
status: to_do
title: Load resources.yaml and serve resource option lists
parent: view-authoring
depends_on: [mutations-slice]
effort: "1d"
---

`core` does not load or validate `resources.yaml` today — only `rename`
text-scans it. A field declared `resource: people` is therefore accepted
with any string, and the UI has no list of valid people to offer, so the
editor built in [[mutations-slice]] falls back to a free-text input for
resource-backed fields. This issue closes that gap end to end: load and
validate resources, then expose each resource's entries so the UI can
render a proper picker.

This also fills the resource-value half of [[schema-metadata-api]]'s
acceptance ("fields backed by a resource, the UI can discover the allowed
values") — the two should be reconciled when this lands.

## Scope

- **Core: load `resources.yaml`** into a model — named sections
  (`people`, `teams`, …), each a list of entries with at least an `id`
  (plus arbitrary display fields like `name`, `email`).
- **Core: validate** that a `resource:`-backed field's value matches an
  `id` in the referenced section. Save-with-warning per ADR-001 (a bad
  reference warns, doesn't hard-reject) — new diagnostic kind, e.g.
  `UnknownResourceRef { field, section, value }`. A field pointing at a
  resource section that doesn't exist is a schema/config diagnostic.
- **Server: serve the option lists.** Extend the editing-vocabulary
  endpoint (`schema_data` / `GET /api/schema`) so a resource-backed
  `FieldSchema` carries its options (id + display label), or add a
  sibling resources payload the UI can join on the `resource` name.
- **UI: render resource fields as a picker** (single → select, and the
  pattern extends to a future multi-resource field) instead of free
  text, in both the detail editor and the create form.

## Acceptance

- A project with `people` entries validates a work item whose `assignee`
  is a known id without warning, and warns on an unknown id (file still
  saves).
- `GET /api/schema` (or its successor) reports the option list for each
  resource-backed field.
- The item editor renders `assignee` (and any `resource:` field) as a
  picker populated from the resource, not a text box.

## Out of scope

- Editing `resources.yaml` from the UI — stays a text-editor job.
- Resource entry display-field customization (which field is the label)
  — default to `name` then `id`; revisit with display-config.
- A dedicated `resource` field *type* — `resource:` stays a constraint
  on existing types (string/list), per the current schema model.
