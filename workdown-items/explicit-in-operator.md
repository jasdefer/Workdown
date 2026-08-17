---
id: explicit-in-operator
status: done
title: Explicit `in` operator; `=` becomes always-literal
parent: polish
depends_on: [view-filter-editor]
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
- `not in` and `!=` agree with each other on items where the field is
  absent — which today they would not. See the absent-field decision
  below.

## Acceptance

- A clause written with `in` / `not in` in `views.yaml` and the same
  filter built in the UI produce identical results (one grammar, one
  behavior — unchanged).
- `field=a,b` is a literal equality against the string `a,b` — on every
  field type, matching `!=`.
- `status not in done,removed` and `status != done` return the same
  verdict for an item carrying no `status`, and `status?` added as a
  second clause excludes it again.
- Round-trip: every guided condition, including multi-value `in`,
  survives structured → clause string → structured unchanged, arity-1
  lists included.
- This repo's own `views.yaml` and test fixtures are migrated to `in`.

## Out of scope

- Escaping/quoting inside `in` lists — a literal comma inside a single
  list member stays unrepresentable; the raw hatch covers it.
- Any other grammar change (regex, presence, ordering stay as they are).
- Validating clause *operands* against a field's value set, so that a
  stale `type=milestone,epic` surfaces as a diagnostic rather than
  quietly matching nothing. Wanted, but independent of this grammar
  change — see [[where-clause-value-validation]].

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

## Design decisions (settled 2026-07-29)

- **An absent field makes negative comparisons true, positive ones
  false.** Today one blanket early return answers *every* value
  comparison `false` when the field has no value, so `status != done`
  excludes an item carrying no status. That makes the two ways of
  building `not in` disagree — `NOT (a OR b)` admits the item, `not-a AND
  not-b` rejects it — and shipping either one leaves `not in`
  inconsistent with `!=`, which is the bug class this issue exists to
  close. So the rule changes for negative operators: an item with no
  `status` matches `status != done` and `status not in done,removed`. Of
  the existing operators only `!=` is affected; `contains`, `matches` and
  the ordering comparisons are all positive and keep answering false.

  Two things make this the right direction rather than a widening of
  scope. The current behavior looks incidental — it falls out of a single
  early return placed before per-type dispatch, and the only test
  covering it uses `>`, a positive operator. And it is the reading that
  keeps both meanings reachable: `where:` clauses are AND-combined with
  no OR available, so under this rule "exclude removed but keep unset" is
  one clause and "exclude both" is two (`status != removed` + `status?`),
  whereas the opposite rule leaves the first case inexpressible.

  Not adopting SQL's three-valued logic, where a NULL operand makes both
  `=` and `!=` fail. Its premise — a value exists but is unknown — isn't
  what an unwritten frontmatter field means, and the failure mode is
  wrong for a planning tool: nearly every real filter here is a negative
  exclusion, so items missing a field would silently drop out of every
  active-work view. It is also not a one-branch change — "unknown" would
  have to propagate through And / Or / Not, which return plain booleans.

- **`not in` desugars to And-of-not-equals.** With the absent-field rule
  above, `NOT (a OR b)` and `not-a AND not-b` are equivalent, so this is
  a code-shape choice: the And form is structurally symmetric with the
  existing Or-of-equals fold, one flat same-field fold per direction,
  rather than a nested `Not(Or(…))` pattern. `decompose_clause` gains the
  mirror fold; it currently gives up on `Predicate::And` outright.

- **Symbolic operators keep parse priority; `in` is matched only after
  they all miss.** The parser splits on the first operator token it
  finds, and `in` is the first token made of letters — which appear in
  field names and values. `title=a in b` must split at `=` (field
  `title`, value `a in b`), not at ` in ` (field `title=a`). The token is
  whitespace-delimited — a field named `sprint` contains the letters
  `in` — and ` not in ` is tested before ` in `, or `status not in done`
  splits into the field name `status not`. One spelling only, lowercase.
  An empty list (`status in `) is a parse error. Accepted consequence:
  `title in review` on a string field now reads as membership instead of
  erroring.

- **One shared `Operator` enum.** `In` / `NotIn` are legal on the wire
  and in `operators_for`, but never reach the evaluator — the parser
  rewrites them before evaluation. The evaluator marks them unreachable
  in its per-type comparisons, exactly as it already does for `is_set` /
  `is_not_set`. The alternative — separate evaluator and wire enums with
  a conversion — removes an internal-only impossibility at the cost of a
  second type, a second generated TypeScript type, and a mapping to keep
  in sync.

- **A payload mismatch is a hard reject at the write endpoint.** `In`
  carrying `value`, or `Equal` carrying a non-empty `values`, is a
  malformed request from our own UI rather than a user-authored file
  problem, so it fails the write instead of riding through as a warning.

- **`in` always produces the n-ary predicate, arity 1 included.**
  A single-member list must not shortcut to a bare equality comparison,
  or `status in open` round-trips back as `=` and silently downgrades the
  operator.

## As built — where it departed from the plan

- **`list` does not get `in`.** The plan named choice, multichoice, link and
  links; `list` shares a `operators_for` arm with the other two collections and
  would have come along for free. It is excluded: a `list` holds free-form
  strings, so there is no known value set to pick members from, which was the
  whole basis for restricting the offer. It stays reachable via the raw hatch.
- **Link-like fields needed their own picker.** Offering `in` on `link` /
  `links` while the multi-select only covered choice-like types left those
  fields rendering a single-select bound to the scalar slot — a row that could
  never be complete and was silently dropped on save. They now render a
  multi-select list box (a list, not checkboxes, because the option set is
  every item id).
- **The word token needs a boundary on *both* sides.** The plan called for a
  leading space, which stops `sprint` from being split. It does not stop
  `status in_progress` — no operator at all — from parsing as membership, so
  the token must also be followed by whitespace or end-of-input. Accepting
  end-of-input is what lets `status in` report a missing value list instead of a
  generic parse failure.
- **Empty and partial lists are a parse error** (`EmptyValueList`), including a
  trailing comma. Neither can ever match, so both are typos rather than intent.

## Migration surface

- `.workdown/views.yaml` — 5× `type=milestone,epic` → `type in milestone,epic`
- `crates/core/tests/views_schema.rs`, `crates/server/tests/views_write_endpoint.rs`
- `parse_where`'s doc table in `crates/core/src/query/parse.rs` — the IN row
- `crates/core/src/query/clause.rs` module doc and `serialize_condition` doc,
  both of which describe the comma-IN fold as intentional
- `docs/views.md` — the "inline `=a,b,c` form" note, plus a new statement of
  the absent-field rule, which the file does not currently document
