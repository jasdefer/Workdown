---
id: assorted-small-fixes
status: done
title: Grab bag of small consistency fixes from the review
parent: maintenance-review-2026-08
---

## In plain words

Everything the review flagged that is real but small — minutes to a
few hours each, none urgent, none blocking anything. The original plan
was to fold each one into whichever milestone item touched the same
area, but by the time this came up the remaining open siblings were
documentation and testing work that touch none of this code. So it was
done as its own batch.

Nothing here changed behavior users can see, except the two noted
below.

## What was done

**Core**

- Coercion moved out of the store layer to its own top-level module.
  The schema parser had been reaching through the store to get at it
  while the store depends on the parser — a circle around a module
  holding no store state.
- One name per helper: the YAML type-name used in error messages (the
  parser and coercion each had a copy, disagreeing only on whether a
  tagged value is called "tagged" or "tagged value"), and the
  is-this-a-leaf test the compute, roll-up, derive, and required passes
  all ask of the same reverse-link index.
- **Visible:** a `pattern:` that is not a valid regex is now caught
  once during schema parsing, naming the field, instead of once per
  item that uses the field. Patterns are compiled once per load rather
  than once per value. The per-item `InvalidPattern` diagnostic can no
  longer occur and is gone.
- `NodeGrid` owns the derive scheduler's `slot × item_count + position`
  encoding, which was written out by hand at ~15 sites.
- `Schema::new(fields, rules)` is the only way to build a `Schema`;
  twenty sites had each remembered to derive the inverse table by hand.
- Dropped a vestigial `_expected: Ordering` parameter, the
  fully-qualified `std::string::String` leftovers, and `add`'s bare
  `fs::write` (now `write_file_atomically`, like every other mutation).

**CLI**

- `main.rs` reads the current directory once instead of thirteen times,
  and dispatches from one flat match instead of an outer match plus an
  inner one plus an `unreachable!`. Templates, the set-mode choice, and
  init's optional hook install are functions.
- **Visible:** a config or schema that will not load now prints like
  every other error. It used to go through `tracing`, which formatted
  it differently and let `WORKDOWN_LOG` drop it.
- The exit-code policy is written down at the comment that tried to
  state it: a warning this mutation caused fails the command, a
  pre-existing one does not. `body` always succeeding turned out to be
  that rule, not an exception — body text goes through no validation,
  so it can cause no warning. No behavior change; see the resolved
  question below.

**Server**

- The cold-load-and-map-the-failure preamble is one helper next to
  `AppState`, used by all six handlers.
- `get_view` is ~75 lines instead of 145: the filter-preview path is
  two named functions.

**Previously deferred, done anyway**

- `CheckedView` — `view_data::extract` panicked unless `views_check`
  had cleared the view, a contract stated only in a doc comment.
  Wrapping is now the only way to obtain extract's first argument.
  This also collapsed two copies of the clearing rule: the CLI
  collected error-severity view ids into a set, the server asked
  `any()` per view. `workdown render` over this repo produces
  byte-identical output.
- ADR-002 now names its two exceptions — `id` and `title` — and says
  why each is deliberate, so the slug-source question does not have to
  be re-derived from `operations/add.rs`.

## Questions that were open, and how they resolved

**`body` vs `set` exit codes.** The review flagged `body` returning
success on warnings while `set`/`add`/`rename` return failure, and
asked for a decision. There was nothing to decide: the shared rule is
that only a warning *caused by* the mutation fails the command, and a
body edit cannot cause one. The two already agreed. Written down
rather than changed.

## Deliberately not done, with reasons

- **The `schema.schema.json` property-matrix mirror** moved to
  [[view-kind-sync-guards]], which is the item about exactly this
  failure shape and already has to write the
  compare-a-JSON-schema-against-a-Rust-enum helper for
  `views.schema.json`. Neither mirror is worth that helper alone.
- **Feature-gating the `ts-rs` derive.** Measured before deciding:
  compiling `workdown-core` with the 88 derives in place takes ~5.9 s;
  with every derive and the dependency stripped out, ~6.6–7.1 s — no
  difference beyond noise. `ts-rs` adds four small crates to the tree
  (`ts-rs`, `ts-rs-macros`, `lazy_static`, `termcolor`); everything
  else it wants (syn, quote, serde, chrono, thiserror) is already a
  direct dependency. The premise — that consumer builds pay for
  machinery only `cargo xtask gen-types` uses — does not hold, and 88
  `cfg_attr`s is a real cost against a benefit that is not there.
- **`missing_docs`.** Measured: 846 public items lack a doc comment
  (818 in core, 26 in server). That is a documentation project, not a
  lint setting. Not enabled.
- **A `[workspace.lints]` table.** With `missing_docs` declined and no
  clippy lints enabled beyond the defaults, the table would hold only
  `warnings = "deny"` — which makes every local `cargo build` hard-fail
  on a warning mid-edit, while CI already catches the same thing with
  `-D warnings`. The table becomes worth adding when there is a lint
  decision to record in it.
