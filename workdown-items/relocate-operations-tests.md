---
id: relocate-operations-tests
title: Move the in-file operations tests to core's tests directory
status: done
parent: testing-strategy
depends_on: [cli-binary-tests]
---

## In plain words

Six files under `crates/core/src/operations/` carry integration tests
at the bottom of the file: they write a temp project and assert on the
files it leaves behind. By decision one of [[testing-strategy-design]]
those belong in `crates/core/tests/`, next to the add, init, templates
and validate tests that already live there. **Example:**
`operations/set/mod.rs` is 16 lines of product code followed by 1,202
lines of tests. Nothing about the tests is wrong except the address.

This is a pure move. No test is deleted, no assertion is changed. It is
split from [[test-audit]] so that the move is reviewed on its own and
the in-file counts are honest before any judgement about them.

## The blocks

| File | Test lines |
|---|---|
| `operations/set/mod.rs` | 1,202 |
| `operations/view_write.rs` | 1,001 |
| `operations/rename.rs` | 523 |
| `operations/body.rs` | 323 |
| `operations/frontmatter_io.rs` | 282 |
| `operations/install_hooks.rs` | 169 |

About 3,500 lines. The `set` submodules (`collection`, `numeric`,
`temporal`, `boolean`) are compute logic with an input space; their
tests are unit tests and stay where they are.

## How

- One file per commit, target `crates/core/tests/<operation>.rs`.
- A test that reaches a private function cannot move as-is. Either it is
  really a unit test of an input-space function and stays, or it is
  rewritten against the public operation. Record which, per test, in
  the commit.
- Reuse the fixture pattern the existing `tests/` files use: config and
  schema strings written to a temp directory.
- Run the full workspace gate after each move, before touching the next
  file.

## Watch out

Coverage in transit is the one risk. Count the `#[test]` attributes in
the block before the move and in the new file after it; the numbers
match or the commit says why.

## Done when

- No file under `crates/core/src/operations/` has a test that writes a
  temp project.
- Test count before equals test count after, per file, or the difference
  is explained.
- [[test-audit]] can start from honest in-file numbers.

## Verdicts per file (2026-09-09)

| File | Before | Moved to `crates/core/tests/` | Stayed | Why the rest stayed |
|---|---|---|---|---|
| `operations/set/mod.rs` | 23 | 23 → `set.rs` | 0 | — |
| `operations/set/{collection,numeric,temporal,boolean}.rs` | 42 | 42 → `set.rs` | 0 | Every test drives the public `run_set` against a temp project; the item's claim that these were unit tests was wrong on inspection. `set/test_support.rs` had no remaining user and is deleted. |
| `operations/view_write.rs` | 45 | 45 → `view_write.rs` | 0 | — |
| `operations/rename.rs` | 19 | 18 → `rename.rs` | 1 | `line_contains_id_boundaries` is a unit test of a private input-space helper. The moved tests carry a test-local copy of that boundary rule for their "no stale references" assertions. |
| `operations/body.rs` | 10 | 10 → `body.rs` | 0 | — |
| `operations/install_hooks.rs` | 13 | 4 → `install_hooks.rs` | 9 | The nine test `hook_script` / `manual_hook_line`, pure string functions with an input space (mode × hostile paths). The four that write a hooks directory through `install_pre_commit` moved. |
| `operations/frontmatter_io.rs` | 19 | 0 | 19 | Misdescribed in the inventory: no test writes a project. They are unit tests of `build_frontmatter_yaml`, `write_file_atomically` and `parse_value_for_field`, each an input-space function. |
| `operations/templates.rs` | 7 | 0 | 7 | Not moved: `tests/templates.rs` already exists with the same cases. Left for [[test-audit]] as a duplication question, not a location one. |

Total: 178 tests before, 107 moved, 71 stayed, 0 lost. `cargo test -p
workdown-core` and `cargo clippy -p workdown-core --all-targets` green
after the move. Mechanical move with one correction: the first pass
dedented the YAML inside string constants along with the code and broke
every fixture; redone with a string-aware dedent.

A counting artifact for the audit's "before" numbers: the method counts
from a file's first `#[cfg(test)]` to its end, and `set/mod.rs` used to
open with `#[cfg(test)] mod test_support;` on line 17, so its baseline
figure of 1,202 "test lines" included about 580 lines of product code.
