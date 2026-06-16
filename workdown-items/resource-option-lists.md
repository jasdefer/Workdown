---
id: resource-option-lists
type: issue
status: to_do
title: Validate resource references and render resource pickers
parent: view-authoring
depends_on: [mutations-slice, schema-metadata-api]
effort: "4h"
---

[[schema-metadata-api]] loads `resources.yaml` and exposes each
resource's entries in the schema metadata, so the UI knows the valid
values for a `resource:`-backed field. Two gaps remain once it lands:
the values aren't *validated* against items, and the editor still
renders resource fields as free text. This issue closes both.

## Scope

- **Core: validate** that a `resource:`-backed field's value matches an
  `id` in the referenced section. Save-with-warning per ADR-001 (a bad
  reference warns, doesn't hard-reject) — new diagnostic kind, e.g.
  `UnknownResourceRef { field, section, value }`. A field pointing at a
  resource section that doesn't exist is a schema/config diagnostic.
- **UI: render resource fields as a picker** (single → select, and the
  pattern extends to a future multi-resource field) instead of free
  text, in both the detail editor and the create form, populated from
  the option lists [[schema-metadata-api]] serves.

## Acceptance

- A project with `people` entries validates a work item whose `assignee`
  is a known id without warning, and warns on an unknown id (file still
  saves).
- The item editor renders `assignee` (and any `resource:` field) as a
  picker populated from the resource, not a text box.

## Out of scope

- Loading `resources.yaml` and serving the option lists — that lands in
  [[schema-metadata-api]].
- Editing `resources.yaml` from the UI — stays a text-editor job.
- Resource entry display-field customization (which field is the label)
  — default to `name` then `id`; revisit with display-config.
- A dedicated `resource` field *type* — `resource:` stays a constraint
  on existing types (string/list), per the current schema model.
