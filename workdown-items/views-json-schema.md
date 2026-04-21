---
id: views-json-schema
type: issue
status: to_do
title: Editor-only JSON Schema for views.yaml
parent: foundation
---

Ship `crates/core/defaults/views.schema.json` — a formal JSON Schema for `views.yaml` that editors (VS Code, IntelliJ) use for autocomplete and inline validation. Not loaded by the CLI at runtime (ADR-005).

## Context

`schema.schema.json` and `resources.schema.json` already exist and follow a consistent style: draft-2020-12, `$id`, `$defs`, `additionalProperties: false`, `if/then` branches for type-specific rules. `views.schema.json` should mirror that style.

Design of `views.yaml` is frozen in `docs/views.md` and the typed model in `crates/core/src/model/views.rs`. The schema mirrors that shape.

## Scope

### 1. `crates/core/defaults/views.schema.json`

- `$schema`: draft-2020-12
- `$id`: `https://workdown.dev/schemas/views.schema.json` (matches existing style)
- Top-level: `{ views: [View] }`, `additionalProperties: false`, `required: ["views"]`
- `View`: discriminated on `type` (enum of all 11 variants), with `allOf` / `if-then` branches enforcing per-type required/optional slots
- Shared: `id` (required string), `where` (optional `array` of `string`), `type` (required enum)
- `aggregate` enum: `count`, `sum`, `avg`, `min`, `max`
- `bucket` enum: `day`, `week`, `month`
- Per-type required slots (see `docs/views.md` table and `crates/core/src/parser/views.rs::convert_view`):
  - `board` / `tree` / `graph`: `field`
  - `table`: `columns`
  - `gantt`: `start`, `end` (+ optional `group`)
  - `bar_chart`: `group_by`, `aggregate` (+ optional `value`)
  - `line_chart`: `x`, `y`
  - `workload`: `start`, `end`, `effort`
  - `metric`: `aggregate` (+ optional `value`, `label`)
  - `treemap`: `group`, `size`
  - `heatmap`: `x`, `y`, `aggregate` (+ optional `value`, `bucket`)
- Unknown slots per view entry rejected via `additionalProperties: false` on each branch

### 2. Sanity check

The schema should validate the 11-view example in `docs/views.md` and in the `parses_full_example` test in `crates/core/src/parser/views.rs`. Run a JSON Schema validator locally against the example to confirm — can be done with any draft-2020-12 validator; no CI hook needed.

### 3. CLAUDE.md additions

- Add a bullet for `views.schema.json` alongside the existing `schema.schema.json` / `resources.schema.json` bullets in the **Configuration Files** section, using the same editor-only phrasing (see ADR-005).
- Add `views.schema.json` to the **Project Structure** block under `crates/core/defaults/`.

(The stale `src/`→`crates/core/src/` restructure and the ADR-005 wording fix for `schema.schema.json` already landed in `views-config-path`.)

## Acceptance

- `views.schema.json` validates the 11-view example in `docs/views.md`
- Schema rejects an obvious bad config (e.g. board without `field`, metric without `aggregate`, unknown slot on any type)
- `views.schema.json` listed in CLAUDE.md Project Structure and described in Configuration Files

## Out of scope

- Runtime validation against this schema — ADR-005 says editor-only
- Wire-up in IDE settings / `$schema` reference in user's views.yaml (user's editor config is their business)
