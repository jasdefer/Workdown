---
id: expression-logical-combinators
status: to_do
title: "`and` / `or` / `not` in the expression grammar"
parent: schema-expressions
depends_on: [expression-predicates]
---

## In plain words

Conditions in the project configuration can test one thing at a time
but cannot say "this **and** that" in a single line, which forces
readers and authors into awkward workarounds.

Nothing is impossible today — you chain several separate tests in the
right order and it works — but that reads badly and the order is easy
to get wrong. Adding the words "and", "or" and "not" would let a
condition be written the way a person would say it out loud.
**Example:** "colour the item red if it is not finished and its end
date has passed" is one thought for a human, but currently has to be
split into two rules that only work in a particular sequence. This is
a comfort improvement rather than a missing capability, so it waits
until someone genuinely finds the current way annoying.

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
