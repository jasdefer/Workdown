---
id: git-status-event-loop
title: The live-update watchers pinged on reads, so a Linux tab reloaded itself in a loop
status: done
parent: misc-work
tags: [bug, web-ui, server]
---

## In plain words

On Linux, a browser tab with the app open reloaded its page four times
a second with nothing happening in the project, and with
`serve.git_controls` on the git pill recounted at the same pace. Each
reload made the server read the item and config files, the watcher
reported those reads as changes, and the browser reloaded again.

Found on 2026-11-02 during the click-through of
[[item-overlay-redesign]] in the Linux dev container. Never seen on
Windows, where the OS reports no file accesses to a watcher.

## Cause

Both watchers in `crates/server/src/watcher.rs` judged a batch by path
alone. Linux inotify reports every `open()` of a watched file as an
`Access(Open)` event, so:

- the file watcher saw the server's own reads of `schema.yaml`,
  `resources.yaml` and every item on each request, matched them by
  extension, and pinged — the browser re-ran its loads, the server read
  again;
- the git watcher saw git opening `.git/HEAD` on every status recount,
  matched it by name, and emitted a `git` event — the pill recounted,
  git read `HEAD` again.

Measured before the fix: idle stream silent with no client; one
`GET /api/git` produced exactly one `git` event; one
`GET /api/views/table` produced one file ping; a tab logged pairs of
events 50 ms apart, four pairs a second. The debug log showed the
batches: `Access(Open(Any))` on `config`, `index`, `refs`, `HEAD` for
a status, and on `schema.yaml`, `resources.yaml`, the items for a view.

## Decisions taken (2026-11-02)

1. **Only writes count.** A shared predicate `records_change` drops
   every `Access(_)` kind before either allowlist is applied; creates,
   modifies (data, metadata, rename), removes and the backend's
   unclassified kinds still count. A write never arrives as an access
   alone, so nothing is lost. Unit-tested over every kind.
2. The allowlists, the debounce and the error arm (an error still
   pings, since a missed event is worse than a spare reload) are
   unchanged.
3. Verified after the fix with the same measurements: a status call
   and a view read produce no event; appending to an item file still
   pings, and a commit still emits a `git` event.
