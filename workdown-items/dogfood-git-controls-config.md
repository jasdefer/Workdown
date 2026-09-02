---
id: dogfood-git-controls-config
status: to_do
title: Switch on git controls and the effort timer in this repo's own config
parent: full-git-loop
---

## In plain words

Two features shipped, and this repo — the one project that uses
workdown every day — has neither of them turned on. Both were held back
on purpose, for a reason that has since expired, and the reminder lives
in a place nobody looks.

## Why it is still off

Both keys were deferred behind the same release-ordering rule: an older
binary hard-fails on a config key it does not know, so adding the key
before the release that understands it breaks every install that has
not upgraded yet.

- `serve.git_controls: true` — needs 0.2.5, deferred in
  [[git-sync-controls]].
- `defaults.effort_field` — needs 0.2.2, deferred in [[effort-timer]].

0.2.5 shipped on 2026-08-30, so the rule no longer bites. The blocker
now is local: the installed `workdown` binary on the maintainer's
machine still reports **0.2.3**, which is also what the last `views/`
re-render was produced with.

## Why it needs its own item

Right now this exists only as a bullet inside the **Deferred** section
of [[git-sync-controls]], and that item is `done` — so it appears in no
board, no query and no rendered view. The same is true of the
`effort_field` half. A reminder that is invisible to every view of the
project is a reminder that gets lost, which is most of the argument for
tracking it as an item rather than a note.

## Scope

- Reinstall/upgrade the local binary to the released version, and
  confirm `workdown --version` agrees with `Cargo.toml`.
- Add `serve.git_controls: true` and `defaults.effort_field` to
  `.workdown/config.yaml`.
- Re-render `views/` with the matching binary.
- Check what the git pill and the timer actually look like against this
  repo's real history — the point of turning them on is that
  [[commit-from-web-ui]] gets designed against something in use rather
  than something imagined.

## Note

`.workdown/config.yaml` is read once at boot, so both keys need a
server restart to take effect. That is [[config-hot-reload]]'s subject,
not a blocker here.
