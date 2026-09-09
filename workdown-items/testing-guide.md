---
id: testing-guide
title: Write the testing rules where a future change will meet them
status: done
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

## Outcome (2026-09-09)

A "Testing: three layers, one assertion per behaviour" section in
`docs/architecture.md`, placed before "Related reading": the layer
table, the one-sentence rule, the three-step "where a new test goes",
the per-layer case checklist from decision two, what is deliberately not
done, and one paragraph on the CI completeness check. It links to the
design item for the reasoning and repeats none of it. One line under
Conventions in `CLAUDE.md` points at the section.

Two clarifications the build items forced into the wording, recorded
here because they were not in the decisions as first written:

- A unit block that builds a `Store` from files in a temp directory
  only to feed an in-memory function is still a unit block. Without
  this, the location rule would have demanded moving the derive, rules,
  cycles, evaluator and checks blocks for no judgement benefit.
- The origin guard is asserted once for all, since it is one layer over
  the API, not once per endpoint.

`cargo doc --no-deps --workspace` with `-D warnings` and the docs drift
test still pass. [[cli-binary-tests]] and [[relocate-operations-tests]]
were built to these rules and needed no wording beyond the two points
above.
