---
type: issue
status: to_do
parent: misc-work
title: Move value coercion out of the store to break the parser↔store cycle
effort: "3h"
tags: [tech-debt]
color: pink
---

## In plain words

Think of the code as departments: the department that reads
`schema.yaml` now borrows a tool from the department that stores work
items — but the storage department already relies on the schema
department. Neither can be understood or replaced without the other,
like two chapters of a manual that each say "see the other chapter
first". **Example:** if we ever wanted a small standalone tool that
only reads schemas (say, a syntax checker for editors), it would drag
the entire item-storage machinery along just because of this one
borrowed tool. Moving the tool to a neutral shared shelf fixes the
circle.

[[conditional-field-value]] made `store::coerce::coerce_value`
`pub(crate)` so `parser/schema.rs` can coerce `when:` `then:` literals
into the field's type. The store already depends on the parser
(`store/mod.rs` uses `crate::parser`), so this created a module cycle:
neither module can now be understood or extracted without the other.

Coercing a scalar into a `FieldValue` is value-level logic, not store
logic — the store is merely its heaviest user.

## Scope

- Move `coerce_value` (and whatever helpers it drags along) to a
  value-level home near `model::FieldValue`, so both the parser and the
  store depend downward on the model instead of on each other.
- Pure relocation — no behavior change, no signature redesign beyond
  what the move forces.
