---
id: web-layer-adr
title: Write down the web layer's design decisions as an ADR
status: done
parent: maintenance-review-2026-08
---

## In plain words

The core crate's big decisions are written down as eleven ADRs; the
web layer's are written down nowhere. How errors are categorized into
tiers, why the server re-reads the project on every request instead of
caching, how live updates work, why the timer holds a lock while
writing a file — the reasoning exists and is good, but it lives
scattered in code comments that reference decision sheets inside old
work items. A new contributor currently reconstructs the failure model
by reading five files in the right order. One slim ADR is the map.

## The problem in detail

`docs/adr/` covers core decisions (001-011) and not one web-layer
decision. The code repeatedly cites rationale parked in work items
("the first-view-end-to-end decisions", "item decision N") that will
not survive as reference material. What the ADR should capture, each
in a few sentences:

- **The envelope and its three failure tiers** — 422 when the project
  cannot load, 200 with diagnostics when a view is unrenderable, 200
  with data plus warnings otherwise (`crates/server/src/envelope.rs`,
  enforced identically across views, schema, items, timer).
- **The two 404 shapes** — a routing miss is bodyless, an operation on
  an unknown item carries an error body; the reasoning currently lives
  only in a comment (`envelope.rs:143-147`) and a client author has to
  discover it empirically.
- **Cold-load per request, no cache** — deliberate
  (`crates/server/src/state.rs:4`); see [[project-load-cache]] for the
  trigger to revisit.
- **Config is read once at boot while everything else hot-reloads** —
  the watcher (`crates/server/src/watcher.rs:105`) pings browsers on
  any `.yaml` change including `config.yaml`, so a config edit triggers
  refetches served with the stale config. The restart requirement is
  documented for the timer hint (`crates/server/src/api/timer.rs:9-10`)
  and nowhere else; state the asymmetry once (or dissolve it by
  reading config per request, which matches the cold-load philosophy).
- **The two-channel live-update design** — generic file-change ping vs
  the timer-named message, and why they are separate.
- **The timer's single mutex and its blocking file write** — the
  atomicity argument at `crates/server/src/timer.rs:11-19` is correct
  and worth preserving so nobody "fixes" the lock without
  understanding it; note likewise that concurrent field writes from
  two tabs have no cross-request locking, accepted for a local
  single-user tool.

## Objective

One slim ADR ("web API envelope and failure tiers", or similar) in
`docs/adr/`, linking to the code rather than duplicating it, per the
repo's keep-ADRs-slim convention.

## Out of scope

- Changing any of the behavior being documented.
