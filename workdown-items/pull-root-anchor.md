---
id: pull-root-anchor
title: A project-level start for unanchored roots, without a synthetic milestone item
status: to_do
parent: schema-expressions
depends_on: [pull-fields]
tags: [enhancement]
---

## In plain words

Forward scheduling ([ADR-011](../docs/adr/011-pull-fields-and-derive-graph.md))
anchors on hand-written `start_date` values: every item without
`depends_on` carries its own date. In a project where all roots start
on the same day — contract commencement, sprint start — that one date
is duplicated across every root and has to be edited in every file
when the start moves. For a bid whose commencement date is not known
yet, the only honest value is a placeholder, repeated N times.

## The workaround today, and what it costs

A synthetic milestone item (`project-start.md`, `duration: 0d`) that
every root depends on. It works, but:

- it forces a fake work item into a collection whose schema may require
  domain fields on every item (a deliverable reference, a section, a
  role), so the schema grows a `kind: milestone` escape hatch and a
  rule to restore those requirements for real items;
- the milestone shows up in every view — tree, board, gantt, workload,
  totals — as a zero-effort root;
- the date still lives in an item file rather than with the other
  project-level values in `resources.yaml`, next to the daily rates
  and the person-day convention.

## Objective

A root with no outgoing links on the scheduling field gets the
project's start without a hand-written date on each root and without a
fake item in the collection. The date changes in one place. A
hand-written value on a root still wins, as everywhere. The
all-or-nothing rule of ADR-011 stays: a *linked* item whose source
lacks a value is still "no value" (or an `error_on_missing`
diagnostic), never silently filled, because that is exactly the
partial answer the ADR rules out.

## Ideas, not decisions

Starting points the implementer can take or leave:

- **A fallback on the `pull` itself** for the no-links case, naming a
  constant: `fallback: $constants.project_start`, type-checked against
  the field type at load like other `$constants` references. With
  `aggregate` on the same field, it would fill leaves only, as the
  pull does. Narrow by construction: it never applies when a link
  exists.
- **Composition through `when` / `compute`** — a pulled
  `earliest_start`, then `start_date` chosen from it or the constant.
  Not expressible today: `then` takes literals only and the grammar has
  no "is set" test, so this route means growing the expression
  language first ([[when-then-value-expressions]],
  [[expression-logical-combinators]]).
- **A `default:` generator reading a constant** was considered and
  looks wrong for this: `default` writes a literal into the file at
  `workdown add`, which would permanently shadow the pull on every item
  it creates.
- **Keeping the milestone item** and only cutting its costs (a kind the
  views hide, a schema exemption) — honest to list, since it needs no
  new config, but it keeps a fake item in the collection.

Touches, whatever the route: the schema check for `pull`, the pull
evaluation, `schema.schema.json`, the schema editor's `PullBlock` in
the web UI, and the docs for `pull` and the scheduling recipe.
