---
id: stale-docs-refresh
status: done
title: Fix the documentation that is actively wrong
parent: maintenance-review-2026-08
---

## In plain words

A few pieces of documentation do not just lag behind the code — they
say the opposite of what is true. The worst is the web UI's README,
which tells a new contributor the app is read-only and editing is not
built yet; all of that shipped long ago. Each fix is minutes; together
they remove every place where reading the docs makes you *less*
correct about the code.

## The problem in detail

The review recorded six locations. A re-sweep before starting (stale
command names, dangling issue references, provisional language in doc
comments, referenced paths that no longer exist) confirmed all six were
still stale and found eight more of exactly the same kind — mostly
comments that predict a "future server", a "future `body`" command or
"future `--append`/`--remove`/`--delta` modes" that have all shipped
since. Handled as one pass.

**Recorded by the review**

- **`ui/README.md`** — said the SPA "renders a project's views
  **read-only**" and "Mutations, item detail pages, and live
  file-watching are … not implemented yet". All three shipped.
- **`ui/vitest.config.ts`** — claimed "the only tests so far are …
  the gantt view"; there are nine test files.
- **`crates/server/src/envelope.rs`** — referenced "the same shape as
  `workdown check --json`"; the command is
  `workdown validate --format json`.
- **`crates/core/src/operations/frontmatter_io.rs`** — two merged doc
  comments sat on the wrong function, leaving `parse_value_for_field`
  undocumented; its collection arm also re-implemented
  `parse_collection_values`' comma-split inline.
- **`crates/core/src/parser/schema.rs`** — three doc comments said
  "Regex for valid …" over hand-rolled character loops.
- **`crates/server/src/api/views.rs`** — the module's tier list said a
  view is withheld when it "has a `views_check` diagnostic pinned to
  it"; only an *error* withholds it.

**Found by the sweep**

- **`crates/server/src/api.rs`** — "See the issue body for the full
  rationale and planned resource files": a pointer to nothing (the
  project has no issues), and every resource module now exists. The
  rationale is already stated in the paragraph itself; the pointer was
  dropped.
- **`crates/core/src/rules/condition.rs`** — "designed to be reusable
  by the query command (issue #15)". `workdown query` shipped with its
  own evaluator (`query::eval`) and never reused this one.
- **`crates/cli/src/cli/mod.rs`, `operations/body.rs`,
  `operations/add.rs`, `operations/set/mod.rs`** — four references to
  "the future server", which has shipped.
- **`crates/core/src/operations/frontmatter_io.rs`** (header) — "the
  future `body`" command, which has shipped.
- **`crates/core/src/operations/set/mod.rs`** — "`None` for future
  `Unset`", where `SetOperation::Unset` is defined 70 lines above.
- **`crates/core/src/project.rs`** — "A future watcher (`live-updates`)
  handles SSE push"; the watcher is in `crates/server/src/watcher.rs`.
- **`crates/core/src/query/mod.rs`** — "future commands (board, tree,
  graph) reuse the engine". Those became view kinds, not commands; the
  real second consumer is every view's `where:` filter, via
  `view_data/filter.rs`.
- **`crates/cli/src/commands/mutation_output.rs`** — "the future
  `--append`, `--remove`, `--delta` modes"; all shipped, plus
  `--toggle`.
- **`crates/xtask/src/main.rs`** — a second copy of the stale "only the
  gantt scale math and format helpers" claim about the vitest suite.
- **`ui/src/routes/+layout.ts`** — "the diagnostic banner / future nav
  menu"; `ViewNav` ships and is rendered in `+layout.svelte`.
- **`ui/README.md` and `README.md`** — both listed `cargo xtask`'s
  build steps, and both were missing steps the orchestrator actually
  runs (`gen-types`, `lint`, `test`).
- **`crates/core/src/model/resources.rs`** — "a future display-config
  feature may let a project pick a different attribute", written before
  display roles shipped. Display roles exist but govern *items*, not
  resource labels; reworded to say so rather than to imply the feature
  is missing.

Checked and left alone because they are still true: `ViewSummary.title`
("always `None` — no source in `views.yaml` yet"), the item body being
rendered read-only in the UI, `WhenConfig`'s "parsed but not yet
type-checked" (a pipeline stage, not a missing feature), and
`docs/views.md` / `docs/schema.md`, whose tables still match the Rust
enums exactly.

## Decisions taken

1. **Scope: the sweep, not just the recorded six.** The review's list
   was a three-week-old snapshot, and the sweep for the same failure
   patterns was cheap and bounded. Closing an item called "fix the
   documentation that is actively wrong" while leaving eight known
   instances of the same wrongness in place would have been odd.
2. **`ui/README.md`: reshaped, not patched.** The stale bullet rotted
   *because* it was a feature inventory; patching it in place would
   have left the rot mechanism running. The opening now says what the
   app is in a few sentences and links to ADR-013 and
   `docs/architecture.md` for the detail. Rule for the file going
   forward: what the app is, how to develop it, how it is built —
   stable shape only, nothing enumerable.
3. **`vitest.config.ts`: restated the reason, did not change the
   setup.** The claim was wrong but its rationale still holds — all
   nine tests target pure, DOM-free modules, so a plain Node
   environment really is enough. The comment now says that and names
   the condition under which the config must grow (a test needing a
   DOM or the Svelte compiler). Adding jsdom and the Svelte plugin
   belongs to [[stateful-test-gaps]], which will know what it actually
   needs.
4. **`frontmatter_io.rs`: fixed the duplication too, not just the
   comment.** Moving the comment while leaving the inline re-implementation
   it pointed at would have been half a fix; `parse_value_for_field`'s
   collection arm now calls `parse_collection_values`.
5. **`schema.rs`: cross-referenced the mirror, did not build a guard.**
   Those three naming rules are also written as real regexes in
   `defaults/schema.schema.json` — an unguarded mirror. The comments
   now name their counterpart so a change to one prompts a change to
   the other; automating the check belongs to
   [[view-kind-sync-guards]], which should own one helper for all such
   mirrors rather than a hand-rolled early version here.

## Objective

Every listed location corrected. Full CI gate set green (`fmt`,
`clippy -D warnings`, `test`, `cargo doc -D warnings`,
`cargo xtask build-ui`).

## Out of scope

- New documentation ([[render-flow-doc]] and [[web-layer-adr]] cover
  that).
