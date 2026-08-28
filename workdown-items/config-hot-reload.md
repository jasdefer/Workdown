---
id: config-hot-reload
status: to_do
title: Read config.yaml per request so it hot-reloads like everything else
---

## In plain words

Edit `schema.yaml`, `views.yaml`, `resources.yaml` or any work item
while `workdown serve` is running and the browser shows the change a
moment later. Edit `config.yaml` and it does not — the server is still
using the copy it read at startup. Worse than not updating: the file
watcher pings the browsers anyway, so the edit visibly *causes* a
refetch, and that refetch is served with the old config. Nothing says
so. The user sees their change ignored and has no reason to suspect a
restart is needed.

## The problem in detail

The server parses `config.yaml` once at boot and holds it in state
(`crates/server/src/state.rs`); every request passes that same parsed
copy to `core::load_project`, which re-reads everything else from disk.
The watcher's allowlist covers `.yaml`, so `config.yaml` edits ping like
any other change (`crates/server/src/watcher.rs`).

The restart requirement is stated in exactly one place — the timer's
effort-field hint (`crates/server/src/api/timer.rs`) — and the
asymmetry as a whole is recorded in ADR-013, which is documentation, not
a fix.

Reading the file per request matches the layer's cold-load philosophy
and costs one small parse next to a full project load. Two wrinkles to
respect:

- **Not everything can hot-reload.** The listening port and the set of
  watched directories are bound at startup. Those keep needing a
  restart, so the fix shrinks the asymmetry rather than dissolving it —
  and the remaining part should be stated where a user meets it.
- **A broken config must not take the server down.** Today an
  unparseable `config.yaml` can only fail at boot. Per request it
  becomes a runtime failure and needs a tier-1 answer (`422` with a
  config-scoped diagnostic), not a panic.

## Objective

`config.yaml` edits take effect without a restart, wherever that is
possible; a broken config surfaces as a diagnostic rather than a crash;
whatever genuinely still needs a restart is said once, in the UI. Update
ADR-013's config section to match what is then true.

## Out of scope

- Hot-rebinding the port or re-targeting the watcher.
- Caching the project load — that is [[project-load-cache]], and its
  trigger has not fired.

Not part of [[maintenance-review-2026-08]]: the milestone is closing and
this is a behavior change, not a cleanup.
