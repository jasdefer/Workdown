---
id: view-filter-editor
type: issue
status: in_progress
title: Build and edit a view's where filter from the UI
parent: view-authoring
depends_on: [remaining-read-views, schema-metadata-api, view-write-backend]
effort: "16h"
---

A view's `where:` clauses are static in `views.yaml` — adjusting what a view shows means editing YAML and reloading. Once users routinely want "items I'm working on this week", "items for team X", and so on, they need to narrow a view from the app.

This issue delivers a filter-building experience that does double duty, and is the reusable piece [[view-creation]] composes a new view's filter with — built once here, used in both places.

## What we want

- From any view, a user can add, change, and remove filter conditions and see the view re-narrow immediately.
- Conditions are built against the schema, not free text: the field, the operator, and (where the field constrains them) the value are chosen from what's valid, drawing on [[schema-metadata-api]]. An escape hatch for expressing a raw condition covers anything the guided builder doesn't.
- Two ways to keep a filter, both supported:
  - **For right now** — a personal, throwaway narrowing that doesn't change `views.yaml` and isn't shared.
  - **Saved** — written back into the view's `where:` in `views.yaml` via [[view-write-backend]], so it persists and is shared with the project.
- Whatever the builder produces means exactly what the same expression would mean when typed into `views.yaml` by hand — one filter grammar, one behavior.

## Acceptance

- A user can narrow any view without touching `views.yaml`, and choose whether that narrowing is just-for-now or saved into the view.
- A saved filter shows up in `views.yaml` and survives a reload; a for-now filter does not alter the file.
- A filter built in the UI and the same filter hand-written in `views.yaml` produce identical results.

## Out of scope

- A full SQL-like query builder — the guided builder plus a raw escape hatch covers the common cases; arbitrarily complex predicates remain a text-editor job.
- A saved-filter library / shared presets beyond a view's own `where:` — defer.
- Per-view display configuration (which fields show where) — that's [[view-display-config]].

## Design decisions

- **One grammar, one home — in Rust.** The filter syntax (`=`, `!=`, `>`, `~`, `/regex/`, comma-IN, `?`, `!…?`) is owned entirely by `core`, for *both* directions: parsing a clause string into a structured condition, and serializing a structured condition back to a string. The browser never reads or writes clause syntax — it deals only in structured `{field, operator, value}` conditions plus opaque raw strings it passes through untouched. This extends the operators-as-data approach [[schema-metadata-api]] already established (the UI picks an `Operator` enum from a server-provided list; it never learns the symbol). It means building the `Predicate`→string serializer `view-write-backend` deliberately deferred — this is the consumer that justifies it.
- **Structured clauses on the wire, both directions.** A clause crosses the wire as a tagged value: a `Comparison { field, operator, value }` (the guided-row case) or a `Raw { raw }` (the escape hatch). The read path returns the view's effective clauses already decomposed into this shape (so the editor can render existing filters as guided rows); the save and preview paths accept it. This revises the `SetViewFilter { where_clauses: [string] }` contract shipped in [[view-write-backend]] — acceptable because nothing consumes it yet.
- **Best-effort decomposition, raw fallback.** A clause that parses to a single comparison decomposes to a guided row; anything else (boolean trees, regex, IN with multiple values, anything `parse_where` accepts but a single row can't represent) comes back as `Raw` and renders as an editable raw-text row. The serializer correspondingly only emits single comparisons; raw clauses pass through verbatim.
- **"For right now" = server re-extract via the page URL.** An ad-hoc narrowing is carried as a URL query param on the existing view page (JSON-encoded structured clauses, so the browser still never serializes syntax). The page loader passes it to `GET /api/views/{id}`, which extracts using those clauses *instead of* the persisted ones (replace, not append — so removing a condition shows it removed) and never writes. This reuses the established URL-param → loader → `invalidateAll()` flow, is shareable/reload-survivable, and keeps the Rust engine the only evaluator — correct even for aggregating views (treemap/metric/charts), which client-side filtering cannot be.
- **"Saved" = `PATCH` + live-reload.** Saving sends the structured clauses to `PATCH /api/views/{id}`; `core` serializes them to strings and persists (save-with-warning per ADR-001). The file write triggers the SSE ping the app already listens on, which re-fetches the now-persisted view. The UI clears the ad-hoc URL param on save.

## Implementation plan

### Core (`crates/core`)

1. **Clause `Condition` model + (de)serialization** near `query::parse`/`query::types`:
   - A serializable `Condition { field, operator, value }` (value optional for `is_set`/`is_not_set`), and a `ClauseView` = `Comparison(Condition) | Raw(String)`.
   - `serialize_condition(&Condition) -> String` — single-comparison `Predicate`→clause-string (the deferred serializer), bounded: it does not emit boolean trees.
   - `decompose_clause(&str) -> ClauseView` — `parse_where`, then map a lone `Comparison` to `Condition`, everything else to `Raw`.
   - Round-trip tests: `decompose(serialize(c)) == Comparison(c)` for every operator; representative complex clauses decompose to `Raw`.
2. **`operations::view_write`** — change `set_view_filter` to accept the structured clause list, serialize each (raw passes through), then persist as today.
3. **Ad-hoc extraction** — a function that extracts a view's data using a supplied clause list instead of the view's persisted `where`, reusing the existing extract path, and returns the validation diagnostics for those clauses.

### Server (`crates/server`)

4. **Wire types** in `mutation_data` — the tagged clause type (request + response form), replacing `SetViewFilter`'s `Vec<String>`; register in the `gen_types` example.
5. **`PATCH /api/views/{id}`** — accept structured clauses; unchanged otherwise.
6. **`GET /api/views/{id}`** — accept an optional `?filter=` param (URL-encoded JSON clause list). When present, extract with it (ad-hoc, not persisted) and surface its diagnostics; when absent, use the persisted `where`. Either way, **echo the effective clauses back, decomposed**, so the editor can seed itself.
7. Integration tests: preview with `?filter=` doesn't mutate the file; save round-trips; the read response carries decomposed clauses; an invalid ad-hoc/raw clause surfaces a diagnostic without persisting.

### UX decisions (settled)

- **Surface:** one reusable filter-bar component that **slides down beneath the nav**, above the view content, so the result stays full-width and live while editing. Reused on every view page (and later by `view-creation`). Chosen over a right slide-over (collides with the item panel) and a modal (hides the result).
- **Live preview:** filters apply instantly as conditions complete (debounced ~300ms, written to the URL with `replaceState` so it doesn't spam history). No "Apply" button — matches Linear/Airtable/Jira/etc.
- **Saved vs for-now:** the live draft *is* the for-now filter. An unsaved-state affordance (`Filtered · unsaved` with **Save** / **Reset**) appears only when the draft differs from the persisted filter. Save persists via `PATCH`; Reset drops the URL param.
- **Operator labels:** hybrid — word plus a symbol hint (e.g. "is at least (≥)"), the word chosen per field type where it reads better (dates: "is on/after").
- **Multi-value (`is` / `is not`) on choice/multichoice:** checkboxes that build an IN clause (`status=open,in_progress`), round-tripped properly (see the core `decompose` extension below). Other operators stay single-value.
- **Raw escape hatch:** an "Add raw condition" affordance (not a per-row toggle) for OR across fields, regex, `parent.status`, etc. Decomposed-complex clauses also render as raw rows.
- **Validation feedback:** a banner in the filter bar (server diagnostics are view-scoped, not per-row).

### Core (Option Z extension)

7b. **Multi-value decomposition** — extend `query::clause::decompose_clause` to fold an `Or` whose branches are all `field = value` on the *same local field* into a single guided `Condition` whose `value` is the comma-joined list (`serialize_condition` already emits the IN form, so saving is unchanged). Round-trip test for the IN case.

### UI (`ui`)

8. **`schemaStore`** — expose `operators_by_type` (already in `SchemaData`, just not surfaced).
9. **API client** — `getView(id, filter?)` (adds the `?filter=` param) and `patchViewFilter(id, clauses)`.
10. **Filter editor component** (reusable — `view-creation` composes it): a list of condition rows, each a field picker (schema fields) → operator picker (valid operators for that field's type) → value picker (type-appropriate: choice/resource select, date input, etc.), plus an "add condition" and a raw-clause row for the escape hatch. Lives in a slide-over on the view page (mirroring `ItemPanel`).
11. **For-now wiring** — edits update the `?filter=` URL param (→ loader refetch → re-render). **Save** calls `patchViewFilter`, surfaces `diagnostics`/`mutation_caused_warning`, and clears the param; **reset** drops the param.

## Acceptance check mapping

- "Narrow any view without touching `views.yaml`; choose just-for-now or saved" → for-now URL param (step 6/11) + save path (steps 4/5/11).
- "Saved filter shows up in `views.yaml` and survives reload; a for-now filter does not alter the file" → step 5 persist + step 6 ad-hoc-doesn't-write, pinned by step 7 tests.
- "A filter built in the UI and the same filter hand-written produce identical results" → one-grammar-in-core (steps 1–3): the UI's structured clauses are serialized by the same `core` that parses hand-written ones, round-trip-tested in step 1.
