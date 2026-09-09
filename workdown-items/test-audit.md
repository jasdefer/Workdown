---
id: test-audit
title: Audit every test block against the rules, delete what fails them, move what is misplaced
status: to_do
parent: testing-strategy
depends_on: [cli-binary-tests]
---

## In plain words

Decisions one and two of [[testing-strategy-design]] say what a test is
for and which cases each layer gets. The existing suite was written
before either existed. This item walks the suite against them, largest
block first, and does three things: deletes tests that fail a rule,
moves tests that sit in the wrong place, and counts before and after so
the milestone can say whether it ended with fewer or better tests than
it started with. **Example:** `crates/core/src/operations/set/mod.rs`
has 16 lines of product code and 1,202 lines of tests that write a temp
project and assert on files. Those are integration tests by our own
definition, so they move to `crates/core/tests/`. Nothing about them
is wrong except the address.

## What the inventory already found

- **Justified, leave alone:** the large input-space blocks — schema
  parser, `coerce`, query evaluator, views parser, `compute_check`,
  `where_check`, expression typechecker, duration, gantt placement,
  derive, rollup, cycles. Large because their input spaces are.
- **Relocate, about 3,500 lines:** the in-file tests under
  `operations/set/mod.rs`, `rename.rs`, `view_write.rs`, `body.rs`,
  `frontmatter_io.rs`, `install_hooks.rs`. Integration tests living in
  the file. Each operation is tested once, so this is a move, not a
  dedup.
- **Look first for excess:** `view_data/gantt_by_initiative.rs` and
  `gantt_by_depth.rs` (about four test lines per product line); the 47
  tests in `crates/server/tests/git_endpoint.rs` (any that assert on
  repository state rather than the HTTP contract); unit tests of glue in
  the `operations/set` submodules.
- **Leave alone:** the git crate's tests. Preview and scope are shared by
  server and CLI and are tested in the crate; commit, pull and push are
  server-only and are tested through the endpoint.

## The rules applied (from decision two)

Delete a test when:

- it is a unit test of glue with one input shape, and an integration
  test in a `tests/` directory runs the same path;
- it restates an assertion a `tests/` file already makes;
- it is a server test asserting on file contents rather than status,
  JSON shape or origin guard;
- it proves the same status code or the same error class as another
  test, for a different input.

Every deletion names, in the commit message, the test that still covers
the behaviour, or states that the behaviour has no user-visible outcome.
A deletion is made because a rule fails, never to reach a number.

## Order of work

1. Record the "before" counts with the method in the design item's
   baseline section.
2. Relocate the operations blocks first. Pure moves, easy to review,
   and they make the in-file numbers honest before any judgement about
   them.
3. Then down the ranked list, one file per commit, largest first.
4. Record the "after" counts the same way, in [[testing-strategy]].

## Watch out

Coverage in transit: a relocated block that loses a case on the way is
the risk the "forward-only" leaning was guarding against. Move whole
blocks, run the full workspace gate after each, and only then edit.

## Done when

- No in-file test block writes a temp project; those all live in
  `tests/`.
- Every deletion is justified against a named rule in its commit.
- Before and after counts are in the milestone, taken the same way.
