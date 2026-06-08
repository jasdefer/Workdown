---
id: schema-metadata-api
type: issue
status: to_do
title: Expose schema metadata so the UI can offer valid choices
parent: view-authoring
depends_on: []
effort: "8h"
---

Any UI that lets a user pick a field, an operator, or a value needs to know the shape of the project's schema: which fields exist, what type each is, which values a choice field allows, and which resources a field references. The serve API exposes view data but nothing about the schema itself, so today a builder UI would have no source of truth to populate its pickers — it could only let the user type raw strings and hope they're valid.

This issue makes the schema's structure available to the UI so that field, operator, and value selection can be constrained to what's actually valid, without the frontend hardcoding any field names or types (only `id` is privileged).

## What we want

- The UI can discover every field defined in the schema and each field's type.
- For fields that constrain their values (choice/multichoice, and fields backed by a resource), the UI can discover the allowed values.
- The UI can tell which operators make sense for a given field type, so it never offers an incompatible comparison.
- The information reflects the project's current schema on disk, like the rest of the serve API.

## Acceptance

- Given a project, the UI can render a field picker, an operator picker, and (where applicable) a value picker populated entirely from this metadata — no field names baked into the frontend.
- The available operators for a field match what the existing `where:` grammar accepts for that field's type.

## Out of scope

- Editing the schema from the UI — schema changes stay a text-editor job.
- Validation rules and aggregate configuration — only what's needed to build and constrain filters.
