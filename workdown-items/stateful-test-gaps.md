---
id: stateful-test-gaps
status: to_do
title: Test the two stateful areas that currently have no coverage
parent: maintenance-review-2026-08
---

## In plain words

Test coverage is strong almost everywhere — with two exceptions, both
stateful. In the web app, all existing tests cover pure calculation
helpers; the timer's browser-side state machine has none. And the
command-line binary has no end-to-end tests: the logic underneath is
well tested, but the wiring — flags reaching the right code, exit codes
coming back right — is only exercised by hand.

The review framed both as "regressions here ship silently". That holds
for the CLI's exit codes and for two specific timer paths. It does not
hold for the rest of the timer store, and the scope below is cut
accordingly — see "Decisions taken".

## The problem in detail

**Web app stores.** All 977 existing test lines target extracted pure
modules (`clauses`, `gantt/scale`, `timerMath`, `announcements`, and so
on) — the extraction habit is exactly right, and it means the timer's
*arithmetic* is already covered. Untested:

- `ui/src/lib/stores/timer.svelte.ts` (326 lines): the toast state
  machine, undo via `previous_value` replay (line 302), the 409-resync
  path (line 264).
- `ui/src/lib/api/client.ts` envelope normalization.

**CLI wiring.** `crates/cli` has no `tests/` directory; only in-file
unit tests for renderers, `schema_args`, and serve helpers. Core
operations and server endpoints are integration-tested (roughly 7,500
lines between them), so *logic* coverage is good — but the clap wiring
in `main.rs` (exit codes, flag plumbing such as the `--as-of`
passthrough) is exercised only manually. An `assert_cmd` smoke suite —
a handful of end-to-end invocations against a fixture project,
asserting exit codes and key output — closes it.

## What is actually at risk

The store's dependencies are five outside things: the API client, the
clock (`performance.now`), the once-a-second tick, the announcer
(chime + notification + cross-tab claim), and the deadline Worker.
Sorting the store's behaviours by whether a regression is *visible*:

**Silent — worth a test:**

- **Undo writing the wrong value.** Undo must restore exactly the prior
  value, including "absent" when the field had none (`op: unset` vs
  `op: replace`). Getting this wrong writes a wrong duration into a
  work item and reports success. Data damage with no symptom.
- **The stale-tab stop (409).** Only fires in a race that daily use
  will not hit, so dogfooding genuinely does not cover it. The path is
  deliberately different from other failures: different advice line,
  plus a resync.

**Loud — not worth a test.** Toast lifecycle, the `busy` flag that
disables the controls, the ticking countdown display, the zero-crossing
chime. These break in front of the only user, immediately. Testing them
is also where all the setup cost sits: a fake clock and a fake announcer
exist only to serve the countdown/announcement tests, and the crossing
*math* they would re-cover already has tests in `announcements.ts`.

## Decisions taken

### Decision 1 — how the timer store becomes testable (settled)

**Wrap the store in a factory and inject one dependency: the API
client.**

The file becomes `createTimerStore({ api })`, with a single
`export const timerStore = createTimerStore({ api })` at the bottom.
Every call site keeps importing `timerStore`; no app behaviour changes.

Rejected alternative: leave the store as a module singleton and reset
the module registry between tests (`vi.resetModules()` plus dynamic
import) with the API replaced via `vi.mock`. It needs no production
change, but couples tests to import mechanics and leaves the module's
state bleeding between tests by default rather than by accident.

Consequences that settle two follow-up questions:

- **Only the API is injected.** The clock, the tick, the announcer and
  the Worker stay as they are. They were only needed for the
  countdown/announcement tests, which are now out of scope. The Worker
  already self-disables outside a browser (`typeof Worker === undefined`,
  line 88), which is exactly what a test wants.
- **No DOM environment is added.** `ui/vitest.config.ts` stays on the
  `node` environment; its comment about deferring DOM setup remains
  accurate and should be left in place.
- **The Svelte compiler plugin is still required.** `timer.svelte.ts`
  uses runes (`$state`, `$effect.root`), so Vitest cannot load it
  without `@sveltejs/vite-plugin-svelte` in the config. This is the one
  piece of new test machinery the trimmed scope still needs, and the
  vitest config comment should be updated to say so.
- **Side benefit:** the module-level `$effect.root` (line 137) moves
  inside the factory, so it stops firing on import in code paths that
  never use the timer.

**Worth weighing at implementation time:** the undo payload choice
(line 302) is a pure function of the stop result. Extracting it into
`timerMath.ts` alongside the existing pure helpers would make the
data-damage test cost nothing at all — no factory, no Svelte plugin.
The other three tests still need the store, so this is a "does it
shrink the whole thing enough to matter" call, not a settled decision.

## Objective

**Timer store** — the factory change from Decision 1, plus exactly four
tests:

1. Undo restores the previous value.
2. Undo unsets the field when it was absent before. *(the silent one)*
3. Stop answered with 409 resyncs and does not advise stopping again.
4. A failed undo keeps the stop result so a retry is possible.

**API client** — envelope normalization, with `fetch` stubbed. Needs no
refactor: empty body normalizes to the empty envelope, malformed JSON
becomes an error result, a network rejection becomes `status: 0` rather
than a thrown exception (which would strand a caller's `busy` flag).

**CLI** — an `assert_cmd` smoke suite covering each command's happy path
and the warning/failure exit-code contract, as originally scoped.

## Out of scope

- **Component (rendered-DOM) tests for Svelte views** — separate
  decision, separate cost.
- **Re-testing logic that core and server integration tests already
  pin.**
- **Timer-store toast lifecycle, `busy`-flag, countdown-display and
  zero-crossing tests** — cut deliberately, not overlooked. Every one of
  these fails visibly and immediately for the only user of the tool,
  and together they are what would force a fake clock, a fake announcer
  and a DOM environment into the test setup. Revisit if Workdown gains
  users who are not the author, or if the store starts changing often.

## Open decisions

Still to settle before implementation. Numbering follows the session
that produced Decision 1; the gaps are the questions it answered.

### Decision 4 — the CLI's exit-code contract *(settled elsewhere)*

Resolved and implemented in [[cli-exit-code-contract]]: `0` succeeded,
`1` the work failed or warned, `2` the invocation was malformed, stated
in `docs/architecture.md`. Nothing is left to decide here. Whether a
test holds the contract in place is part of the CLI question below.

### Decision 5 — how a CLI test gets a project

- **(a)** Run `workdown init` in a `TempDir` — self-bootstrapping, and
  exercises the shipped defaults. But then every test depends on
  `defaults/schema.yaml`.
- **(b)** Write purpose-built config/schema strings into a `TempDir` —
  what `crates/server/tests` already does. Explicit, stable, verbose.
- **(c)** A checked-in fixture project copied per test.

Recommendation: (a) for one init-and-smoke test, (b) for everything
behaviour-specific, behind a small helper in `crates/cli/tests/`.

Related yes/no: three crates would now hand-roll fixture projects —
**one shared dev-only test-support helper, or accept the duplication?**
Leaning accept; a shared fixture builder tends to accumulate every
caller's special case.

### Decision 6 — assert on substrings, or snapshot the output?

Snapshots would lock in the wording that `message-style-consistency`
just standardised, at the cost of churn on every deliberate wording
change. Recommendation: pattern/substring assertions — it is a smoke
suite. Revisit only if output formatting starts regressing.

### Decision 7 — `serve` in the smoke suite?

`workdown serve` blocks forever and has no shutdown handler, so it
cannot be smoke-tested as written. Exclude it (recommended — the
server's endpoints are already integration-tested, and a shutdown
handler is its own item), or add the handler first.

## Notes for implementation

- Building the CLI's test binary needs `ui/dist/` present: the CLI
  depends on `workdown-server`, whose `UiAssets` embed only switches to
  the committed fixture under `cargo test` *for that crate*. Run the
  full gate (`cargo xtask build-ui`, then the workspace checks) — a
  partial run misleads here.
- `cargo xtask build-ui` runs `npm run test`, so the new UI tests are
  CI-gated automatically; no `ci.yml` change needed.
- New dev-dependencies for `crates/cli`: `assert_cmd`, `predicates`,
  `tempfile`.
