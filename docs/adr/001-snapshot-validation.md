# ADR-001: Snapshot-only validation

**Status:** Accepted
**Date:** 2026-04-10

## Context

Work items are Markdown files edited directly or via the CLI. The question is whether `workdown validate` should check state transitions (requiring git history diffing) or only validate the current state of files.

## Decision

Validation is snapshot-only. The CLI validates the current state of all work items against the schema without inspecting git history. No state transition enforcement.

## Consequences

- Simpler implementation, no git dependency for validation
- Transition rules may be added later for visualization or CLI command guidance, but not for validation
- Users are free to set any valid value for any field at any time
