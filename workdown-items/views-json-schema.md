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

### 3. CLAUDE.md fixes

- **Line 30 contradiction (ADR-005):** `schema.schema.json` is described as "Used by the CLI for validation and by editors for autocomplete." Change to: "Used by editors for autocomplete. Not loaded by the CLI at runtime — see ADR-005."
- Add a matching bullet for `views.schema.json`.
- Add `views.schema.json` to the **Project Structure** block under `defaults/`. (Also update the stale `src/` → `crates/core/src/` and `defaults/` → `crates/core/defaults/` paths if the sibling issue `views-config-path` hasn't landed yet; if it has, these will already be fixed.)

## Acceptance

- `views.schema.json` validates the 11-view example in `docs/views.md`
- Schema rejects an obvious bad config (e.g. board without `field`, metric without `aggregate`, unknown slot on any type)
- CLAUDE.md no longer contradicts ADR-005
- `views.schema.json` listed in CLAUDE.md project structure

## Out of scope

- Runtime validation against this schema — ADR-005 says editor-only
- Wire-up in IDE settings / `$schema` reference in user's views.yaml (user's editor config is their business)
