---
id: relocate-operations-tests
title: Move the in-file operations tests to core's tests directory
status: to_do
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
