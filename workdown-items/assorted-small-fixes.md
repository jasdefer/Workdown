---
id: assorted-small-fixes
status: to_do
title: Grab bag of small consistency fixes from the review
parent: maintenance-review-2026-08
---

## In plain words

Everything the review flagged that is real but small — minutes to a
few hours each, none urgent, none blocking anything. Collected here so
they are not lost; fold them into whichever milestone item touches the
same area, or knock them off in idle moments. Nothing in this list
changes behavior users can see, except where noted.

## The list, by area

**Core layering and duplicate helpers**

- The schema parser imports coercion from the store layer
  (`crates/core/src/parser/schema.rs:18` uses `store::coerce`), while
  the store depends on the parser — a layering inversion. Coercion is
  a pure operation with no store state; move `coerce.rs` to its own
  top-level module (a file move plus path edits).
- `yaml_kind_name` (`parser/schema.rs:928`) and `yaml_type_name`
  (`store/coerce.rs:418`) — identical body, two names.
- `is_leaf` (`store/compute.rs:115`) and `is_tree_leaf`
  (`store/rollup.rs:106`) — identical body and doc, two names, each
  called from a different sibling module.
- Regex `pattern` handling in coercion (`store/coerce.rs:139`):
  compiled per value per item, and an *invalid* pattern (a schema
  defect) is reported per item instead of once against schema.yaml —
  against the crate's own policy. Validate at schema parse, compile
  once. (Slightly behavior-visible: better diagnostics.)
- The derive scheduler's raw `slot * item_count + position` node-id
  arithmetic appears ~15 times in `store/derive.rs`; a two-method
  helper would spare the next reader.
- A `Schema::new(fields, rules)` constructor that computes the inverse
  table — six test modules currently hand-build the three-field ritual
  (`store/mod.rs:334`, `store/coerce.rs:441`, `store/cycles.rs:189`,
  `resolve.rs:140`, `generators.rs:370`, `schema_data.rs:241`), and a
  new invariant field on `Schema` breaks every literal constructor.
- Vestigial `_expected: Ordering` parameter on
  `check_field_comparison` (`rules/assertion.rs:219`); fully-qualified
  `std::string::String` leftovers in `model/rule.rs`, `assertion.rs`,
  `condition.rs`.
- `operations/add.rs:189` writes the new file with bare
  `std::fs::write` while every other mutation goes through
  `write_file_atomically` — harmless for a brand-new file, but it
  muddies which write path is canonical.

**CLI**

- `main.rs::run` repeats the `std::env::current_dir().map_err(...)`
  boilerplate thirteen times and needs a nested
  `match cmd { Command::Init => unreachable!(), ... }` for the init
  special case (`main.rs:60`) — hoist `current_dir` once, split
  per-command dispatch into functions. This is the file every new
  contributor reads first.
- Two user-facing error channels: operation failures print styled via
  `output::error`, but config/schema-load failures bubble to
  `tracing::error!` (`main.rs:16-19`) — different formatting, and
  subject to log filtering. Unify on one stderr format.
- `body` returns success even when the mutation caused warnings, while
  `set`/`add`/`rename` return failure
  (`commands/body.rs:26-33` vs `commands/mutation_output.rs:70-74`) —
  possibly intentional; decide and document it at the policy comment
  in `mutation_output.rs`.

**Server**

- The load-project-or-map-failure preamble is repeated five times
  (`api/items.rs:52-58`, `api/views.rs:87-92` and `:110-115`,
  `api/schema.rs:29-34`) while `api/timer.rs:249-257` already wraps it
  as `load_state_project` — share one helper.
- `get_view` (`api/views.rs:105-248`) is a 145-line handler with the
  filter-preview diagnostics recomputation inline (lines 154-214) —
  core-flavored logic in a handler; extract it.

**Deliberately deferred (do when the trigger appears)**

- A `CheckedView` newtype returned by `views_check`, making the
  "extraction panics unless views_check ran first" contract structural
  instead of documented (`view_data/mod.rs:9-12`) — do it when the
  next extraction call site appears.
- A `[workspace.lints]` table centralizing the clippy configuration
  that currently exists only as CI flags, plus deciding on
  `missing_docs`.
- Feature-gating the `ts-rs` derive in core so consumer builds stop
  compiling type-generation machinery only `cargo xtask gen-types`
  uses (`crates/core/Cargo.toml:27`).
- `title` is the one field name besides `id` baked into core logic
  (slug derivation in `operations/add.rs:237`). Sanctioned by
  CLAUDE.md's title-fallback rule, but ADR-002 should say so in a
  sentence — or the slug source becomes a config key.
