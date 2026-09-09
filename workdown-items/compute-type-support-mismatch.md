---
id: compute-type-support-mismatch
status: to_do
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

No behaviour changes for users. The five types that may be calculated
(integer, float, date, duration, boolean) stay the five; the messages
stay as they are.

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

1. **Two new rows.** `FieldProperty` in `crates/core/src/model/schema.rs`
   gains `Compute` and `Pull`; `allowed_field_properties` lists them on
   integer, float, date, duration and boolean, nowhere else. The
   exhaustive `match` keeps a future type honest.
2. **The parser asks the table.** The two `type_supports_*` checks in
   `crates/core/src/parser/schema.rs` call `field_property_allowed`
   instead of carrying their own type lists. The error wording stays
   exactly as it is — the parser tests at the "only valid for" messages
   pin it.
3. **The probe covers them for free.** `crates/core/tests/schema_schema.rs`
   walks every `FieldProperty` variant against the JSON schema, so the
   two rows are probed once they exist. Add a `representative_value`
   for each (a well-formed `compute: other + 1`-style expression needs
   a referenced field; a `pull:` needs a link field and a source — the
   probe document builder may need a second field). Delete the comment
   that excludes them.
4. **The JSON schema follows its own rule.** The shared `if`/`then`
   block that forbids `compute` and `pull` on seven types becomes
   `"compute": false, "pull": false` inside each of those types' own
   blocks, as its `$comment` asks ("keep one block per type"). Remove
   the sentence in that comment pointing at this item.
5. **Docs unchanged.** `docs/schema.md` already states the five types.
   Widen the probe note in [[view-kind-sync-guards]] only if it names
   the exclusion.

## Acceptance

- `field_property_allowed(type, Compute)` and `(type, Pull)` are the
  only places that know which types may be derived.
- The probe test reports zero disagreements with `compute` and `pull`
  included.
- All CI gates green; no user-visible change.
