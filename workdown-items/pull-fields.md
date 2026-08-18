---
id: pull-fields
status: done
title: Pull fields — cross-item derivation over forward links
parent: schema-expressions
depends_on: [computed-fields, aggregate-rollup]
---

A field can declare a `pull:` config that reads a *different* field
from the items a forward link points at and reduces the values:
`start` as `max` of `end` over `depends_on`. Combined with the
existing mechanisms this completes forward scheduling (the CPM
forward pass) from minimal manual input:

```yaml
start:
  type: date
  pull:
    over: depends_on
    field: end
    function: max
end:
  type: date
  compute: start + duration
```

Manual input is `depends_on` + `duration` everywhere, plus a
hand-written `start` on items with no dependencies (the anchors —
manual wins, derive fills absence, so roots need no special casing).
Transitivity emerges from recursion (b pulls from a, c pulls from b);
the pull itself is strictly one hop.

The three mechanisms now read: **aggregate** = same field, reverse
links, transitive; **pull** = other field, forward links, one hop;
**compute** = same item, cross-field.

## Scope

- `pull:` config block on a field definition — `over` (a `link` or
  `links` field, followed forward), `field` (the source field on the
  linked items), `function` (reusing the aggregate function set).
  Sibling of `compute` / `aggregate` / `when`; mutually exclusive
  with `compute` and `when` on the same field.
- Schema-level checks, compute_check-style (one diagnostic against
  `schema.yaml` disables the field; items stay quiet): `over` must
  name a link/links field with `allow_cycles: false`, `field` must
  exist, function/source-type/declared-type compatibility per the
  aggregate typing rules.
- **Unified derive engine.** Replace the per-field pass ordering in
  `store/derive.rs` with a dependency graph over (item, field) nodes:
  edges from compute references (same item), pull references (forward
  link targets), and aggregate contributions (reverse-link
  descendants). Evaluate in topological order. The graph owns
  *ordering only*; each mechanism keeps its own value semantics and
  diagnostics — load-bearing for aggregates, whose `count` /
  `average` / `median` reduce over transitive manual contributions,
  not direct-child values. `compute.rs` / `rollup.rs` value logic
  survives; only orchestration changes.
- Field-level cycles that are acyclic at the item level (`start` ↔
  `end` here) evaluate naturally in this graph. Genuine (item, field)
  cycles — including the union-graph case where two pull fields over
  two different link fields are only jointly cyclic — get one runtime
  diagnostic naming the items and fields on the loop; cycle nodes and
  their downstream stay absent, everything else evaluates.
- Items on an actual link cycle (already diagnosed by the cycle
  detector) are skipped by the pull without a second diagnostic.
- Missing inputs are all-or-nothing: any linked item lacking the
  source field → no value, silently; `error_on_missing: true` emits a
  diagnostic naming the linked item and field. Rationale: for every
  reduction a missing input can change the result, so a partial
  result is a silent guess — for `max(depends_on.end)` a too-early
  start that looks plausible. Empty or absent link field → value
  absent → manual anchor expected; `required: true` composes with the
  deferred required check to make unanchored roots a named error.
- Manual override wins silently, like compute. A pinned `start`
  earlier than a dependency's `end` is a rule-system concern
  (`start >= depends_on.end` as an optional warning rule), not a
  derive concern.
- Pull + aggregate on the same field mirrors compute + aggregate:
  pull fills only leaves of the aggregate's `over` hierarchy, the
  rollup fills everything above. A `depends_on` on a non-leaf does
  *not* feed its own pulled field — anything else would smuggle in
  dependency-inheritance semantics by accident; it stays meaningful
  for rules.
- Convention to document: `end = start + duration` is exclusive (a
  one-day task starting Jan 5 ends Jan 6), so a successor starting at
  `max(end)` does not double-book the last day.
- `schema.schema.json`: formal definition of the `pull` config.
- Commented example in `defaults/schema.yaml` next to the aggregate
  examples.
- ADR: the pull mechanism and the unified derive graph (supersedes
  the per-field ordering description in ADR-009's consequences).

## Decisions taken

1. **Mechanism:** new `pull:` config block — not an `aggregate`
   extension (transitive up-walk vs. one-hop forward read are
   different animals), not expression-language functions (keeps the
   algebra closed and same-item; arithmetic composition goes through
   named intermediate fields, e.g. lag:
   `earliest_start: pull(...)` + `start: compute: earliest_start +
   $constants.handover_lag`).
2. **Name:** `pull`. `lookup` (Airtable/Notion) means pulling
   *without* reduction there; `rollup` is taken by aggregate; `pull`
   encodes the distinguishing fact — direction.
3. **Engine:** unified (item, field) derive graph — one scheduler,
   three evaluators. Chosen over a special-cased interleave (two
   ordering regimes, mixtures must be forbidden) and demand-driven
   memoized recursion (elegant core, nondeterministic-shaped cycle
   diagnostics). Every mechanism composition works by construction.
4. **Cycles:** `allow_cycles: false` required on the pull's `over`
   field (schema error otherwise); runtime cycle members skipped
   with the cycle detector's diagnostic as the single cause.
5. **Missing inputs:** all-or-nothing, silent by default,
   `error_on_missing` opt-in — mirroring compute exactly.
6. **Manual override:** wins silently; conflict checks belong to the
   rule system.
7. **Aggregate composition:** pull on leaves only, rollup above —
   mirroring the compute + aggregate contract.
8. **Graph edges only where evaluation reads inputs** (review
   finding): settled items (hand-written value) wait for nothing, so
   a manual anchor breaks any dependency loop; non-leaves of a
   derive+aggregate field never wait for same-item/pull inputs. And
   `aggregate.over` now requires `allow_cycles: false` like `pull.over`
   — the graph starves on cyclic hierarchies the old walk tolerated,
   so the cycle detector must be guaranteed to cover them.

## Acceptance

- Chain a → b → c via `depends_on` with durations and a manual
  `start` only on `a`: query, table, and gantt show correct starts
  and ends for all three; no derived value appears in any file.
- A dependency pointing at a milestone whose `end` is aggregated
  from children (which themselves pull) resolves correctly — pull →
  rollup → compute chains across items.
- A dependency lacking `end` leaves the dependent's `start` absent;
  with `error_on_missing: true` the diagnostic names the dependency
  and the missing field; `required: true` on `start` flags unanchored
  roots via the deferred required check.
- Schema errors for: `over` not a link/links field, `over` with
  `allow_cycles: true`, unknown source field, function/type
  mismatch, `pull` combined with `compute` or `when`.
- A genuine (item, field) cycle gets one diagnostic naming the loop;
  unrelated items still evaluate.
- Existing aggregate semantics are unchanged by the engine refactor —
  regression coverage for `count` / `average` / `median` on multi-level
  hierarchies.

## Out of scope

- Backward pass (`latest_start`, slack, true critical path) — the
  natural follow-up; the derive graph is built to take it as another
  edge type.
- Working-day calendars; durations stay calendar-blind.
- `skip_missing: true` (reduce over present values) — add only if a
  real use case materializes; "count the done dependencies" is
  filtered counting, a different feature.
- Dependency inheritance (a parent's `depends_on` constraining its
  children).
- Inline lag syntax — the intermediate-field pattern covers it.
- Functions or link traversal in the expression language.
