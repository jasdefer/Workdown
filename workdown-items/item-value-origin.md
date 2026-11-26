---
title: The item detail does not say whether a value was written or derived
status: done
parent: misc-work
tags: [web-ui, feature-request]
---

## In plain words

Feature request.

A field's value on an item can come from two places: someone wrote it
into the item's file, or workdown derived it (a `compute` expression, a
`when` list, a `pull` over a link, an `aggregate` rollup from the
children). A hand-written value always wins over a derived one.

The item detail in the web app shows the resulting value and nothing
about where it came from:

- A computed value looks exactly like a written one.
- A field with an `aggregate` carries the note "computed" next to its
  name, but the note comes from the schema. It is shown on every item,
  also on the ones where the value was written by hand.
- The only way to find out today is to open the `.md` file and look
  whether the field is in the frontmatter.

This matters beyond curiosity. Editing a derived value in the detail
writes it into the file, which switches the derivation off for that
item from then on. Nothing tells the user that this happened, and
nothing on the item shows it afterwards.

## Requirement

A user looking at an item can tell, for each field, whether its value
was written on the item or derived by workdown.

How this is shown, and whether anything follows from it (for example a
way to go back from a written value to the derived one), is for the
implementer to decide.

## Decisions taken

User experience, decided 2026-11-26:

1. **The mechanism is named.** A derived value says which mechanism
   produced it: `computed`, `conditional`, `pulled` or `aggregated`, the
   words the schema page already uses.
2. **Derived values and overrides are both marked.** A field nothing can
   derive carries no mark. A derived value carries its mechanism. A
   hand-written value on a field that would otherwise be derived is
   marked as an override.
3. **The mark is a badge next to the field name**, where the `computed`
   note is today, with a tooltip. The value itself is drawn as before.
4. **An override has a way back.** An action on the override removes
   the written value so the derivation applies again.
5. **No warning before overriding.** The override badge and the way
   back are the feedback.
6. **The override does not show what the derivation would have
   produced.** Left for a follow-up if anyone asks.
7. **Scope is the item editor**, which is the dialog, the docked panel
   and the standalone page at once. Table cells and cards are out. The
   CLI has no single-item view to extend.
8. **Edge cases.** A derivable field with no value shows its mechanism
   as `… · no value`. A field with both a recipe and an `aggregate`
   shows the mechanism that produced the value on this item.

What counts as an override, decided 2026-11-26 (refines 2):

- **A written value is an override exactly where a mechanism would
  otherwise be responsible for the field on that item.** That is the
  recipe (`compute`, `when`, `pull`) wherever it would run, and the
  rollup on an item that has children along the aggregate's link.
- **A value written on the lowest level of a field that only
  aggregates is plain input** and carries no mark: nothing would fill
  it, so it overrides nothing.
- The same rule decides the `… · no value` mark: it appears where a
  mechanism is responsible and produced nothing.

Data model, decided 2026-11-26:

- **The derive phase records the origin as it writes a value.** Nothing
  reconstructs it afterwards from the schema.
- **A value and its origin are one entry.** An item's fields are their
  own type (`ItemFields`) holding one map of value plus origin, not two
  maps side by side whose keys could drift apart. Reading a value is
  unchanged for every consumer that only wants the value. There is no
  plain insert: a write says whether the value is written or derived.
  The cost of touching every reader was explicitly not a criterion.
- **The server decides each field's state, the browser only words it.**
  The item detail carries a `derivations` list beside the values: one
  entry per field a mechanism is responsible for on that item, with the
  mechanism and a state of `derived`, `overridden` or `empty`. The
  card field type shared with every view is left alone.
- **The way back is the existing unset.** No new endpoint.
