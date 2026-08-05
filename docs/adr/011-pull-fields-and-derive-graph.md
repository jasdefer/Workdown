# ADR-011: Pull fields and the unified derive graph

**Status:** Accepted
**Date:** 2026-08-05

## Context

Computed fields (ADR-009) derive within one item; aggregated fields
(ADR-003) derive the same field up reverse links. Neither can express
forward scheduling — `start = max(end of the items depends_on points
at)` — which needs a *different* field read through a *forward* link.
And the combination `start` (cross-item) ↔ `end = start + duration`
(same item) is cyclic at the field level while acyclic at the item
level, which the per-field pass ordering in the derive orchestrator
structurally could not evaluate.

## Decision

- Fields declare `pull: {over, field, function, error_on_missing}` —
  read `field` from the items the `over` link (a link/links field with
  `allow_cycles: false`) points at, one hop forward, and reduce with an
  aggregate function. Transitivity emerges from recursion, never from
  walking. A new config block, not an expression function: the
  expression algebra stays same-item, and arithmetic composition (lag,
  offsets) goes through named intermediate fields. Mutually exclusive
  with `compute` and `when`; the mechanisms now read *aggregate = same
  field, reverse links, transitive; pull = other field, forward links,
  one hop; compute/when = same item*.
- The derive orchestrator schedules one unified dependency graph over
  (item, field) nodes — edges from compute/when references (same
  item), pull sources (forward links), and aggregate children (reverse
  links) — evaluated as a deterministic Kahn walk. The graph owns
  ordering only; value semantics and diagnostics stay with the
  mechanisms, so aggregates still reduce over transitive bearer
  contributions (`count`/`average`/`median` count bearers, not
  already-reduced children). This supersedes ADR-009's per-field
  evaluation order, which survives as the walk's tie-breaking priority.
- Missing inputs are all-or-nothing: any linked item lacking the source
  field means no value (silent by default, `error_on_missing` opts into
  a diagnostic naming `target.field`) — a partial reduction is a
  silently-wrong answer, worst of all for `max` in scheduling. Items
  without outgoing links carry manual anchors; `required: true` turns
  an unanchored root into a named error via the deferred required
  check.
- Manual values win silently, as everywhere. Pull + aggregate mirrors
  compute + aggregate: pull fills leaves of the rollup hierarchy, the
  rollup fills everything above — a `depends_on` on a non-leaf does not
  feed its own pulled field (dependency inheritance stays a non-goal).
- Graph edges exist only where evaluation will actually read the
  input: an item whose file already carries the field is settled and
  waits for nothing — so a hand-written anchor breaks any dependency
  loop — and non-leaves of a derive+aggregate field never wait for the
  same-item or pull inputs the rollup makes irrelevant. The plan and
  the evaluator share one eligibility rule and cannot drift.
- Cycles: nodes on a genuine, unanchored dependency cycle stay
  unevaluated. A cycle within one link field is the link cycle
  detector's finding — guaranteed by construction, since both `pull`'s
  and `aggregate`'s `over` link must declare `allow_cycles: false`
  (the latter is a new schema requirement introduced here; the old
  walk-based rollup tolerated cyclic hierarchies, the graph starves on
  them). A loop only the combination of link fields produces
  (jointly-cyclic link graphs) gets its own item diagnostic naming the
  `item.field` chain.

## Consequences

- Forward scheduling (the CPM forward pass) is schema configuration:
  `depends_on` + `duration` as input, `start`/`end` derived, gantt
  views light up from dependencies alone.
- The backward pass (`latest_start`, slack, critical path) and
  working-day calendars remain future work; the graph is built to take
  the former as just another edge type.
- Pull config errors are schema diagnostics that disable the one field
  (compute_check standard), not load failures.
