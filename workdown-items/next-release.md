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

## Decisions taken (2026-08-03)

1. **The version number is decided during PR review**, with the full
   diff in view, not before. Candidates discussed: `0.2.0` final (the
   alpha series was building toward exactly this), `0.2.0-alpha.3`
   (conservative), `0.3.0` (signals the `where:` semantics change).
2. **No release notes.** The generated release body (installer
   boilerplate) is accepted — the tool has one user today, so a
   CHANGELOG would be ceremony without a reader. Revisit when someone
   else is actually updating across versions.
3. **Sequence:** decide version at review → push the `Cargo.toml` bump
   as a commit to the branch (cargo-dist checks the tag against the
   crate version, so the bump must precede the tag) → merge → tag the
   merge commit on `main` → push the tag; the release workflow builds
   binaries and installers and publishes on its own.

## Out of scope

- Publishing to crates.io — distribution stays prebuilt-binary-only,
  per the README's stated design.
