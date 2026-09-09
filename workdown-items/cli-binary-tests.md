---
id: cli-binary-tests
title: Test the shipped binary, so the CLI's wiring and exit codes are pinned
status: done
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

## Decisions taken (2026-09-09)

1. **Fixture:** one shared default project, written by
   `crates/cli/tests/common/mod.rs` from purpose-built config, schema,
   views, one template and two items. Variants (a `$today`-dependent
   schema) only where a flag cannot show through the default. Not
   `workdown init`, which would couple every test to the shipped
   defaults.
2. **Assertions:** exit code first, then one substring of stdout or
   stderr, or a JSON field where the command has `--format json`. Words,
   never glyphs or colour. No snapshots.
3. **Binary location:** `env!("CARGO_BIN_EXE_workdown")`.
4. **Environment isolation:** the helper clears `WORKDOWN_CONFIG` and
   `WORKDOWN_LOG` and runs in the temp root, so a developer's shell and
   CI see the same thing.
5. **Git-dependent commands:** `install-hooks` and `changes` get their
   exit-1 case outside a repository and their exit-0 case inside a
   throwaway repository created in the temp directory (`git init` plus
   one commit with a fixed identity). Nothing outside the temp
   directory is ever touched.
6. **`add` and the `2`:** an unknown flag, or a choice value outside the
   schema's values, is rejected by the schema-built parser and exits
   `2`; a well-formed invocation the operation refuses (no title,
   duplicate id) exits `1`. Both are pinned.
7. **`serve`:** only the no-project case, under a ten-second timeout so
   a regression that binds before loading the config fails the test
   instead of hanging the suite.
8. **Layout:** one file per command plus `cli.rs` for the top-level
   parse (`--help`, `--version`, `--config`, no project), so the CI
   completeness check of [[ci-test-run-completeness]] can enumerate
   them.

## Outcome (2026-09-09)

Fourteen files under `crates/cli/tests/`, 88 tests, about 1,000 lines
including the helper. Every command has its exit-0 case, every flag its
one visible-result case, and every command its `1` and `2` where the
command has a parse to fail (`render` has none). Full gate green in the
dev container: fmt, clippy `--all-targets`, doc, `cargo test
--workspace`. Coverage was not re-measured (`cargo llvm-cov` is not in
the container); every `commands/*.rs` module is now reached by at least
one test by construction.
