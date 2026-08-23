---
id: stateful-test-gaps
status: to_do
title: Test the two stateful areas that currently have no coverage
parent: maintenance-review-2026-08
---

## In plain words

Test coverage is strong almost everywhere — with two exceptions, both
stateful. In the web app, all existing tests cover pure calculation
helpers; the timer's state machine (the toast, the undo, the recovery
when the server disagrees) has none, so a regression there ships
silently. And the command-line binary has no end-to-end tests: the
logic underneath is well tested, but the wiring — flags reaching the
right code, exit codes coming back right — is only exercised by hand.

## The problem in detail

**Web app stores.** All 977 existing test lines target extracted pure
modules (`clauses`, `gantt/scale`, `timerMath`, and so on) — the
extraction habit is exactly right. Untested:

- `ui/src/lib/stores/timer.svelte.ts` (326 lines): the toast state
  machine, undo via `previous_value` replay (line 302), the 409-resync
  path (line 264). Regressions here — double-stop, stuck `busy` flag,
  wrong undo payload — are precisely the silent-shipping kind.
- `ui/src/lib/api/client.ts` envelope normalization.

The vitest environment is `node` (`ui/vitest.config.ts`), so store and
component tests need environment setup first — that setup is part of
this item.

**CLI wiring.** `crates/cli` has no `tests/` directory; only in-file
unit tests for renderers, `schema_args`, and serve helpers. Core
operations and server endpoints are integration-tested (roughly 7,500
lines between them), so *logic* coverage is good — but the clap wiring
in `main.rs` (exit codes, flag plumbing such as the `--as-of`
passthrough) is exercised only manually. An `assert_cmd` smoke suite —
a handful of end-to-end invocations against a fixture project,
asserting exit codes and key output — closes it.

## Objective

- Timer-store tests covering start/stop/undo/409-resync and the toast
  lifecycle, plus the vitest environment work they need.
- An `assert_cmd` smoke suite for the CLI covering each command's
  happy path and the warning/failure exit-code contract.

## Out of scope

- Component (rendered-DOM) tests for Svelte views — separate decision,
  separate cost.
- Re-testing logic that core and server integration tests already pin.
