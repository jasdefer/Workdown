---
id: next-release
type: issue
status: to_do
title: Cut the next release
parent: polish
effort: "2h"
---

Everything since `0.2.0-alpha.2` exists only on the `polish` branch:
the `in`/`not in` operator, the expression grammar, `when:` conditions,
`$today` evaluation semantics, resource validation and pickers, and the
polish fixes. The release machinery is cargo-dist: pushing a version
tag builds all platform binaries, generates installers, and publishes
the GitHub release (prerelease-flagged automatically for suffixed
versions).

## Open decisions

1. The version number: another alpha, promote to beta, or `0.3.0`.
2. Release notes: there is no CHANGELOG file, so cargo-dist generates a
   generic body — write notes by hand for this release, start a
   CHANGELOG, or accept the generated body.
3. Timing: tag from `main` after the polish PR merges (the tag should
   not point at an unreviewed branch head).

## Out of scope

- Publishing to crates.io — distribution stays prebuilt-binary-only,
  per the README's stated design.
