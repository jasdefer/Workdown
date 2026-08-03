---
id: resource-option-lists
type: issue
status: done
title: Validate resource references and render resource pickers
parent: polish
depends_on: [mutations-slice, schema-metadata-api]
effort: "4h"
---

[[schema-metadata-api]] loads `resources.yaml` and exposes each
resource's entries in the schema metadata, so the UI knows the valid
values for a `resource:`-backed field. Two gaps remain once it lands:
the values aren't *validated* against items, and the editor still
renders resource fields as free text. This issue closes both.

## Scope

- **Core: validate** that a `resource:`-backed field's value matches an
  `id` in the referenced section. Save-with-warning per ADR-001 (a bad
  reference warns, doesn't hard-reject) — new diagnostic kind, e.g.
  `UnknownResourceRef { field, section, value }`. A field pointing at a
  resource section that doesn't exist is a schema/config diagnostic.
- **UI: render resource fields as a picker** (single → select, and the
  pattern extends to a future multi-resource field) instead of free
  text, in both the detail editor and the create form, populated from
  the option lists [[schema-metadata-api]] serves.

## Acceptance

- A project with `people` entries validates a work item whose `assignee`
  is a known id without warning, and warns on an unknown id (file still
  saves).
- The item editor renders `assignee` (and any `resource:` field) as a
  picker populated from the resource, not a text box.

## Decisions taken (2026-07-31)

1. **An unknown value warns, per item, wherever the value came from** —
   hand-written, a stamped `default:`, or derived by `compute:`/`when:`.
   Warning rather than error (unlike `BrokenLink`, the structurally
   identical check): a dangling `parent` breaks tree and graph
   traversal, an unknown assignee breaks nothing — the value still
   renders, groups and filters. `resources.yaml` is people data that
   lags reality, so a new hire assigned before someone edits the file
   must not fail `workdown validate` in CI.
2. **An unset value never warns.** A `required` field left empty is
   already `MissingRequired`; nothing here duplicates it.
3. **An unusable option set is reported once, at config scope, and
   switches the per-item check off for that field** — the same
   suppression `compute_check` does for broken compute configs. N
   identical item findings for one missing config points at the wrong
   file N times. Two cases, two severities:
   - Section named but not declared while `resources.yaml` exists
     (`resource: peple`) → **error** pinned at `schema.yaml`. A typo,
     unambiguous.
   - Section declared but empty, or `resources.yaml` absent entirely →
     **warning**. The list just isn't filled in yet.
4. **`default:` is validated against the section**, mirroring the
   `choice` precedent (`parser/schema.rs:1105`, "default 'x' is not in
   the allowed values"). Two rules, both schema-scope:
   - A literal default outside a populated section is an error — every
     added item would carry a bad value.
   - A *generator* default (`$uuid`, `$filename`, `$filename_pretty` —
     the ones `string` accepts) on a resource-backed field is refused
     outright: it can never produce a valid entry.
   Unlike choice's, these checks need `resources.yaml` loaded, so they
   live in `resources_check`, not the schema parser — same reason
   `compute_check` runs at project load.
5. **The per-item check runs inside `Store::load`**, next to the
   existing broken-link loop, not as a project-level pass. The mutation
   path builds its own store (`operations/set/mod.rs:276`) and never
   calls `load_project`, so a project-level pass would leave
   `workdown set my-task assignee carol` silent until someone later ran
   `validate` or `render`. Placing it after the derive passes is what
   gives decision 1 its "wherever the value came from".
6. **`workdown init` ships a populated `people` section** — the
   `alice` / `bob` examples already in `defaults/resources.yaml`,
   uncommented. The scaffolded schema points `assignee` at that list,
   so an empty default section would have meant either a warning on a
   fresh project or a carve-out to suppress one. Consequence, accepted
   deliberately: the check is live from item one, so a real user's
   first `workdown set my-task assignee justus` warns. That is the
   moment the feature explains itself and the fix is one line.
7. **Unknown values survive the picker.** A plain `<select>` cannot
   represent a value outside its options — it would render blank and
   the next commit would erase it, on exactly the items the warning
   flags. The current value is retained as a marked option instead.
   (A `datalist` type-ahead is the better answer once a `people` list
   runs to hundreds of entries; a select matches how `choice` and
   `link` already render, and consistency wins now.)
8. **`list` fields get the picker too**, though the scope note below
   called multi a future concern: validation has to walk list elements
   regardless, and validating a field while still editing it as
   free-text chips is the inconsistent outcome. Rendered like `links`
   (multi-select over the option set).
9. **The view filter builder gets the picker too** —
   `FilterRow.svelte:203` has the identical "resource-backed: free
   text" fallthrough and the option list is already on the wire.
   Validating what ends up in a `where:` clause stays with
   [[where-clause-value-validation]].
10. **Read-side labels stay out** — table cells and board cards keep
    showing the stored id while pickers show `name ?? id`. That is a
    display-role question, filed as [[resource-label-display]].

## Out of scope

- Loading `resources.yaml` and serving the option lists — that lands in
  [[schema-metadata-api]].
- Editing `resources.yaml` from the UI — stays a text-editor job.
- Resource entry display-field customization (which field is the label)
  — default to `name` then `id`; revisit in [[resource-label-display]].
- A dedicated `resource` field *type* — `resource:` stays a constraint
  on existing types (string/list), per the current schema model.
