# ADR-004: Validation rule system

**Status:** Accepted
**Date:** 2026-04-10

## Context

Field-level validation (required, min/max, values, pattern) covers single fields in isolation. Projects need additional constraints that span multiple fields on the same item ("if status is in_progress, assignee is required"), relationships between items ("parent can't be backlog if child is active"), and the collection as a whole ("at most 5 items in progress").

## Decision

A declarative **match/require/count** rule system defined in `schema.yaml` under a `rules:` key.

- **match** selects items by condition (scalar = equality, array = membership, object = operator). Multiple entries are AND. OR is not supported; use two rules.
- **require** asserts what must hold for matching items (keyword strings `required`/`forbidden`, or object with operators like `values`, `not`, `lte_field`, `min_count`).
- **count** constrains how many items may match (min/max). Used for collection-wide rules like WIP limits.
- **Dot notation** (`parent.status`, `children.type`) traverses relationships, one level deep.
- **Severity** (`error`/`warning`) on every rule.

The formal structure is defined in `defaults/schema.schema.json` (JSON Schema), replacing the previous `type_system.yaml`. JSON Schema was chosen over YAML-describing-YAML because it is unambiguous, machine-readable, and enables editor tooling (autocomplete, validation).

## Consequences

- All structural definitions (field types and rules) live in one JSON Schema file
- The CLI validates `schema.yaml` against the JSON Schema at load time
- Users write rules in YAML; the JSON Schema catches structural errors
- Behavioral semantics (null handling, AND logic, quantifier edge cases) are documented in `docs/schema.md` and implemented in the CLI, not expressible in JSON Schema
- OR conditions are a known limitation — can be added later if needed
