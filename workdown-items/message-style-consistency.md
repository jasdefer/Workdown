---
id: message-style-consistency
status: to_do
title: One voice for validation messages
parent: maintenance-review-2026-08
depends_on:
- metric-row-check-unification
---

## In plain words

Validation messages are the product of a validate command, and the
codebase has a clear house style for them — but a few messages leak
programmer-formatted output at users (a list of allowed values printed
as `["open", "done"]` instead of `one of: open, done`), one message
uses backticks where everything else uses quotes, and two different
arrow characters are used for cycle chains. Small, but the
inconsistency sits inside a single error type and invites more of the
same.

## The problem in detail

The house style is lowercase prose with single-quoted identifiers
(`view '{id}', slot '{slot}': …`), with `where_check`'s
`describe_option_set` as the model for value lists
(`one of: done, in_progress, open`, with truncation —
`crates/core/src/where_check.rs:366-379`). Deviations, all in
`crates/core/src/model/diagnostic.rs`:

- **Debug formatting leaks.** `FieldValueError::InvalidChoice` renders
  `{allowed:?}` producing `["open", "done"]` (line 1199);
  `InvalidMultichoice` likewise (1202); `OutOfRange` /
  `OutOfRangeDuration` render `(min: Some(1.0), max: None)`
  (1205, 1210) — while `InvalidColor` in the *same enum* does it right
  with `allowed.join(", ")` (1222-1224).
- **Backticks vs quotes.** `ViewSlotCyclic` says
  ``must set `allow_cycles: false` `` (line 1038); everything else
  single-quotes.
- **Two arrow glyphs.** `Cycle` joins chains with `→` (line 917);
  `ComputeCycle` and `DeriveCycle` use ASCII `->` (lines 1149, 897).
  Pick one.

Depends on [[metric-row-check-unification]] only to avoid rewording
Display arms that item deletes; the `FieldValueError` fixes are
independent and can land first if convenient.

## Objective

Every user-facing message renders values the `describe_option_set`
way, identifiers single-quoted, one arrow glyph. Tests asserting on
message text are updated alongside — they pin the *new* wording.

## Out of scope

- Restructuring diagnostics; this is wording only.
