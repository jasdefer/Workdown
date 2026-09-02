---
id: browser-tab-project-title
title: Put the project name in the browser tab title
status: done
parent: misc-work
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

The root layout's `<title>` is the constant "Workdown" whenever no
timer counts down (see the note below), so two servers on two ports are
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
- The tab is not quite title-less any more: [[timer-notifications]]
  shipped a `<title>` in the root layout showing the pomodoro countdown,
  and the plain word "Workdown" whenever nothing counts down. So this
  work replaces that constant rather than introducing the element, and
  the countdown has to keep decorating whatever the new title is.

## Decisions taken

1. **A narrow `GET /api/project`**, not a general config endpoint —
   `{ name, description }` and nothing else. `paths.*` is local
   filesystem layout the browser has no business knowing, and a
   "here is the config" shape invites shipping it. Further shell needs
   join this contract field by field.
2. **Answered from the boot-time config alone**, not from a project
   load. That is what keeps the tab named while a broken schema has
   every other read answering 422.
3. **Fetched in the root layout's `load`, in parallel with the views
   index.** The fallback before it lands (and if it never does) is the
   tool's own name, `Workdown`.
4. **`Project — Page`, project first.** The complaint is two servers on
   two ports; a narrow tab shows only the first few characters, and the
   window switcher and any bookmark inherit the same order.
5. **Every page label comes from data already at hand** — the views
   index for a view's name, the URL's id prettified for an item (the
   same label the item page's own heading shows), fixed words for the
   new/edit routes, and no label at all for `/` and the error page. No
   page waits on a second fetch to be named.
6. **`description` is carried and used** as `<meta name="description">`.
   No crawler will read a localhost app, but it costs one line and
   bookmark tooling sometimes picks it up.
7. **No static `<title>` in `app.html`.** The HTML spec gives the tab
   the *first* title element in the document, so a static fallback there
   would shadow the layout's dynamic one for the tab's whole life. The
   fallback lives inside the rendered title instead, and the layout's
   `load` awaits the fetch before the first render anyway.
