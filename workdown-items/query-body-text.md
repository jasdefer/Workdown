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

## Decisions taken (2026-11-26)

1. **A search flag, not a where clause.** `workdown query --search
   <TEXT>`. Searching an item's text is not a comparison on a field, so
   it does not go into the field grammar. A `$body` name inside
   `--where` was considered: it cannot collide with a schema field and
   views would have had it for free, but it needs shell quoting and
   ties the matching to what `~` does on fields. A bare `body` name was
   dropped because a project may define a field called `body`.
2. **CLI only.** Views do not get the search. That would be a new slot
   in `views.yaml` (model, JSON Schema, validation, web UI) and is a
   separate item if someone asks for it.
3. **What is searched:** the body, the id, and every field of type
   `string`. Chosen by type, so no field name is wired in; a `choice`
   field such as a status is not searched, so `--search done` does not
   return every finished item.
4. **How `--search` matches:** substring, ignoring case, the text taken
   literally including spaces. An empty text is an error.
5. **`--search-regex <PATTERN>`** for a regular expression, as a flag
   of its own so that a text such as `/api/users/` stays literal under
   `--search`. Case-sensitive (`(?i)` in the pattern turns that off);
   `^` and `$` match at line starts and ends, because a body has many
   lines. An invalid or empty pattern is an error.
6. **Combining:** every `--search`, `--search-regex` and `--where` must
   hold (AND). Each search term may be found in a different place, so
   one term can hit the title and another the body.
7. **Match only.** The body is not a column and not a sort key. A
   snippet around the hit, as grep prints it, would be a later item.
