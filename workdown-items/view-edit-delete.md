---
id: view-edit-delete
status: in_progress
title: Edit and delete views from the UI
parent: phase-04-visualization
depends_on: [view-authoring]
---

The view-authoring milestone deliberately stopped at "create a view and
adjust its filter" — everything else about a persisted view (kind, slots,
name, deletion) stayed a text-editor job. In practice the first thing a
user wants after creating a view is to tweak its inputs, and the second
is to remove experiments. This issue closes both gaps: a view can be
edited and deleted from the UI, with `views.yaml` staying the source of
truth and nothing committed automatically.

## What we want

- An "Edit view" surface reachable from a view's page that reuses the
  create form, seeded with the view's current definition and filter, and
  saves the adjusted view back to `views.yaml`.
- Renaming a view from the same form (the id re-slugs from the new name).
- A "Delete view" action with a confirmation step.
- Everything written or removed is validated exactly like a hand edit,
  with the same save-with-warning diagnostics.

## Decisions taken

1. **Edit is full definition replacement.** The edit form is the create
   form seeded with the current view; kind switches are allowed. The
   server replaces the whole entry in place (same list position).
2. **Rename is included.** View ids are referenced nowhere else (no view
   links to a view; config names fields, not views), so rename is
   re-slug + duplicate check + replace. Because name → id is lossy, the
   form seeds the name by prettifying the id and sends `name` only when
   the user actually edited it — an untouched name never renames.
3. **Seeding via `GET /api/views/{id}/definition`**, returning exactly
   the update payload shape: the flat definition (no `id`, no `where`)
   plus the filter decomposed into structured clauses. What you GET is
   what you PUT back. Built on the parser's existing per-view
   serializer, exposed as `view_to_value` (inverse of `view_from_value`).
4. **`PUT /api/views/{id}`** replaces the view (`{name?, definition,
   filter}`); the filter-only `PATCH` stays for the filter bar.
5. **Delete also removes the rendered output file** (`<output_dir>/
   <id>.md`) if present, best-effort; rename removes the old id's file
   the same way. Removal outcomes ride in `info_messages`, mirroring the
   item-mutation pattern.
6. **No CLI commands.** The consistent rule today: work items (data) get
   mutation commands on every surface; `.workdown/` configuration files
   have zero CLI mutations — a terminal user has an editor. The UI
   mutates views only because a browser user doesn't. A views-only CLI
   would break that symmetry, not extend it.
7. **UI surface:** view page gains Edit/Delete actions; edit lives at
   `/views/{id}/edit` reusing the generalized form component; delete
   confirms, then navigates home.

## Acceptance

- A view created in the UI (or by hand) can have any input adjusted from
  the UI and the change lands in `views.yaml` as a hand edit would.
- Renaming from the form moves the entry to the new id and the UI
  navigates to it; an untouched name never changes the id.
- Deleting removes the entry (and its stale rendered file) after
  confirmation; unknown ids are a 404.
- Save-with-warning semantics are preserved on update: a definition that
  loads but fails cross-file validation is written and surfaced.

## Out of scope

- Reordering views in the navigation.
- Editing display roles beyond what the create form already covers
  (`display.fields` as "Columns"); other roles set by hand in
  `views.yaml` survive an edit round-trip untouched.
- CLI mutation commands for configuration files.
