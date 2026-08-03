---
type: issue
status: to_do
parent: misc-work
title: A view over tags
color: pink
---

## In plain words

Work items can carry tags (this one carries none; some carry
`tech-debt` or `docs`), but no view shows them — there is no page
answering "which tags exist here, and which items carry them?"
**Example:** a rendered `views/tags.md` could list `tech-debt — 2
items`, `docs — 1 item`, `testing — 1 item`, each linking to its
items — or something better the implementer comes up with.

`tags` is a schema `list` field, but nothing surfaces it: no view kind
answers "what tags exist in this project, and what carries them".

Deliberately underspecified — the implementer should think about what
the useful shape actually is. Candidates, not commitments: a listing of
all tags in use with their items, a count per tag, a board-like
grouping where an item appears in every column its tags name (the
multi-membership case boards don't have today), or just making an
existing view kind handle `list` fields well. Per the generic-type-system
rule, whatever ships must work for any `list` field, not the name
`tags`.
