---
title: The same view once per person fills up the navigation
status: to_do
parent: misc-work
tags: [web-ui, views]
---

## In plain words

A team often wants the same board once per person: identical kind,
field and display, and only one filter value differs (`assignee=anna`,
`assignee=ben`, ...). The same happens with other fields, such as one
board per team or per sprint.

Today the only way to keep such a board is to create one view per
value. That works, but it has two costs:

- **The navigation bar fills up.** Every saved view is an entry in the
  header, so five people mean five near-identical entries next to the
  views that are really different.
- **The definitions are copies.** A change to the board (another
  display field, a different sort) has to be repeated in every copy,
  and a new person needs a new view.

## Requirement

A user can look at one view for different values of a filter (typically
per person) without one navigation entry and one copied definition per
value.

## Ideas (suggestions, not decisions)

The implementer decides all of this after thinking it through.

- A view with a parameter: one definition, and a picker on the view
  page chooses the value (a person from the resource the field
  references). The choice could live in the URL so a person's board can
  still be bookmarked and linked.
- A quick filter on the view page that is not saved at all, so the one
  general board is narrowed to a person on the fly.
- Grouping in the navigation: the variants stay separate views but
  collapse under one entry.
- Points to consider: where the list of values comes from (resource,
  the field's `values`, the values present in the items), whether "me"
  can be a value, what `workdown render` produces for such a view, and
  whether the navigation bar needs help with many views regardless of
  this case.
