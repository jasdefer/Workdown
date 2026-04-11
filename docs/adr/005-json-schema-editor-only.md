# ADR-005: JSON Schema files are editor-only artifacts

**Status:** Accepted
**Date:** 2026-04-11

## Context

The project ships `schema.schema.json` and `resources.schema.json` — JSON Schema files that formally define the structure of `schema.yaml` and `resources.yaml`. The question is whether the CLI should load and validate against these JSON Schema files at runtime, or handle validation purely in Rust.

Three options were considered:

1. **Dual maintenance:** Rust structs + serde handle deserialization and custom validation. JSON Schema exists separately for editors. Two sources of truth maintained manually.
2. **JSON Schema at runtime:** Validate `schema.yaml` against `schema.schema.json` at runtime using a JSON Schema crate, then still parse into Rust structs for all behavioral logic.
3. **Code generation:** Compile JSON Schema into Rust validation at build time.

## Decision

Option 1: The JSON Schema files are **editor-only artifacts**. The CLI never reads or validates against them at runtime.

Validation of `schema.yaml` is handled by:
- **serde deserialization** — catches structural issues (missing fields, wrong types, unknown properties via `deny_unknown_fields`)
- **Custom Rust validation** — checks type-specific constraints (e.g. `choice` requires `values`, `min`/`max` only on numeric types) and rule semantics (field references resolve, operators are type-compatible)

## Rationale

The schema definitions drive far more than self-validation. The CLI needs rich typed Rust structs for work item validation, visualization (board columns, tree hierarchy), aggregate computation, and default generation. These structs must exist regardless of how `schema.yaml` is validated.

Option 2 would validate the file and then parse it again into Rust structs — double work with no reduction in Rust code. Option 3 adds build complexity for the same limited benefit.

The drift risk between JSON Schema and Rust is manageable: the type system is built into the CLI and changes infrequently. A CI test loading the default `schema.yaml` through both paths can catch divergence.

## Consequences

- `schema.schema.json` and `resources.schema.json` provide editor autocomplete (VS Code, IntelliJ) but are not loaded by the CLI
- All validation logic lives in Rust — single place to debug and extend
- JSON Schema and Rust validation rules must be kept in sync manually; CI tests mitigate drift
- No runtime dependency on a JSON Schema validation crate
