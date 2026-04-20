---
id: views-yaml-validation
type: issue
status: to_do
title: views.yaml JSON Schema and load-time validation
parent: foundation
depends_on: [views-yaml-design]
---

Formalize `views.yaml` with a JSON Schema, mirroring `schema.schema.json`. Validate at CLI startup.

## Scope

- `defaults/views.schema.json` — formal structure
- Load `views.yaml` when it exists; validate against the JSON Schema
- Friendly error messages on validation failure (point to the bad field with a line number if practical)
- `workdown validate` also performs cross-file checks the JSON Schema can't express: referenced fields must exist in `schema.yaml`, referenced field types must be compatible with the view type (choice → board, link → tree, links → graph)

## Out of scope

- Theming / styling config
