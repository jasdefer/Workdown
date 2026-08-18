---
id: ci-workspace-coverage
status: done
title: Run the whole workspace in CI
tags: [ci]
---

Found during the PR 47 review: the workspace's `default-members =
["crates/cli"]` (a `cargo run` convenience) also scoped CI's bare
`cargo build` and `cargo test` to the CLI crate — the core and server
suites (the bulk of the tests) never ran on GitHub. Clippy, doc, and
the release build already passed `--workspace`, so the gap was
invisible behind a green checkmark.

## Decision taken

Keep `default-members` (the local ergonomics are the point of it) and
pass `--workspace` explicitly on the CI build and test steps, with a
comment in `ci.yml` naming the trap. The UI gate needed no change: the
"Build UI bundle" step already runs svelte-check, eslint, prettier,
and vitest through `cargo xtask build-ui`.

## Acceptance

- CI runs every crate's tests: the run reports the core and server
  suites, not just the CLI's.
