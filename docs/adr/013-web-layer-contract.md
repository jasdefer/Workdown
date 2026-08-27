# ADR-013: Web layer response contract and runtime model

**Status:** Accepted
**Date:** 2026-08-27

## Context

ADR-006 settled the web layer's *structure*, not how the running server
*behaves*: response shape, what happens when the project is broken, what
is re-read per request, how a browser learns something changed. Those
decisions were made during implementation and cited only by work-item
name. This ADR is retroactive — it records what the code already does so
the reasoning outlives those items, and points at the code for detail.

## Decisions

### One envelope, three failure tiers

Every endpoint answers with the same three slots — `data`,
`diagnostics`, `error` (`crates/server/src/envelope.rs`). Status answers
"did it happen", `diagnostics` "what should the user know about the
project", `error` "why did this request fail". `diagnostics` is always
present, often empty, so the client never optional-chains it. Breakage
grades into three tiers, identical across the view, schema, item and
timer handlers: the project cannot load (`422`, load diagnostic, no
data); it loaded but the view cannot render (`200`, no data,
diagnostics — not an error status, because the request succeeded and the
answer is "this view is misconfigured", rendered as a banner in its
place); or success (`200`, data plus any diagnostics, warnings
included).

`diagnostics` and `error` stay separate: a diagnostic is a structured,
scoped project finding (ADR-007), a request failure is neither. Around
the tiers sit `201` on create, `409` for refusals about state rather
than input (create over an existing id, start a timer while one runs),
`500` for I/O. Per ADR-001 a schema violation is not a failure — the
mutation saves, its warnings ride in `diagnostics`.

### Two shapes of 404

A URL naming a view that does not exist answers `404` with no body; an
operation on an unknown *item* answers `404` with an envelope carrying
`error`. A wrong URL is not a project finding, and synthesizing a
diagnostic for one would dilute a vocabulary that otherwise means
"something needs attention in your project" — whereas a failed operation
does have something to tell the caller who attempted it.

### Cold-load per request, no cache

Every request re-reads and re-derives the whole project through
`core::load_project`; nothing is cached (`crates/server/src/state.rs`).
No cache means no invalidation and no stale-read bugs, and it keeps
`$today`-derived values honest across midnight (ADR-010). The cost is
accepted at present project sizes; `project-load-cache` holds the
trigger to revisit.

### Config is read once at boot; everything else hot-reloads

`config.yaml` is parsed at startup and held in state; schema, views,
resources and items are re-read per request. So a config edit needs a
restart — and the watcher pings browsers on any `.yaml` change,
`config.yaml` included, so the edit provokes a refetch served with the
*old* config, with nothing to say so (`crates/server/src/watcher.rs`).
Recorded, not defended: only the timer's effort-field hint mentions the
restart (`crates/server/src/api/timer.rs`), and while the port and
watched directories are genuinely boot-bound, the rest could be read per
request as the cold-load rule predicts — see `config-hot-reload`.

### Two live-update channels, not one

Two broadcast channels (`crates/server/src/state.rs`) merge into one SSE
stream per tab (`crates/server/src/api/events.rs`): a contentless
"something changed" ping from the file watcher, and a timer-named
message. Pings carry no detail because one item edit can ripple into
other items' computed values, so refetching the current view is simpler
and always correct. The channels stay separate so the two refresh scopes
do — one channel would reload the timer on every file save and the whole
page on every timer action — and so that lag on the timer channel can
only mean missed timer pings.

### The timer's lock is held across its file write

The effort timer lives in server memory, one per process, behind one
mutex (`crates/server/src/timer.rs`), and stop writes the elapsed
duration to the item file *while still holding* it. That looks like a
lock held too long and is not: taking the timer and writing it are one
indivisible step, so two tabs pressing stop cannot both write, and a
failed write leaves the interval running rather than discarding measured
time. Do not narrow it without replacing that guarantee. Ordinary field
writes have no equivalent cross-request lock — two tabs editing one
field race, last write wins — accepted for a local single-user tool
whose general answer to concurrent edits is git.

## Consequences

- The failure model is one page rather than five module headers read in
  the right order; those headers keep the detail and now point here.
- A client author learns the contract — three tiers, two 404 shapes, the
  statuses around them — without reading the server.
- The per-request full load and the config restart requirement are on
  the record rather than in folklore.
