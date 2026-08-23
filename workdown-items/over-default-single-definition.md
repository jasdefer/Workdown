---
id: over-default-single-definition
status: to_do
title: Define the "parent" roll-up default exactly once
parent: maintenance-review-2026-08
---

## In plain words

When a roll-up field does not say which relation to roll up over, it
defaults to `parent`. That default is hardcoded as the literal text
`"parent"` in three unrelated files — one of them in the web server,
outside the core crate entirely. If the default ever changes, all
three spots must be found by memory; miss one and different parts of
the tool quietly use different defaults. Define it once, reference it
everywhere.

## The problem in detail

Three independent spellings of the same fact:

- `crates/core/src/parser/schema.rs:1061` —
  `agg.over.as_deref().unwrap_or("parent")`, used for validation.
- `crates/core/src/store/rollup.rs:32` —
  `pub(super) const DEFAULT_OVER_FIELD: &str = "parent"`, used for the
  roll-up walk.
- `crates/server/src/api/timer.rs:327` —
  `aggregate.over.as_deref().unwrap_or("parent")`, used to decide
  whether starting a timer needs the roll-up-override confirmation.

The model documentation (`crates/core/src/model/schema.rs:439`) even
says the default is applied "at use sites" — documenting the hazard
rather than removing it. The server copy is the dangerous one: a
change to core's default would silently miss it, and the timer's
confirmation dialog would diverge from the actual roll-up behavior.

## Objective

Preferred: resolve `over` once at schema parse time, so the parsed
model carries a plain `String` and no use site ever defaults again
(for example an `over_or_default()` on `AggregateConfig`, or making
the parsed field non-optional). At minimum: one exported constant that
all three sites reference.

While in the area, consider moving `rollup_confirmation_needed` (the
server-side check around `api/timer.rs:327`) into core next to the
aggregate machinery — it is roll-up domain logic living in an HTTP
handler, and relocating it removes the duplicated literal for free.

## Out of scope

- Changing what the default *is*.
