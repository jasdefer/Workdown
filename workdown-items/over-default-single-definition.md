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

## Scope question to settle first

The item above treats this as a deduplication: the literal `"parent"`
is written in three places, so write it once. A prior review round
raised a larger question that should be answered before any of that.

The fallback assumes the project's schema contains a field literally
named `parent`. That is a field name driving behavior — the one thing
this project reserves for `id` alone. It is not silently magic: a
schema with no `parent` field and a roll-up that omits `over` fails to
load with a clear message. But it is the single place where a name
carries meaning.

Three ways to go, to be decided rather than assumed:

1. **Keep the default, define it once.** The item as originally
   written. Smallest change; the name-based fallback stays.
2. **Take the fallback from the config's declared hierarchy field**
   (`defaults.tree_field`). No hardcoded name, but `schema.yaml` stops
   being readable on its own — you would need `config.yaml` open to
   know what a roll-up climbs — and schema parsing currently does not
   read the config at all.
3. **Drop the default; require every roll-up to name what it climbs.**
   No magic name anywhere and the schema stays self-explanatory. Costs
   a little boilerplate and breaks any existing consumer schema that
   omits `over`. Every roll-up example in the shipped default schema
   already writes `over` explicitly, so the convenience is barely
   used. Pre-1.0 is the cheap moment for it.

Under option 3 the duplicated literal disappears rather than needing a
home, so the original objective is only reached through options 1
or 2.
