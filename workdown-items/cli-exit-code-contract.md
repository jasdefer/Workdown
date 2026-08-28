---
id: cli-exit-code-contract
title: Give the CLI one exit-code contract, and write it down
status: done
parent: maintenance-review-2026-08
---

## In plain words

The command-line tool answers with a number when it finishes, and
anything calling it reads that number to decide what happened. Twelve
of the thirteen commands used one convention for "you typed that
wrong"; one used another; and nothing anywhere said what the numbers
were supposed to mean. **Example:** the pre-commit hook workdown
installs into a user's repository decides whether to block a commit
purely from one of these numbers — so the convention is already
load-bearing for someone, while existing only in whichever code
happens to implement it.

## The inconsistency

Twelve commands are parsed by clap, which exits with the conventional
`2` on a malformed invocation. `workdown add` cannot use that path: its
flags are built from the project's schema, so it parses its own
arguments, and it returned `1` — the same code it returns when the work
itself failed.

Neither behaviour was documented, so there was no statement for the odd
one out to be inconsistent with.

## Decision taken

Three codes:

| Code | Meaning |
|---|---|
| `0` | The command did what was asked. |
| `1` | It ran, but the work failed or warned. |
| `2` | The invocation itself was malformed. |

Considered and rejected: collapsing to `0` and non-zero. Simpler, but it
throws away the one distinction a caller actually wants — "this project
has errors" is a result worth acting on, "this command line is wrong" is
a bug in the caller. Twelve of thirteen commands already drew that line,
which made the odd one out a one-place fix rather than a convention.

Also rejected: a fourth code separating "the project would not load"
from other failures. Anything needing that much detail should read
`--format json`, not the exit status.

## Acceptance

- `workdown add` with an unrecognised flag exits `2`; `--help` and
  `--version` still exit `0`.
- The three codes are stated once in `docs/architecture.md`, at the top
  of the four-exits section where they cover all four, rather than being
  implied by the two places that previously mentioned an exit code in
  passing.
- No other command's exit code changes.

## Not here

A test pinning the contract. Nothing currently runs the built binary at
all — that gap, and what to do about it, belongs to
[[testing-strategy]].
