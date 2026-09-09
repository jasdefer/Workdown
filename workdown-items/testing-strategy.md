---
id: testing-strategy
title: Decide what our tests are for, and restructure them accordingly
status: done
---

## In plain words

The project has more test code than product code, and no written idea
of what any of it is for. Tests have accumulated wherever they were
easiest to write — overwhelmingly right next to the code they test —
rather than where they would say the most. Nothing anywhere runs the
program a user actually installs. **Example:** the exit code that
`workdown render` returns decides whether the pre-commit hook we
generate blocks a commit; if that code were ever inverted, the hook
would keep running, print nothing unusual, quietly stop protecting the
repository, and no test anywhere would notice.

This milestone is for settling what kinds of tests we want, what each
kind is responsible for, and what changes as a result. It commits to
no answer — see [[testing-strategy-design]], which is where the
approach gets worked out.

## What we have today

Counted 2026-08-28. "In-file test lines" means everything from a
file's `#[cfg(test)]` marker to the end of that file.

| Layer | Where | Lines |
|---|---|---|
| Unit — Rust, in-file | `crates/*/src/**` | ~30,900 |
| Unit — web app, pure modules | `ui/src/**/*.test.ts` | 980 |
| Integration — one operation, real project on disk | `crates/core/tests/` | 5,332 |
| Integration — one endpoint, real project on disk | `crates/server/tests/` | 2,663 |
| Anything running the built binary | — | 0 |

Roughly 38,900 lines of test against roughly 32,600 lines of product
code. Test modules sit in 77 of core's 103 source files, 19 of the
CLI's 36, and 5 of the server's 11.

## The core problem

**Nobody decided any of this.** There is no statement anywhere of what
a unit test is for here, what an integration test is for, or which one
a new piece of behaviour should get. In the absence of that, the answer
has always been "a test module at the bottom of the file", because that
is the path of least resistance in Rust. Two consequences:

- **The volume is a standing cost.** Every one of those ~38,900 lines
  is something a refactor has to carry. That is worth paying for tests
  that catch things; it is pure drag for tests that restate the code
  above them.
- **We cannot tell which is which.** Core's largest in-file blocks are
  `parser/schema.rs` (1,546 lines), `operations/set/mod.rs` (1,202) and
  `view_data/gantt.rs` (1,152). Some of that is a parser with genuinely
  many cases and is exactly right. Some of it may repeat what
  `crates/core/tests/` already proves end to end. Nobody has looked,
  and there is no coverage measurement to look with.

**Nothing tests the thing we ship.** Every Rust test calls a library
function directly. `crates/cli` has no `tests/` directory at all. The
layer between the command line and the operations — arguments reaching
the right code, optional flags actually being passed along, exit codes
coming back — has only ever been exercised by hand.

**Exit codes are a contract nobody owns.** Twelve commands let clap
exit with the conventional `2` on a malformed invocation. `workdown
add` builds its flags from the project's schema and parses them itself,
and returns `1` (`crates/cli/src/main.rs:281`). Nothing writes the
convention down, so there is nothing to be inconsistent with. Meanwhile
the hook we generate contains `workdown render || exit 1`
(`crates/core/src/operations/install_hooks.rs:157`), which makes one of
these codes load-bearing for a user's repository.

**In the web app the split is accidental.** All 980 lines of UI tests
target pure calculation modules; every module holding live state has
none. That is not a judgement about risk — it is where a test could be
written without first adding test machinery.

**A green checkmark has already lied to us once.**
[[ci-workspace-coverage]] found that CI's `cargo test` was scoped to a
single crate, so the core and server suites — the large majority of the
tests — never ran on GitHub. Nothing about the passing check gave that
away. Whatever structure we land on, how we would notice the same class
of failure is part of the question.

## What this milestone does not decide

It does not presume a layer model, a directory layout, a naming scheme,
a coverage target, or that anything gets deleted. Those are the outputs
of [[testing-strategy-design]], not inputs to it.

## Related work already in flight

[[stateful-test-gaps]] (under [[maintenance-review-2026-08]]) held the
two areas with no coverage at all: the web app's timer state and the
CLI's wiring. It has been narrowed to the browser half, and its CLI
questions moved into [[testing-strategy-design]] — testing the built
binary is what this milestone is for, and answering it there would have
pre-empted it.

## Outcome (2026-09-09)

Designed in [[testing-strategy-design]] and built in five items:
[[cli-binary-tests]], [[relocate-operations-tests]], [[test-audit]],
[[ci-test-run-completeness]], [[testing-guide]]. What changed:

- **The thing we ship is tested.** 88 tests under `crates/cli/tests/`
  run the compiled binary: every command reaches its operation, every
  flag arrives, and the `0` / `1` / `2` exit-code contract is pinned per
  command. The inverted-exit-code failure from the opening paragraph now
  fails the build.
- **Every test has a place.** 107 operations integration tests moved
  from the bottoms of source files to `crates/core/tests/`; the
  remaining in-file blocks are unit tests of input-space functions.
- **Fewer tests, by rule.** 109 tests deleted, each against a named
  rule with the test that still covers the behaviour, and about 24
  file-content assertions removed from server tests. Net, the suite
  went from 1,960 Rust test functions to 1,939 while adding the binary
  layer, and from about 44,400 test lines to 43,130.
- **A silent green is loud.** CI derives every test target from the
  tree and fails by name when one did not run.
- **The rules are where a change meets them:** the Testing section of
  `docs/architecture.md`, pointed at from `CLAUDE.md`.

| Layer | Before (2026-09-09) | After |
|---|---|---|
| Rust unit, in-file | 32,300 lines | 26,161 |
| Core integration | 5,300 | 9,183 |
| Server integration | 4,700 | 4,346 |
| CLI binary | 0 | 1,374 |
| Web app unit | 2,100 | 2,066 |
| Rust test functions | 1,960 | 1,939 |

Parked, not done: browser tests; the Svelte store factory. Left open by
the audit as gaps rather than rule violations: `walker::walk_up`,
`model/field_value.rs` formatting for eight variants,
`cli/src/render/markdown.rs`, the `Multichoice` case of `schema_args`,
the port scan in `serve.rs`.
