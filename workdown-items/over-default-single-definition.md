---
id: over-default-single-definition
status: done
title: Make every roll-up name the relation it climbs
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

## Decisions taken

**1. The `parent` default is removed, not relocated.** `aggregate.over`
becomes mandatory: a roll-up names the relation it climbs or the schema
fails to load. This settles the scope question below in favour of
option 3.

The deciding evidence: `pull` — aggregate's sibling mechanism, walking
a link field under the same `allow_cycles: false` requirement — already
requires `over` unconditionally. Only `aggregate` made it optional, so
the change finishes an existing rule rather than inventing one. The
convenience being dropped is used nowhere in shipped material: both
commented examples in `defaults/schema.yaml` write `over: parent`
explicitly, and this repo's own dogfood schema has no aggregate field
at all. Removing it also removes the last field name besides `id` that
carries meaning.

Rejected: taking the fallback from `defaults.tree_field`. That config
key is currently read by nothing but its own validation warning
(`config_check.rs`); promoting a decorative display role into
load-bearing schema semantics would add a config-to-schema-parse
dependency that does not exist today, and would make `schema.yaml`
unreadable without `config.yaml` open beside it.

**2. Absence is caught by the file reader, not by the schema checker.**
`over` becomes a non-optional field on the deserialised config, so a
roll-up without it fails as a YAML deserialisation error.

The schema has a consistent rule for this already: a key that is
*always* mandatory is enforced by serde (a field's `type`, a roll-up's
`function`, all three of a pull's keys), while a key that is mandatory
only *under a condition* is enforced by the validation pass, where the
message can name the condition (`values` is required only for choice
types; `allow_cycles: false` is required only of a link something rolls
up over). Mandatory `over` is unconditional, which puts it in the first
group — beside `function`, in the same config block.

Rejected: a raw/model split so the validation pass could report a
batched, migration-hinting message. It would leave one roll-up block
whose two mandatory keys fail through two different mechanisms with two
different voices, and there is no precedent anywhere in the schema for
an unconditionally-mandatory key getting the explanatory treatment. The
upgrade hint lives in the changelog instead.

**3. `rollup_confirmation_needed` stays in the web server.** The
original note suggested moving it to core as roll-up domain logic in an
HTTP handler; the code says otherwise, so it is deliberately left where
it is:

- No second consumer exists or is foreseen — the CLI has no equivalent
  guard on writing an aggregating parent.
- Combined schema-and-store questions are answered in the caller layer
  in every existing case (`cli/commands/move.rs`, `cli/commands/set.rs`
  both look up the field definition themselves). `Project` carries no
  query methods at all; adding one for a ten-line helper would
  establish a new pattern for a single user.
- "Confirmation needed" is dialog policy, not domain vocabulary. Living
  in core would require renaming it to something structural — and
  needing that rename is the tell that the function is UI policy
  wrapped around a one-line question.
- Its only concrete reason for appearing in this item — a third
  disconnected copy of the literal — disappears with decision 1.

**4. Migration is a changelog entry.** Breaking change: a consumer
schema whose roll-up omits `over` stops loading until `over: parent` is
written explicitly. Recorded in the changelog's Unreleased section, with
one sentence in ADR-003.

## Where it landed

- `AggregateConfig.over` is a plain `String` —
  `crates/core/src/model/schema.rs`. No `#[serde(default)]`, so the
  deserializer reports absence: `fields.<field>.aggregate: missing
  field 'over' at line N column M`, naming the file, the key path and
  the position.
- `DEFAULT_OVER_FIELD` is gone from `crates/core/src/store/rollup.rs`.
  Its three consumers (`store/derive.rs` twice, `store/required.rs`)
  read `aggregate.over` directly, and `required.rs` no longer imports
  `rollup` at all. The provenance branch in `derive.rs::cycle_diagnostic`
  now skips a slot without an `over` instead of inventing one — a
  same-slot edge only exists for aggregate fields, so the old fallback
  was unreachable.
- `crates/server/src/api/timer.rs` reads `aggregate.over.as_str()`.
  `rollup_confirmation_needed` stays put (decision 3).
- `validate_aggregate_over` in `crates/core/src/parser/schema.rs` lost
  its two-branch message: only "references unknown field" remains, and
  its doc comment records that absence never reaches it.
- `defaults/schema.schema.json` lists `over` in the aggregate
  `required` array and no longer declares a `default`, so editor
  autocomplete flags the omission. Prose updated in
  `defaults/schema.yaml` and `docs/schema.md`; one sentence added to
  ADR-003.
- 26 test fixtures across six files gained an explicit `over: parent`
  (behavior-preserving — they relied on the fallback). The obsolete
  `aggregate_default_over_requires_parent_field` test is replaced by
  `aggregate_without_over_is_rejected_by_the_deserializer`, which pins
  the new contract: absence is an `InvalidYaml`, not a `Validation`,
  error.
- Changelog carries the breaking change with the `over: parent`
  migration line.

Not touched: the `aggregate:` example in the completed
[[duration-field-type]] item omits `over`. It is a historical spec of
what was decided then, not live documentation, so it was left as
written.
