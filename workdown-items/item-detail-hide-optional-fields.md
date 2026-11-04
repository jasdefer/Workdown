---
id: item-detail-hide-optional-fields
title: Let the reader hide optional fields in the item detail
status: to_do
parent: misc-work
depends_on: [item-overlay-redesign]
tags: [web-ui]
---

## In plain words

The item detail shows every schema field except `id`, whether or not the
item has a value for it and whether or not the reader cares. Helper
fields in particular get in the way: a condition-derived `color`, a
duration the timer writes to, a tag list nobody set. They take the same
room as title and status and make the field column long before the
reader reaches what they actually opened the item for.

## Requirement

The reader should be able to hide optional fields in the item detail
(the modal from a view and the standalone `/items/:id` page alike) so
that the fields that matter are the ones on screen. Required fields
always stay visible. Hidden fields must stay reachable: a hidden field
that needs a value still has to be editable, and a diagnostic on a
hidden field still has to be visible.

## Things to settle

- **What counts as optional.** The schema's `required: false` is the
  obvious reading. Fields that are derived (`when`/`compute`) are the
  typical "helper" case and could be hidden by default.
- **Where the choice lives.** A per-reader toggle in the detail
  (session or browser local) is the cheapest. A per-project setting
  would fit the existing `fields` display role, which views already
  use to choose their columns and card badges, but today that role
  does not reach the item detail; whether it should is part of this
  item.
- **Empty versus optional.** Hiding only fields that are both optional
  and empty is a different, smaller feature; decide whether that is
  enough.
