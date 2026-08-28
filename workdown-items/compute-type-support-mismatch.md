---
id: compute-type-support-mismatch
status: to_do
title: Decide which field types may declare compute and pull
---

## In plain words

Two places answer "may a `string` field derive its value from a
`compute:` expression?", and they answer differently. The editor
autocomplete schema says no; the Rust checker says yes. Nobody has
decided which is right — the disagreement was found by reading them
side by side, not by anyone hitting it.

## The problem in detail

`crates/core/defaults/schema.schema.json` forbids `compute:` and
`pull:` on `string`, `choice`, `multichoice`, `color`, `list`, `link`
and `links` fields — a single `if`/`then` block listing all seven. So
an editor with the schema loaded flags `compute:` on a `string` field
as invalid.

`crates/core/src/compute_check.rs::expression_type_of` maps a declared
field type to the expression type it participates as, and returns a
type for three of those seven: `String` and `Choice` become `Text`,
`Color` becomes `Color`. Only the four collection types
(`multichoice`, `list`, `link`, `links`) return `None`. So the CLI
accepts a computed `string` field that the editor reddens.

`pull:` is restricted differently again — not by a type list but by
whether the source field's type has aggregate functions and whether
the result type fits the declared type — so its overlap with the JSON
schema's seven-type list needs checking on its own rather than being
assumed to match `compute:`.

## Why it is a decision, not a fix

Either side can be made to agree with the other, and the two answers
are genuinely different products:

- **The CLI is right** — a computed `string` (say, a label built from
  other fields) and a computed `color` are useful, the expression
  algebra already types them, and the JSON schema is simply stale.
  Then the fix is to narrow the schema's list to the four collection
  types.
- **The schema is right** — derivation is for the scalar/temporal
  types and text derivation invites string concatenation the algebra
  was not designed for. Then the fix is a check in `compute_check`
  rejecting the declaration, plus a diagnostic.

Whichever wins, the losing side changes and the pair gets a test, so
the two cannot drift apart again.

## Where it came from

Found while writing the decision sheet for
[[view-kind-sync-guards]]'s fourth mirror. That item guards the
field-type → allowed-*properties* matrix by probing the JSON schema
against the Rust rule; `compute:` and `pull:` were deliberately left
out of its scope, because settling a real behavioral rule should not
ride along inside a test-only change. Once this item lands, the probe
there can be widened to cover both keys.

## Objective

Decide which types may declare `compute:` and which may declare
`pull:`, make both the Rust checker and `schema.schema.json` say it,
and cover the agreement with the probe test from
[[view-kind-sync-guards]].
