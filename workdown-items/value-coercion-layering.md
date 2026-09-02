---
status: done
parent: misc-work
title: Move value coercion out of the store to break the parser↔store cycle
tags: [tech-debt]
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

## Outcome (verified 2026-08-31)

Done — the move landed inside the [[maintenance-review-2026-08]] PR
(`03508c8`) rather than as its own change, which is why this item was
never closed. `crates/core/src/store/coerce.rs` is now
`crates/core/src/coerce.rs`; `parser/schema.rs:10` imports
`crate::coerce::{coerce_value, yaml_type_name}`, and the parser has no
dependency on `store` at all any more. The motivating case holds: a
schema-only tool would pull in the parser it needs and none of the item
store.

One smaller edge remains, recorded rather than reopened: `coerce.rs:24`
imports `crate::parser::RawWorkItem` for `coerce_fields`, while the
parser imports `coerce_value` back — so `parser` and `coerce` are now
mutually dependent where `parser` and `store` used to be. It is a much
smaller knot (one struct, one function) and `coerce_value` itself is
parser-free, so splitting the raw-item entry point off would settle it
if it ever gets in the way.
