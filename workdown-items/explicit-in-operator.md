---
id: explicit-in-operator
type: issue
status: to_do
title: Explicit `in` operator; `=` becomes always-literal
parent: polish
depends_on: [view-filter-editor]
effort: "8h"
---

The filter grammar overloads `=`: `status=open` is an equality test, but
`status=open,in_progress` silently becomes an IN filter because the value
contains a comma. A user who types a literal value containing a comma
(`title=bug, crash` on a string field) gets a different filter than they
wrote, with no warning — the clause is valid, it just means something
else. The tell that this was accidental rather than designed: `!=` with a
comma is a *literal* comparison, so `=` and `!=` disagree about what a
comma means.

This issue makes list membership its own operator and returns `=` to
meaning exactly "equals".

## What we want

- `status in open,in_progress` — matches any of the listed values.
- `status not in open,done` — matches none of them.
- `=` and `!=` compare literally, commas included. `title=bug, crash`
  means the title *is* "bug, crash".
- The guided builder's multi-select produces `in` conditions; structured
  clauses carry the members as a list, not a comma-joined string.

## Acceptance

- A clause written with `in` / `not in` in `views.yaml` and the same
  filter built in the UI produce identical results (one grammar, one
  behavior — unchanged).
- `field=a,b` is a literal equality against the string `a,b`. On a
  choice/link field, cross-file validation flags it as an unknown value
  (save-with-warning), which is how a stale comma-IN clause surfaces.
- Round-trip: every guided condition, including multi-value `in`,
  survives structured → clause string → structured unchanged.
- This repo's own `views.yaml` and test fixtures are migrated to `in`.

## Out of scope

- Escaping/quoting inside `in` lists — a literal comma inside a single
  list member stays unrepresentable; the raw hatch covers it.
- Any other grammar change (regex, presence, ordering stay as they are).

## Design decisions (settled 2026-07-02)

- **No compatibility window.** Green-field: `=`+comma flips to literal in
  the same change that adds `in`. The only migration is this repo's own
  files. No dual grammar, no deprecation diagnostic.
- **Bare comma list, no brackets.** `status in open,in_progress`, not
  `status in [open, in_progress]`. Consistent with the grammar's existing
  comma, terser inside YAML strings; brackets add parser surface without
  making literal commas in members representable (only quoting would, and
  nothing needs it).
- **Negation is `not in`.** Reads naturally and keeps `!=` strictly
  literal, so the `=`/`!=` pair is symmetric. `!` stays reserved for the
  presence check (`!field?`).
- **Wire shape: new operators, `values` list.** `Operator::In` /
  `Operator::NotIn` plus a `values: Vec<String>` slot on `Condition`,
  with `value` reserved for scalar operators (validated per operator).
  One row type in the UI; the comma-join leaves our data model entirely.
  `in` desugars to the existing Or-of-equals predicate — no evaluator
  change; decompose folds that shape back to one `in` condition.
- **Offered for choice-like and link-like fields only.** `operators_for`
  adds `in` / `not in` to choice, multichoice, link, links — the types
  where "any of these known values" is the natural question and a picker
  exists. Elsewhere it stays reachable via the raw hatch.
- **UI labels: "is any of" / "is none of".** The multi-select moves from
  "is (=)" to "is any of"; "is (=)" on choice fields becomes
  single-select. Switching a row's operator between the scalar and list
  forms converts the value instead of carrying a comma string across.
