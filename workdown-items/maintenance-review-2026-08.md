---
id: maintenance-review-2026-08
title: 'Maintenance pass: findings from the 2026-08 codebase review'
status: done
---

## In plain words

A full review of the codebase in August 2026 (six independent review
passes: core domain, validation, query/views, CLI, server + web UI,
architecture) came back with a unanimous verdict: the architecture is
sound and no major refactor is warranted. What it found instead is a
contained cleanup backlog — this milestone collects it.

The common thread across almost every finding: **a handful of facts are
written down in more than one place** — a default value, a list of
auto-fill mechanisms, a sort order, a validation rule, a formatting
rule. Each duplication works today; each is a place where two copies
can silently drift apart tomorrow (two already have). This milestone is
mostly "make each fact live in exactly one place", plus paying down
documentation debt.

## Suggested order

The items are largely independent, but a sensible sequence:

1. **Behavior first** — the three findings where users can already see
   wrong output: [[derived-field-single-predicate]],
   [[view-order-in-extractor]], [[over-default-single-definition]].
2. **Deduplication** — [[validation-phase-boundaries]] (added
   mid-milestone: deciding where the required-field check lives, and
   removing the hand-mirrored mechanism list behind
   [[derived-field-single-predicate]]), [[metric-row-check-unification]],
   [[query-value-consolidation]], [[chart-renderer-sharing]],
   [[schema-property-table]], [[message-style-consistency]].
3. **Documentation** — [[web-layer-adr]], [[render-flow-doc]],
   [[stale-docs-refresh]].
4. **Guards and tests** — [[view-kind-sync-guards]],
   [[stateful-test-gaps]], and [[cli-exit-code-contract]], added
   mid-milestone once the CLI's exit codes turned out to be an
   undocumented contract that the generated pre-commit hook already
   depends on.
5. **Anytime** — [[assorted-small-fixes]] is a grab bag of
   minutes-to-hours fixes to fold into any of the above.

Explicit dependencies exist only where two items touch the same code
and one would redo the other's work; they are recorded as `depends_on`
on the items themselves.

## Not in this milestone

[[project-load-cache]] — the server's reload-everything-per-request
design is a deliberate tradeoff that only becomes a problem at scale.
It is tracked as a standalone watch item with an explicit trigger, so
it does not block this milestone.
