---
id: ci-test-run-completeness
title: Fail CI when a test file exists but did not run
status: to_do
parent: testing-strategy
---

## In plain words

CI was green for weeks while the core and server suites — most of the
tests — never ran, because `cargo test` was scoped to one crate
([[ci-workspace-coverage]]). Every test that did run passed, so the
checkmark was honest about what it saw and silent about what it did not.
This item makes that class of failure loud. **Example:** if someone
scopes the job to one crate again, the build fails with "tests/add.rs
exists but did not run" instead of turning green.

## How (decision four of [[testing-strategy-design]])

A short script step after `cargo test --workspace` in `ci.yml`:

1. List the expected test targets from the tree: one unit binary per
   crate in `crates/`, plus every `crates/*/tests/*.rs` file.
2. Parse the `Running ...` lines from the captured `cargo test` output.
3. Fail, naming each expected target that did not appear.
4. Assert the Vitest step ran and reported a non-zero test count.

The expected list is derived from the repository, so it needs no
maintenance as files are added or removed. No coverage tool, no fixed
test count.

## Watch out

- `cargo test` prints target paths relative to the crate; match on the
  file name and the crate, not the full path.
- A doctest binary appears too; ignore it or include it deliberately,
  but do not let it mask a missing `tests/*.rs`.
- Pipe nothing through `tail` or `grep` that could swallow the exit code
  of `cargo test` itself. Capture to a file, check the exit code, then
  read the file.

## Done when

- Removing `--workspace` from the CI test step makes the job fail with
  the missing targets named. Verified once on a branch, then reverted.
- The check runs in under a few seconds and adds no new tooling.
