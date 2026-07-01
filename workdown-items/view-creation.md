---
id: view-creation
type: issue
status: in_progress
title: Create a new view from the UI
parent: view-authoring
depends_on: [view-write-backend, schema-metadata-api, view-filter-editor, app-shell-navigation]
effort: "16h"
---

A user who wants a new view has to leave the app, learn the `views.yaml` shape for the kind they want, and hand-write it. This issue lets them assemble a view in the browser instead: choose a kind, fill in what that kind needs, optionally attach a filter, and save it.

This is the "Create view" entry point that [[app-shell-navigation]] deliberately deferred. The filter portion reuses the builder from [[view-filter-editor]] rather than reinventing it, and saving goes through [[view-write-backend]].

## What we want

- A single place in the UI to compose a new view: pick the kind, then supply the inputs that kind requires (driven by the schema metadata, so only valid fields are offered).
- A filter can be attached during creation using the same builder used to narrow existing views.
- Before saving, the user can tell whether the view is valid — a misconfigured view is caught here, not after it's written.
- On save, the view is written to `views.yaml`, appears in the navigation, and renders.
- Reachable from the navigation chrome, which currently has a slot waiting for it.

## Acceptance

- A user can go from "I want a board grouped by X" to a rendered, navigable view without editing `views.yaml` directly.
- The created view is a normal `views.yaml` entry — indistinguishable from a hand-written one and editable as such.
- Required inputs missing or incompatible for the chosen kind are surfaced before the view is saved.

## Out of scope

- Editing an existing view's kind or slots after creation — text-editor job (this issue only *creates*; filter changes live in [[view-filter-editor]]).
- Duplicating views, templates, or a view gallery — defer until the need is real.
- Per-view display configuration — that's [[view-display-config]].

## Design decisions

- **Slot spec is a static frontend table.** The 13 view kinds and their slots are a *fixed app vocabulary* (the `ViewType` enum + `RawView`), identical for every project — unlike fields/operators, which are project-schema-driven and must be served. So the form is driven by a documented TS table (kind → slots, each with label, required?, accepted field types), with a comment pointing at `views_check`/`convert_view` as the Rust source of truth. Field *options* per slot are filtered from schema metadata by the accepted types. The server (`add_view` + `views_check`) stays the validator, so a drift or wrong-typed field surfaces as a save diagnostic, never corruption. (Serving the spec was the alternative; rejected for v1 because it needs a `views_check` refactor to avoid moving the drift into Rust, and the vocabulary is stable.)
- **Reuse the filter builder by extraction.** Split a pure `FilterBuilder` (renders rows, emits `Clause[]`) out of `FilterBar`; the view page keeps a thin wrapper for seed/preview/save, the create form embeds `FilterBuilder` and folds its clauses into the create payload. (Alternative — a "mode" prop on `FilterBar` — grows conditional branches; extraction keeps each piece focused.)
- **All 13 kinds.** Including the heavy ones: `gantt`/`gantt_by_*` present the three input recipes (end / duration / after+duration) as a mode choice; `metric` gets a minimal repeatable-row editor.
- **Name → slug id, server-side.** The form takes a human *name* (spaces allowed); `core` slugs it to a kebab-case `id` (`"My Roadmap"` → `my-roadmap`) using the *same* `slugify` items use — lifted to a shared, `pub` location so there's one slug rule. Uniqueness is the server's job (`DuplicateId` → 409, surfaced).
- **Surface:** a dedicated `/views/new` page, reached from the nav's deferred "Create view" slot (roomier than a modal, and linkable).
- **Validity before save:** client-side gating (Save disabled until required slots are filled; field pickers only offer type-valid fields) plus the server's diagnostics on save. No live data preview (the item asks for validity feedback, not a render), no dry-run endpoint.

## Implementation plan

### Core
1. Lift `slugify` out of `operations::add` into a shared `pub` location (return a result with a reason), so item and view ids slug identically. Existing slug tests move/extend.

### Server
2. `CreateView` wire type → `{ name, definition }` (definition = kind + slots + optional `where`, *no* id). The `POST /api/views` handler slugs `name` → id via core, injects it into the definition, then calls the existing `add_view`. Map a bad name → `422`, duplicate id → `409`. Regenerate types. Integration tests: create a board by name, duplicate name → 409, blank/invalid name → 422, a created view then renders via `GET`.

### UI
3. **Extract `FilterBuilder.svelte`** from `FilterBar.svelte` (rows list + add-condition/add-raw + row edits, emitting `Clause[]`); refactor `FilterBar` to wrap it with the existing seed/preview/save. Confirm the filter editor still checks/builds.
4. **`lib/views/viewKinds.ts`** — the static slot-spec table + helpers (slots for a kind, whether a field's type fits a slot). Unit-tested.
5. **`/views/new` route** — name field; kind picker; per-kind slot inputs driven by the table (field pickers filtered by accepted types from schema metadata; gantt input-mode radio; metric rows); an embedded `FilterBuilder` for the optional filter; Save gated on required slots → `api.createView` → on success navigate to the new view; surface diagnostics/hard errors.
6. **`api.createView(name, definition)`** client method; wire the nav's "Create view" slot to `/views/new`.

## Acceptance check mapping

- "Board grouped by X → rendered, navigable view without editing yaml" → steps 2, 5 (+ SSE/nav refresh on save).
- "Created view is a normal `views.yaml` entry" → reuses `add_view` (step 2), which writes the same shape a hand edit would.
- "Required/incompatible inputs surfaced before save" → client gating + type-filtered pickers (steps 4, 5); server diagnostics as backstop.
