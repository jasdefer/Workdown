---
id: dogfood-git-controls-config
status: done
title: Switch on git controls in this repo's own config
parent: full-git-loop
---

## In plain words

Git controls shipped in 0.2.5, and this repo — the one project that
uses workdown every day — has them turned off. The key was held back on
purpose, for a reason that has since expired, and the reminder lived in
a place nobody looks.

## Why it is still off

`serve.git_controls: true` was deferred behind a release-ordering rule:
an older binary hard-fails on a config key it does not know, so adding
the key before the release that understands it breaks every install
that has not upgraded yet. 0.2.5 shipped on 2026-08-30, and the
maintainer's installed binary now reports 0.2.6, so nothing blocks it
any more.

## Why it needs its own item

This existed only as a bullet inside the **Deferred** section of
[[git-sync-controls]], and that item is `done` — so it appeared in no
board, no query and no rendered view. A reminder that is invisible to
every view of the project is a reminder that gets lost, which is most
of the argument for tracking it as an item rather than a note.

## Scope

- Add `serve.git_controls: true` to `.workdown/config.yaml`.
- Restart the server and look at the pill against this repo's real
  history: with a dirty working tree, after a terminal-side commit, and
  with the remote unreachable.
- Write down what the pill does and does not tell you at the moment you
  would want to commit. That is the input [[commit-from-web-ui]] is
  waiting for — the point of turning this on is that the commit step
  gets designed against something in use rather than something
  imagined.
- Re-render `views/` with the 0.2.6 binary.

## Not in scope

`defaults.effort_field` was originally bundled here because it was
deferred behind the same release-ordering rule. Dropped on 2026-09-02:
this repo's schema has no duration field by design (scheduling fields
were removed on 2026-08-18 because nobody read them), and adding one
only to exercise the timer would recreate exactly that. The timer slot
showing "no effort field configured" is the intended state for this
project.

## Note

`.workdown/config.yaml` is read once at boot, so the key needs a server
restart to take effect. That is [[config-hot-reload]]'s subject, not a
blocker here.
