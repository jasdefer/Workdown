---
id: testing-guide
title: Write the testing rules where a future change will meet them
status: to_do
parent: testing-strategy
---

## In plain words

Decisions one and two of [[testing-strategy-design]] say what each test
layer is for, which cases each gets, and where a new test goes. Right
now they live in a work item, which nobody reads before adding a test.
This item puts them in the two places that are read: a short section in
`docs/architecture.md`, and a one-line pointer in `CLAUDE.md`.
**Example:** someone adds a `--dry-run` flag to `set`. The guide tells
them, without re-reading the milestone: one core integration test for
the behaviour, one CLI binary test that the flag arrives, and a unit test
only if the flag introduces a function with more than one input shape.

## What goes in `docs/architecture.md`

One section, kept short enough to read in the moment:

- The three layers as a table: question each answers, where it lives.
- The one-sentence rule: a behaviour is asserted once, in the crate that
  owns it, at that crate's public entry point; layers above assert only
  what they add.
- Where a new test goes, in the three-step form from decision one.
- The case rules per layer from decision two, as a checklist.
- What is deliberately not done: browser tests, coverage as a gate.

Not the reasoning. That stays in the design item, and the section links
to it.

## What goes in `CLAUDE.md`

One line under Conventions pointing at the section, so the rule is in
front of every change made with the tool.

## Done when

- The section exists and `cargo doc` and the docs checks still pass.
- Nothing in it duplicates the design item's reasoning; it states rules
  and links back.
- [[cli-binary-tests]] and [[test-audit]] follow it, which is the test
  of whether it is decidable in the moment. Adjust the wording if they
  could not.
