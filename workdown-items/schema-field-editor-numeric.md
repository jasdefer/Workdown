---
id: schema-field-editor-numeric
status: done
parent: schema-editor-web
depends_on: [schema-field-editor-shell]
title: Field editor block for integer, float and duration
---

## In plain words

Numbers and durations can be bounded: an estimate cannot be negative, a
percentage stops at a hundred. In the schema that is `min` and `max`.
This item adds the block of the field editor that edits them, typed per
type — an integer bound is a whole number, a duration bound is a
duration.

One of the five type-specific shapes in [[schema-editor-web-design]].

## What is needed

- `min` and `max`, each optional, entered with the type's own editor
  so a duration bound reads `2d` and not `2`.
- Client-side check that `min` does not exceed `max`; the server's
  parse rejects it anyway, this only saves a round trip.
- Clearing a bound removes the key from the file.

## Not in scope

- `compute`, which these types also accept:
  [[schema-derived-field-editor]].

## Decisions taken

Settled on 2026-10-06 with the user, who asked for the clean solution
over the quick one.

1. **A duration bound travels as suffix shorthand, not seconds.** The
   definition payload had typed duration bounds as whole seconds, on
   the assumption that the item panel's duration editor works in
   seconds. It does not: a duration value travels as text (`1w 2d`)
   and the server parses it. The web app has a seconds-to-text
   formatter but no parser, so seconds would have meant a second copy
   of the duration grammar in TypeScript. `FieldShape::Duration` now
   carries `min`/`max` as text: served in canonical form, written as
   sent, judged by `parse_schema`, refused with its message. One
   grammar, in Rust, and a bound behaves exactly like a value does.
2. **The server checks `min ≤ max` for integer and float too.** This
   item's premise, "the server's parse rejects it anyway", was true
   only for duration. The check is now one function for every bounded
   type, with the same message. It is a schema parse error, the
   project-fails-to-load tier, like the duration check always was;
   an inverted pair admits no value at all, so no project could have
   been using one. The client check remains, and only saves a round
   trip: it greys out *Save* with the reason for a numeric pair, and
   leaves a duration pair to the server, whose grammar it does not
   have.
3. **A fractional bound on an integer field stays accepted by the
   server.** The editor's integer input steps by whole numbers, so the
   panel cannot produce one; a hand-written `min: 2.5` is a gap this
   item does not close.
4. **The bounds are entered with `ValueEditor`,** the same component
   an item's value and the field's default use, with a spec that is
   never required and not itself bounded. Clearing an input hands over
   `null`, which unsets the bound and drops the key from the file; the
   `withBound` helper in `fieldDraft.ts` is the one place that turns
   the editor's value into a bound of the shape.
5. **`BoundsBlock.svelte` owns no state.** The draft's shape is the
   value, every change goes up as the whole new shape, and the panel
   derives the problem message and passes it down to be shown under
   the inputs it is about.
6. **The schema page's settings line shows the served text as is.**
   The server formats a duration bound canonically, so the table, the
   editor and the file read the same without a second formatter pass.
