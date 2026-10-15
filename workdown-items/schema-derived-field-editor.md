---
id: schema-derived-field-editor
status: in_progress
parent: schema-editor-web
depends_on: [schema-field-editor-shell]
title: Edit compute, when, pull and aggregate blocks in the field editor
---

## In plain words

Some fields work out their own value: a computed number, a colour
picked by condition, a total rolled up from children. The plain field
editor shows those blocks but sends you to `schema.yaml` to change
them. This item makes them editable in the panel.

**Parked until** the plain-field editors ([[schema-field-editor-shell]]
through [[schema-field-editor-relation]]) have shipped and shown what
the panel's UX is. Decided in [[schema-editor-web-design]]: the simple
blocks go first so the derived ones are designed against a real panel,
not a sketch.

## What has to be settled when it is picked up

- **Expressions.** A `compute` line and a `when` condition are text in
  a typed expression language. Free text with the type checker's
  diagnostics shown inline, or a structured builder like the filter
  editor already has for `where:`. The filter builder is the obvious
  starting point; whether it covers arithmetic is the question.
- **`when` branches.** An ordered list of condition → value pairs plus
  a default. A list editor with a condition input and the type's value
  editor per row.
- **`aggregate` and `pull`.** Function picker from the per-type table,
  link-field picker for `pull`.
- **Unlocking the type.** Once the expression is editable, the type
  lock from the shell can be lifted for derived fields, with the type
  check as the guard.

## Not in scope

- New expression grammar. That is [[schema-expressions]].

## Decisions taken

Settled on 2026-10-09 with the user. Four premises checked first:
the type lock stays, because decision 12 of [[schema-editor-web-design]]
removed type changes from the editor altogether; the filter builder's
rows are field, operator, literal and cannot hold arithmetic or a
field-against-field comparison, only its guided-row-plus-raw-row pattern
carries over; the grammar is one comparison or one arithmetic term with
no `and`/`or` yet, every node carrying a span; and the wire serves each
recipe as YAML text while the write leaves recipe keys untouched and
cannot create a field with one. The parser makes `compute`, `when` and
`pull` mutually exclusive; `aggregate` combines with any of them.

Usage checked too: neither this repo's schema nor the shipped default
has a computed field, and every `when` branch in both is a single
field-against-literal comparison.

### Form or text

1. **Conditions are a row, formulas are text.** The objective is a user
   who does not know the syntax. A condition is the case such a user
   writes ("status is done"), so it gets a form: left operand, operator,
   right operand, where an operand is a field, a constant, `$today` or
   a literal entered through the shared `ValueEditor`. That removes the
   quoting trap (`status == done` is an unknown-field error). A formula
   is rare, has no user yet, and the language it is written in is still
   growing ([[schema-expressions]]), so it stays text, made friendly:
   the fields and constants the formula may use are listed beside the
   box and a click inserts one, and the type checker's error is shown
   at the exact character. Rejected for now: a nested row builder
   covering the whole grammar for both. It is the only option that
   meets "no syntax anywhere", costs about twice the rules editor, and
   serves a case nobody has; it is the step to take when formulas have
   users. Also rejected: text for conditions, which would leave the
   quoting trap in the common case.
2. **A condition the row cannot hold is text.** Arithmetic inside an
   operand, or anything beyond one comparison, is shown and edited as a
   raw text row, the filter editor's existing pattern. Rust decides
   which it is, with an exhaustive match on the parsed expression, so a
   new node kind fails to compile until the row shape names it.

### The panel

3. **A "Value" section in the left column.** A selector with four
   entries, entered by hand, computed, conditional, pulled, since the
   parser makes the three recipes exclusive. "Roll up along a link"
   (`aggregate`) is its own section on reducible types, because it
   combines with any of the four. The right column keeps the usage and
   gains the YAML pane the rule panel has. Changing the selector drops
   the old recipe.
4. **Formula options.** `round` only on date fields, `error_on_missing`
   on every type. The writer emits the bare expression string when
   neither is set, the shortest form the parser reads, as the rule
   writer does.
5. **Branches.** An ordered list with Up and Down, since first match
   wins. The `then` cell is `ValueEditor` with the field's own spec, so
   a color field offers the color editor and a choice its values. The
   default control, locked on a conditional field until now, unlocks as
   the fallback: a literal only, no generator, as the parser demands.
6. **Aggregate and pull are pickers over the served tables.** Function
   from the per-type table; `over` from the link fields with cycles
   forbidden, `link` only for aggregate, `link` or `links` for pull;
   pull's `field` from the fields whose reduction fits this field's
   type; `error_on_missing` as a toggle. The server remains the judge.
7. **Removing a `when` drops its `default` too.** Beside `when` the
   default is the evaluated fallback; left in place after the `when`
   goes it would silently become a stamped add-time default, a
   different meaning. The panel clears it and says so. The same when
   switching to computed or pulled, which forbid a default.

### The wire

8. **Structured recipes from Rust, mirroring the rules editor.**
   `GET /api/schema/definition` serves each recipe as named parts
   beside the text it serves today: the expression string and its
   options, the branches with each condition as a row or as text, the
   pull and aggregate configs. The write gains `recipe` and `aggregate`
   and the add path accepts them, so a field can be created with a
   recipe. Types reach the browser through `cargo xtask gen-types`.
   Rejected: the browser reading and writing the YAML itself, a second
   copy of the grammar that drifts.
9. **A preview endpoint, `POST /api/schema/field-preview`.** Takes the
   structured draft, runs the candidate schema through the load spine
   (the type check lives in the compute check and needs
   `resources.yaml`), and returns the YAML the write would produce plus
   the verdict. The pane shows the text; the verdict shows in the
   panel before Save. Same pattern and reason as the rule preview:
   produced by the code that saves, it cannot drift from it.
10. **A rejected recipe blocks Save.** A broken expression is a
    diagnostic that disables the field, not a load failure, so a hand
    edit may leave one in the file. In the panel Save is greyed out
    while the preview reports a problem: the person is right there and
    the position is known. Rejected: write, then warn, as items do.

### Build

11. **One item, in the order aggregate, pull, compute, when**, simplest
    form to hardest; each lands usable. Rust half first (structured
    recipe, writer, preview endpoint), then the panel.

Not in this item: compute expressions reading a field in the usage list
(today they appear only as the warning removal would cause), `and`/`or`
in conditions, and a resources editor for constants.
