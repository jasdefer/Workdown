---
id: compute-type-support-mismatch
status: done
parent: misc-work
title: Fold `compute` and `pull` into the field-property table
---

## In plain words

Every "may this field type carry this setting?" question is answered
from one table in the schema model, except two. Whether a field may
have a `compute:` or a `pull:` line is decided by two hand-written type
lists in the schema parser instead. Two hand-written lists next to one
table is how entries drift; the fix is to give the table two more rows
and make the parser ask it, like it does for every other setting.

The five types that may be calculated (integer, float, date, duration,
boolean) stay the five. The only thing a user sees is that every
"is not valid for type" message now also says which types it is valid on.

## What was actually found (2026-09-09)

This item used to claim that the editor description
(`schema.schema.json`) and the Rust checker disagreed about computed
text and colour fields. They do not. The parser rejects `compute:` and
`pull:` on any other type before the checker runs
(`crates/core/src/parser/schema.rs`, "'compute' is only valid for
integer, float, date, duration, and boolean fields"), the schema guide
says the same, and the JSON schema forbids the same. The function the
original finding pointed at, `expression_type_of` in
`compute_check.rs`, maps text and colour to expression types so those
fields can be *referenced* in a condition (`status == "done"`), not so
they can be computed.

Why not allow computed text and colour anyway: the expression language
has no text operations — no joining, no formatting. The only text an
expression can yield is a literal or a copy of another field, which
`default` and `when:` already do better. Reopen when text operations
exist.

## Decisions taken

1. **Two new rows, one of them derived.** `FieldProperty` in
   `crates/core/src/model/schema.rs` gains `Compute` and `Pull`.
   `Compute` is a plain row on integer, float, date, duration and
   boolean. `Pull` is answered the way `Aggregate` already is: allowed
   exactly where `allowed_aggregate_functions` is `Some`, because pull
   applies the aggregate functions over a forward link, and a type that
   gains aggregate functions should gain pull without a second edit.
   The exhaustive `match` keeps a future type honest.
2. **One code path, one message shape.** The bespoke `type_supports_*`
   checks in `crates/core/src/parser/schema.rs` go. The generic property
   walk reports them, and its message grows a suffix listing the types
   the table allows, derived from the table: `'compute' is not valid for
   type 'choice' (valid on: integer, float, date, duration, boolean)`.
   Every type-restricted property gets the suffix, so the five type
   names are spelled out nowhere but the table. The two parser tests
   that pinned the old wording move to the new one. Rejected: keeping
   the old messages and skipping the two in the walk — that would
   reintroduce a hand-written list, and the old message text itself
   hard-coded the five names.
3. **`when` stays outside the table.** Its rejection on link/links is
   a reasoned rule with an explanatory message about phantom edges,
   not a type capability, and the JSON schema already enforces it via
   `dependentSchemas`. Only the `FieldProperty` doc comment changes.
4. **The probe covers them with one-liners.** The probe in
   `crates/core/tests/schema_schema.rs` only feeds the document to the
   JSON schema and asks `field_property_allowed` for the Rust answer;
   JSON Schema cannot resolve cross-field references, so
   `compute: other + 1` and a `pull:` mapping naming `parent` are
   enough. No second field, no builder change. The string form of
   `compute` sidesteps the `round` shape rule. Delete the comment that
   excludes them.
5. **The JSON schema follows its own rule completely.** Both shared
   type-list blocks at the end of `allOf` — the compute/pull rejection
   on seven types, and the "no `round` outside date" rule on integer,
   float and duration — fold into each type's own block, as the
   `$comment` asks. Remove the sentence in that comment pointing at
   this item.
6. **Docs unchanged.** `docs/schema.md` already states the five types.
   The probe scope note in [[view-kind-sync-guards]] gets a line saying
   the exclusion is lifted.

## Acceptance

- `field_property_allowed(type, Compute)` and `(type, Pull)` are the
  only places that know which types may be derived; no error message
  spells the type list out by hand.
- The probe test reports zero disagreements with `compute` and `pull`
  included.
- All CI gates green. The only user-visible change is the
  `(valid on: …)` suffix on "is not valid for type" messages.
