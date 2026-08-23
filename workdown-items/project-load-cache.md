---
id: project-load-cache
status: to_do
title: Cache the project load in the server (when it starts to hurt)
tags:
- watch
---

## In plain words

The web server re-reads and re-processes the entire project — every
item file, every auto-filled field, every validation — on every single
request. That is a deliberate simplicity choice: no cache means no
stale-cache bugs, and correctness always wins. It is the right
tradeoff at today's project sizes, and this item exists so the
eventual fix is remembered, not so it is done now.

**Trigger to act:** a project around a thousand items, or web-app
interactions becoming visibly sluggish — whichever is noticed first.

## The problem in detail

Every API hit runs `load_project()`: parse all items, run the full
derive-graph walk, run all validation — O(items × fields) per request,
multiplied by the live-update design (a file-change ping makes every
open tab refetch). The choice is documented at
`crates/server/src/state.rs:4` and consistent with ADR-010's
freshness reasoning (`$today` must not go stale at midnight).

The remedy is contained when needed: the file watcher
(`crates/server/src/watcher.rs`) already knows when files change, so
a watcher-invalidated (or mtime-keyed) cached load fits inside the
existing architecture — with one wrinkle to respect: a cache keyed
only on file changes must still refresh across a date change, or
`$today`-derived values go stale exactly the way ADR-010 forbids.

Not part of [[maintenance-review-2026-08]] — no action until the
trigger, and a watch item must not block the milestone from
completing.
