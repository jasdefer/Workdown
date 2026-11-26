---
title: The item detail does not say whether a value was written or derived
status: to_do
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
