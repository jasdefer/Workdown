---
id: add-missing-items-directory
status: done
title: Commands fail when the work-items directory is missing
tags: [bug]
parent: dogfood-bugs
---

## In plain words

If the folder that holds the work items is not there, `workdown add`
refuses to create an item and reports a failure to load work items.
A project with no items yet is a perfectly normal state, and the folder
is easy to end up without: git does not keep empty directories, so a
fresh clone of a project whose items were all deleted — or one where
the configured path has simply not been created yet — has none.

**Example:** you clone a project, run `workdown add --title "First
item"`, and instead of a new item you get an error about work items
that cannot be loaded. Creating the folder by hand makes the same
command succeed.

## What needs to be done

- `workdown add` works when the configured work-items directory does
  not exist, and the new item ends up in the right place.
- Every other command that reads the project treats a missing
  work-items directory as a project with no items, not as an error.
  A genuinely unreadable directory must still be reported.
