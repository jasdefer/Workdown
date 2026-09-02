---
id: status-transition-dates
status: to_do
title: Fill in a date when a status changes, instead of typing it by hand
---

## In plain words

Move an item to In progress and something should write down that it
started today. Move it to Done and something should write down that it
finished today. Today both dates are typed by hand, which is slow for
the common case and silently wrong for the uncommon one. **Example:** a
start date was recently entered as the year **2606**. Nothing complained
— it is a perfectly valid date — and it was only caught later, by
accident, when a genuine 2026 end date was rejected for falling before
the start.

Dates that the tool already knows should not be a typing exercise with
a four-digit failure mode.

## Why the existing machinery does not cover it

Two mechanisms look like they should, and neither does:

- **`when:` / conditional values** ([[conditional-field-value]],
  [[when-then-value-expressions]]) derive a value at read time and never
  write it to the file. A `started` field derived as `$today` would not
  record when work began — it would silently say "today" every day you
  look at it. What is wanted here is a value that freezes at the moment
  of the change, which means an actual write.
- **Validation** cannot see it either. ADR-001 is snapshot-only by
  decision: the CLI judges the current state of the files and never
  inspects git history, so "the status changed" is not a fact validation
  has access to.

So this is a new capability class — *write once, on transition* — not a
new expression. That is the reason it is its own item rather than a
bullet on either of the above.

## Where a transition is actually visible

At the mutation seam, and only there. `workdown set` already reads the
old value before writing the new one and hands it back:
`SetOutcome.previous_value` (`crates/core/src/operations/set/mod.rs:125`)
is the transition, already in hand. The web app performs every change
through that same path, so a card dragged between columns is a
transition the tool can see.

A file edited by hand in an editor is not. Whatever gets built has to
say plainly what happens to those — nothing at all, or a fixup at
commit time via the pre-commit hook we already generate
(`crates/core/src/operations/install_hooks.rs`).

## Shapes discussed, none chosen

- **Schema-declared, on the mutation path.** The schema says which
  transition writes which field; `set` applies it. Consistent with
  "field types drive behavior", and hand-edits are simply not covered.
- **A git hook that calls workdown.** Catches hand-edits too, since it
  runs over whatever is staged, but it must reconstruct the transition
  from the diff — which is exactly the git-history dependency ADR-001
  spent a decision avoiding.
- **A debounce in the web app** — apply the change a few minutes after
  the last edit, so a burst of fiddling produces one write. Raised as a
  way to keep the history quiet; largely obsolete if changes are batched
  behind an explicit save, which is [[commit-from-web-ui]]'s subject.

## Open questions

- **Re-entry.** Moving Done → In progress → Done: does `finished`
  overwrite, keep the first value, or keep the last?
- **Going backwards.** Does In progress → To do clear `started`, or is
  a written date permanent once written?
- **Whose decision is it.** A project-level config key, a per-field
  schema declaration, or a rule block — this is the one that determines
  how much machinery the feature needs.
- **Date or timestamp**, and whether it follows `--as-of` and the
  once-per-invocation clock read ([[evaluation-date-single-read]]).
- **Does a write-on-transition rule bend ADR-001?** Validation stays
  snapshot-only either way, but the tool would start reacting to
  transitions for the first time. Worth stating explicitly rather than
  discovering later.

## Origin

Raised in discussion alongside [[commit-from-web-ui]]: if the board is
where status changes happen, the dates that go with them should happen
there too. Not scheduled.
