---
title: Query for text in an item's body
status: in_progress
parent: misc-work
tags: [cli, query]
---

## In plain words

Reported as GitHub issue #71
(https://github.com/jasdefer/Workdown/issues/71).

A query can search the frontmatter, for example the title:

```
workdown query --where "title~abc"
```

It cannot search the Markdown body. Something like

```
workdown query --where "body~abc"
```

does not work, and there is no other way in workdown to find the items
whose description mentions a word. The body is where most of an item's
text lives, so "which item talked about X" currently means leaving the
tool and grepping the items directory.

## Requirement

A user can find items by text in their body, from the same place they
already query the frontmatter.

## Ideas (suggestions, not decisions)

The implementer decides all of this after thinking it through.

- The issue's own proposal: a `body` name usable in `--where` with the
  existing `~` operator.
- Points to consider if it goes that way:
  - The body is not a schema field, and so far only `id` is privileged.
    A project may define its own field called `body`; what the name
    means then needs an answer.
  - Where the name is accepted: `--where` on the CLI only, or everywhere
    a where clause is (view filters in `views.yaml`, the web UI's filter
    editor, rules).
  - Which operators make sense on a body besides `~`, and whether the
    match is case-sensitive, the same as for `title~`.
  - Whether the body can also be shown or sorted by, or is match-only.
- A different shape is fine too, for example a separate search flag
  that looks at title and body together, if that serves "find the item
  that mentioned X" better than a where clause.
