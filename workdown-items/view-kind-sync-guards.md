---
id: view-kind-sync-guards
status: done
title: Make the non-Rust schema mirrors fail loudly when they drift
parent: maintenance-review-2026-08
depends_on:
- render-flow-doc
---

## In plain words

When a new view kind is added, the Rust compiler points at every Rust
place that must be updated. Three mirrors of that knowledge live
outside its reach: a hand-maintained table in the web UI, the JSON
editor-autocomplete schema, and the docs. The JSON schema turned out
to be guarded already — the gap there is narrower than the review
thought. The web UI table is half-guarded — the type checker catches
one of its three lists — and every existing guard checks *shapes*
rather than the *list of kinds*, so a fourteenth kind missing from a
mirror still ships silently. Close that.

A fourth mirror with the same shape — `schema.schema.json`'s copy of
the field-type property matrix — was folded in from
[[assorted-small-fixes]]: it needs the same "does this JSON schema
still agree with the Rust enum" assertion, and writing that helper
once for both is the reason they belong together.

## The problem in detail

The three mirrors, and what guards each today:

- **`ui/src/lib/views/viewKinds.ts:1-10`** — a hand-maintained table
  of the thirteen view kinds with their slot and accepted-type lists,
  duplicating `views_check.rs` validation by convention. The blast
  radius is honestly bounded (the server re-validates, so drift is a
  UX gap, not corruption) and the file says so.

  Sharper than the review recorded: the file holds *three* lists, and
  they are not equally exposed. `VIEW_KIND_CONTROLS` is typed
  `Record<ViewType, Control[]>`, so TypeScript already refuses a
  missing kind once `ViewType` is regenerated — that one is guarded.
  `VIEW_KINDS`, the ordered picker list, is a plain `ViewType[]` and
  is not; nor are the per-slot accepted-type lists that mirror
  `views_check`. The existing `viewKinds.test.ts` does not close the
  gap either: its assertion is `toHaveLength(13)`, which still passes
  when a fourteenth kind is missing from the list. So the work here is
  narrower and more precise than "add a sync test" — it is the picker
  list and the accepted-type lists, not the controls record.

  It remains the one place the generated-types discipline is
  deliberately broken. Either guard those two lists, or serve the
  table from the backend, which already knows it.
- **`crates/core/defaults/views.schema.json`** — **already guarded**,
  contrary to the original review finding.
  `crates/core/tests/views_schema.rs` compiles the schema and runs it
  against the default `views.yaml`, a multi-view example, and a battery
  of bad shapes (29 tests). The remaining gap is narrower than "add a
  drift test":
  - the guard has *two* "all view types" fixtures
    (`full_example_with_all_view_types_validates` and
    `display_block_on_every_view_type_validates`) and both cover 12 of
    the 13 kinds — `gantt_by_depth` is missing from each. Corrected
    while writing [[render-flow-doc]]'s checklist; the review had
    recorded 11 of 13. Two hand-maintained copies of the same kind
    list is the milestone's own theme, so extending them means
    deriving both from one fixture, not editing two;
  - it validates *shapes*, never asserting that the set of `type:`
    values the schema accepts equals the `ViewKind` variants — a
    fourteenth kind absent from the schema passes every test.

  So: extend the example to all kinds, and add one assertion comparing
  the schema's accepted `type` values against the enum.
- **`docs/views.md`** — its view-kind table matched the `ViewKind` enum
  thirteen-for-thirteen when the review ran. A doc-drift test is
  optional; at minimum the adding-a-view-kind checklist
  ([[render-flow-doc]]) must name it.

### A fourth mirror, same shape (moved here from [[assorted-small-fixes]])

- **`crates/core/defaults/schema.schema.json`** — the field-type →
  allowed-properties matrix. [[schema-property-table]] made Rust the
  single source of truth (`model/schema.rs::allowed_field_properties`,
  an exhaustive match a new field type cannot dodge), but this file
  still hand-encodes the same matrix as `allOf` / `if`-`then` blocks
  for editor autocomplete. The CLI never reads it (ADR-005), so drift
  is a UX gap rather than a correctness one — the same trade, and the
  same fix, as `views.schema.json` above: one test asserting the JSON
  schema's per-type property sets equal what the Rust match allows.

  It landed here rather than in the grab bag because doing it beside
  the `views.schema.json` assertion means writing the
  compare-a-JSON-schema-against-a-Rust-enum helper once. Neither is
  worth the helper alone; together they are.

### A fifth mirror, already removed — not in this item's scope

`crates/core/examples/gen_types.rs` used to hold two hand-maintained
lists that had to agree: the `write_type::<T>()` calls, and an
`ALL_TYPES` name array the import resolver scanned. A comment asked
the next contributor to keep them in sync. Forgetting the call omitted
a `.ts` file; forgetting the array entry emitted the file but silently
dropped the `import type` line from every type referencing it — the
quieter and nastier of the two.

Found while writing [[render-flow-doc]]'s checklist and fixed there,
because it needed no helper: collecting the exports before writing any
file lets imports resolve against what was actually exported, so the
array is gone rather than guarded. Generated output is byte-identical.
Noted here only so this item's scope stays four mirrors, not five.

## Evidence this is worth doing

`metric-row-check-unification` broadened the `count`-with-`value` rule
from metric rows to every aggregate slot, and the Rust change passed
every gate while `views.schema.json` and `docs/views.md` still
described the narrow rule — silent drift in exactly two of the three
mirrors this item is about, introduced by a change that was reviewed.
Both were corrected afterwards, and the JSON schema now carries the
rule once as a shared `$def` referenced by all three definitions, with
tests. The `viewKinds.ts` mirror had no equivalent stake in that
change and so was not exercised.

## Objective

A failing test (or backend-served data) for each mirror that can
drift silently, so the compiler-plus-tests together cover every
touchpoint on the adding-a-view-kind checklist.

## Out of scope

- A registry or trait-object architecture for view kinds — reviewed
  and rejected as premature; exhaustive enums plus guards are the
  right cost at thirteen kinds.

Depends on [[render-flow-doc]] because that item writes the
adding-a-view-kind checklist this one makes enforceable; writing the
checklist twice, or guarding a list nobody has written down, is the
failure mode.

## Decisions taken

1. **Rust enumerates its own variants via `strum`** (`VariantArray` derive),
   not a hand-written `ALL` const. A hand list would make the guard itself
   the next mirror — self-defeating for an item whose whole point is "no
   fact written twice". The derive generates the list from the enum
   definition, so a new variant reaches every guard automatically. Applied
   to `ViewType`, `FieldType`, and the existing hand-written
   `FieldProperty::ALL`, which it replaces.
2. **The picker list (`VIEW_KINDS`) is guarded at compile time**, by a
   TypeScript assertion that the array covers every member of the generated
   `ViewType` union — not by a runtime test. It costs three lines, runs in
   the existing `npm run check` gate, and reports a missing kind by name.
   The `toHaveLength(13)` assertion it replaces is deleted.
3. **The accepted-type lists are generated from Rust**, not served at
   runtime and not left unguarded. The slot type lists move out of
   `views_check`'s check calls into a Rust table; `gen_types` emits the
   TypeScript from it. Chosen over a backend endpoint (the table is static
   and project-independent — a build artifact beats a round-trip) and over
   guarding kind coverage only (leaves the drift the item is about).

   This is adjacent to the "no registry architecture" exclusion below and
   was okayed explicitly: the per-kind exhaustive matches stay exactly where
   they are, and the table is data those matches read — not a dispatch
   mechanism.
4. **`views.schema.json`**: one all-kinds fixture, with the display-block
   variant derived from it programmatically rather than written a second
   time; plus one assertion that the schema's accepted `type` values equal
   the enum.
5. **`schema.schema.json`'s property matrix is guarded by behavioral
   probing**, not by parsing its `if`/`then` blocks: for each field type ×
   property pair, build a minimal field definition and assert the JSON
   schema accepts it exactly when Rust does. The probe is indifferent to how
   the schema expresses the rule. `field_property_allowed` becomes public —
   it is a genuine fact about the model, not an implementation detail.

   Scope stays at the eight type-restricted properties. `compute:` / `pull:`
   are excluded: reading the two sides side by side turned up a *behavioral*
   disagreement (the JSON schema forbids `compute:` on `string`, `choice`
   and `color`; `compute_check::expression_type_of` accepts all three), and
   settling which side is right is a real rule change that should not ride
   along in a test-only item. Filed separately.
6. **`docs/views.md` gets the drift test**, though the item called it
   optional — its kind table is machine-readable and the test is ~15 lines.
7. **Checklist row 12 is in scope** — `ViewRenderer.svelte`'s if/else chain
   becomes a kind → component map, so TypeScript enforces it for the same
   cost as decision 2. **Row 14** (the recording dot, reimplemented in six
   components) is **out**: it is duplication rather than a mirror, and the
   extraction has design questions of its own. Filed separately.

## What landed

Every row of the adding-a-view-kind checklist in `docs/architecture.md`
now has an enforcement mechanism except the recording dot, which moved to
[[recording-dot-extraction]]. Four things came out differently than the
decisions above anticipated, and one mirror turned out to be a fifth:

- **Row 12's guard is an exhaustiveness assertion, not a component map.**
  A `Record<ViewType, Component>` fights the per-variant prop types (each
  view component takes its own `ViewData` variant), so `ViewRenderer`
  keeps its `if`/`else` chain and its final `{:else}` branch passes `data`
  to `unrenderedKind(data: never)`. Same guarantee — a missing branch
  fails `npm run check` and the error names the kind — without weakening
  any component's props. The runtime placeholder survives for the
  stale-bundle case.
- **The picker list is gone rather than guarded.** `VIEW_KINDS` was one of
  *two* hand-kept kind lists in `viewKinds.ts`; the other was
  `KIND_LABELS`, already exhaustive by its `Record<ViewType, string>`
  type. The picker list is now derived from that record's keys, so there
  is no second list to guard. The compile-time assertion decision 2 called
  for was written and then deleted as redundant.
- **`config_check` was a third copy of the slot type lists**, with a
  comment saying so ("Each rule mirrors the matching slot in
  `views_check` exactly"). It reads `view_slots` now, which is why the new
  module has three consumers rather than two.
- **`docs/views.md` carried a second, shorter adding-a-view-kind
  checklist** that had already drifted (no UI rows, no docs row). Replaced
  with a pointer to the one in `docs/architecture.md`.
- **The property-matrix probe found six real disagreements**: the shipped
  `schema.schema.json` accepted `pattern:` and `allow_cycles:` on `date`,
  `boolean` and `list` fields, all six of which the CLI rejects. Fixed by
  restructuring that file's matrix into one block per field type, mirroring
  the shape of the Rust table — the overlapping type-list blocks are how
  the six went missing.

Two smaller things the work needed:

- `check_slot`'s `expected_label` parameter is gone. Every mismatch message
  is now worded from the slot's type list by `view_slots::describe`, so the
  prose and the list cannot disagree; `check_link_slot`'s arity parameter
  went the same way, since a link slot's accepted types say what its arity
  is.
- The three editor-schema guards (`schema_schema`, `resources_schema`,
  `views_schema`) each carried the same four helpers. They share
  `tests/json_schema/mod.rs` now — the "write the helper once" this item
  was bundled for, though it turned out to be the compile-and-assert
  harness rather than a compare-against-the-enum helper: the views side is
  a structural read of `oneOf` branches, the schema side a behavioral
  probe, and they have no shared shape.
- `vitest.config.ts` needed the `$lib` alias. Type-only imports are erased
  before Vitest sees them, so the generated table was the first *value*
  imported from `$lib` by a module under test.
