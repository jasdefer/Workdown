---
id: expression-logical-combinators
status: to_do
title: "`and` / `or` / `not` in the expression grammar"
parent: schema-expressions
depends_on: [expression-predicates]
---

The expression grammar has comparisons but no logical combinators — a
deliberate v1 cut recorded in [[expression-predicates]]. Nothing is
inexpressible without them: every comparison has a complement operator,
so `when:` chains express conjunctions by bailing out on complements in
earlier branches, and disjunctions by two branches with the same `then`.

What combinators buy is ergonomics: `if: status != "done" and end_date <
$today` reads as one thought instead of two ordered branches, and rules
or future consumers of boolean expressions would not need branch
ordering as a workaround at all.

Addable without breaking any existing schema (new keywords, lower
precedence than comparisons; decide `not` spelling and precedence when
building). Wait for authoring friction — three-branch colour configs
have not produced any yet.
