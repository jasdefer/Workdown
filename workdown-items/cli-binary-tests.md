---
id: cli-binary-tests
title: Test the shipped binary, so the CLI's wiring and exit codes are pinned
status: to_do
parent: testing-strategy
---

## In plain words

Nothing runs the `workdown` executable a user installs. Every Rust test
calls a library function, so the 300 lines between the command line and
the operations — clap parsing, the dispatch in `crates/cli/src/main.rs`,
the flags being handed on, the exit code coming back — have only ever
been exercised by hand. Coverage confirms it: every `commands/*.rs`
file except `serve.rs` sits at 0%. **Example:** the `validate` arm
returns `exit_code(!result.has_errors)`. Drop the `!` in a refactor and
validate exits `0` on broken items and `1` on clean ones; every core
test stays green, and the generated pre-commit hook now blocks clean
commits and lets broken ones through.

This item adds the binary layer from decision one of
[[testing-strategy-design]]: a new `crates/cli/tests/` directory that
runs the compiled executable in a temp project and asserts only on what
the CLI adds.

## Scope (decision two, CLI binary)

Per command:

- One invocation that reaches its operation and exits `0`.
- Each flag arrives at the operation, once per flag, proven by a flag
  that visibly changes the result (a file written differently, a
  different item picked).
- Exit `1` on an operation error and `2` on a malformed invocation, once
  per command, since each command has its own parse. `workdown add`
  parses its schema-derived flags itself and is the one to watch.
- `serve` without a project exits without blocking. Nothing else about
  `serve`; it has no shutdown handler.

Not in scope: what any operation does to the files beyond the one
assertion that proves the flag arrived. That is core's job and is
already tested there.

## Shape decisions to take while building

Inherited from the design item; decide in the first commit and record
here:

- **Fixture:** purpose-built config and schema strings written to a
  temp directory, as `crates/server/tests` does. `workdown init` in the
  temp directory is the alternative, and would couple every test to the
  shipped defaults.
- **Assertions:** exit code first, then a substring of stdout or stderr,
  or a `--format json` field where the command has one. No snapshots,
  which would freeze wording the project keeps deliberately changing.
- **Binary location:** the `CARGO_BIN_EXE_workdown` environment variable,
  which cargo sets at compile time for integration tests of a crate with
  a binary target.

## Watch out

Building the CLI test binary needs `ui/dist/` present, because the CLI
links `workdown-server`, whose embedded assets only switch to the
committed fixture under `cargo test` for the server crate itself. Run
the UI build first, or the gate misleads. The dev container has
`ui/dist` today.

## Done when

- `crates/cli/tests/` exists and runs under `cargo test --workspace`.
- Every command has the cases above; the exit-code contract from
  `docs/architecture.md` is asserted for each.
- Coverage of `crates/cli/src/commands/*` is no longer zero (a check,
  not a target).
