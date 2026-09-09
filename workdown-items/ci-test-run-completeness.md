---
id: ci-test-run-completeness
title: Fail CI when a test file exists but did not run
status: done
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

## Outcome (2026-09-09)

`.github/scripts/check-test-run.sh`, run by a new "Check every test
target ran" step after the test step in `ci.yml`. Both the UI build and
`cargo test --workspace` now `tee` their output to a log under
`shell: bash` (which brings `pipefail`, so the `tee` cannot mask a
failure).

What it checks:

- One unit binary per crate under `crates/`, matched by the package name
  in each `Cargo.toml` against `Running unittests src/(lib|main).rs
  (.../deps/<package>-<hash>)`.
- Every `crates/*/tests/*.rs`, matched by file name against
  `Running tests/<file> (`. The line does not name the crate and two
  crates can share a file name (`tests/set.rs` exists in both the CLI and
  core), so the script compares counts per name and reports which crates
  own the file when fewer runs than files were seen.
- The Vitest summary (`Test Files N passed`, `Tests N passed`) in the
  UI build log, after stripping the ANSI colour codes Vitest emits even
  when piped; zero or absent fails.
- Doc-test lines are ignored and cannot stand in for a missing target.

Verified in the dev container: the workspace log passes ("5 unit
binaries, 42 integration targets, 207 Vitest tests in 19 files"); a log
from a bare `cargo test` (which `default-members` scopes to the CLI
crate) fails with 31 problems, each naming the target that did not run;
an empty UI log fails. **Not yet verified on GitHub:** the "remove
`--workspace` on a branch" run needs a push, which waits for the branch
to be pushed at all.
