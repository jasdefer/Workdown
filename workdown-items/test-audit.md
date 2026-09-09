---
id: test-audit
title: Audit every test block against the rules, delete what fails them, move what is misplaced
status: to_do
parent: testing-strategy
depends_on: [relocate-operations-tests]
---

## In plain words

Decisions one and two of [[testing-strategy-design]] say what a test is
for and which cases each layer gets. The existing suite was written
before either existed. This item walks the suite against them, largest
block first, and does two things: deletes tests that fail a rule, and
counts before and after so the milestone can say whether it ended with
fewer or better tests than it started with. **Example:**
`view_data/gantt_by_initiative.rs` has 123 lines of product code and
499 lines of tests. Either each of those tests is a distinct placement
or grouping shape, and they stay, or some restate what the gantt tests
below them already prove, and those go. Reading them against the rule
is the work.

The pure moves — the in-file operations tests that belong in
`crates/core/tests/` — are [[relocate-operations-tests]], which runs
first so this item starts from honest in-file numbers.

## What the inventory already found

- **Justified, leave alone:** the large input-space blocks — schema
  parser, `coerce`, query evaluator, views parser, `compute_check`,
  `where_check`, expression typechecker, duration, gantt placement,
  derive, rollup, cycles. Large because their input spaces are.
- **Already moved by [[relocate-operations-tests]]:** the in-file
  operations integration tests, about 3,500 lines. Not this item's
  concern beyond confirming nothing was lost.
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
   baseline section. The baseline was taken before the relocation, so
   the per-layer split has shifted; the totals have not.
2. Down the ranked list, one file per commit, largest first. For each
   block: name the function under test, decide input-space or glue,
   then apply the four rules test by test.
3. Record the "after" counts the same way, in [[testing-strategy]].

## Watch out

A deletion that looks like a restatement can be the only test of one
corner the `tests/` file never sends through. Before deleting, find the
integration test that covers the same path and check it reaches the same
branch. If it does not, the test stays, or the case moves into the
integration test first.

## Done when

- Every block in the ranked list has been read against the rules, and
  the item records the verdict per file, one line each.
- Every deletion is justified against a named rule in its commit.
- Before and after counts are in the milestone, taken the same way.
