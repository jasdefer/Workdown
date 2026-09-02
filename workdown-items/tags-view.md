---
status: removed
parent: misc-work
title: A view over tags
---

> **Removed 2026-09-01 — the need is already met.** Kept as a record of
> the question, not as work to do.
>
> Two existing views answer it between them, with no new code:
>
> ```yaml
> - id: tag-census        # which tags exist, and how many carry each
>   type: bar_chart
>   group_by: tags
>   aggregate: count
>
> - id: tech-debt         # the items under one tag
>   type: table
>   where: [ "tags=tech-debt" ]
> ```
>
> The bar chart already spreads a multi-valued field across bars and
> prints counts in its `## Values` table; `tags=<value>` already tests
> membership. What is missing is only convenience — you have to know a
> tag's name before filtering for it, and it is two surfaces rather than
> one.
>
> **Three shapes were considered and rejected:**
>
> - *Let a board group by `list`.* Rejected on the difference between
>   `list` and `multichoice`: you reach for `list` precisely because the
>   vocabulary is **open**, which means many values, growing over time.
>   Open vocabulary and high cardinality are the same fact, so a column
>   layout is the wrong home for it. The slot's existing exclusion was
>   right.
> - *A new view kind.* Its Markdown output would be indistinguishable
>   from a board's — a rendered board is already vertical sections, not
>   columns — so it would buy a different web layout for the cost of the
>   full add-a-view-kind checklist.
> - *Sectioning for the `table` view.* Still arguable, but not for this:
>   its case rests on `group_by: status`, not on tags. If it is wanted,
>   it should be filed and judged on that.
>
> A tag is not a place, which is why no hierarchy-shaped visualization
> fits: an item does not *live in* `tech-debt`, it *has* `tech-debt`.
> That leaves two honest shapes — sections where an item repeats under
> each of its values, or one flat list where the values become a filter
> — and the second is what filtering already gives you.
>
> One factual correction for a future reader: the note below claims
> multi-membership is "the case boards don't have today". Boards do have
> it — a `multichoice` item appears in every column its values name.
> What boards refuse is the `list` type specifically.
>
> Revisit if tags actually proliferate in a real project and the
> census-plus-filter pair starts to chafe.

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
