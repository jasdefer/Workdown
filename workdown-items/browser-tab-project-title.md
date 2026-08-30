---
id: browser-tab-project-title
status: to_do
parent: misc-work
title: Put the project name in the browser tab title
---

## In plain words

Open two workdown projects in two browser tabs and nothing tells them
apart — both tabs show the same thing, because the app sets no title at
all. Whichever tab you meant is a guess until you click it. The project
already has a name in its config; the tab should use it. **Example:**
`localhost:7777` and `localhost:7778` become "Workdown" and "Acme
Backlog", and the taskbar is readable again.

From GitHub issue #52.

## The problem in detail

`ui/src/app.html` has no `<title>` element and no route sets one, so
the browser falls back to the URL. Two servers on two ports are
indistinguishable in the tab strip, in the window switcher, and in
whatever the user later bookmarks.

The name exists: `project.name` in `config.yaml` (`Workdown` in this
repo, `My Project` from `workdown init`). The server holds the parsed
config in state — but no endpoint hands any of it to the browser.
`/api/schema`, `/api/views`, `/api/items` and `/api/timer` are the whole
surface; project metadata is not on it.

So this is two small changes, not one:

1. **Server** — expose the project's identity to the client. The
   smallest honest shape is a read-only project/config endpoint
   returning at least `name` (and probably `description`); it is the
   natural home for anything else the shell needs to know about the
   project itself.
2. **UI** — set the document title from it, with a fallback for the
   moment before the fetch lands, and a per-page suffix if a page has
   something worth naming (an item's title, a view's title).

## Notes

- The config is parsed once at boot and held in state, so a title fed
  from it will not follow a `config.yaml` edit until restart — the same
  asymmetry [[config-hot-reload]] is about. Not a blocker; whatever that
  item does fixes this too.
- `project.name` is free-form user text and lands in the DOM as a title.
  Set it as text, never as markup.

## Open question

Whether the endpoint is scoped narrowly ("project identity") or becomes
the general read-only config endpoint the web app will eventually want
anyway. Worth about five minutes' thought before writing it, since the
first shape tends to stick.
