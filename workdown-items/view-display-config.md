---
id: view-display-config
type: issue
status: to_do
title: Per-view-kind display configuration (which fields show where)
parent: phase-04-visualization
depends_on: [remaining-read-views]
---

Every view kind makes implicit choices about which fields display where: which fields show on a board card, which columns appear in a table, what labels gantt bars carry, which fields populate graph node tooltips, which fields show in item previews. The `server` milestone hardcodes these per view kind. This issue establishes the pattern for letting users customize them — one design that applies across every view kind, with a consistent shape for persistence and UI.

## Open design questions

- **Where does the config live?**
  - Per-view in `views.yaml` (declarative, project-scoped, source-controlled).
  - Per-user via localStorage (personal, transient).
  - Hybrid: per-view defaults + per-user runtime overrides.

- **Config shape:**
  - Each view kind declares named display slots (e.g. `card.title`, `card.subtitle`, `tooltip.summary`, `bar.label`). Config picks which schema field fills each slot.
  - Slots are typed — some want strings, some want dates, some accept any field.
  - **Wire prerequisite:** rendering a *typed value* in a card/tooltip slot needs the field's type alongside its value (a bare wire value can't distinguish a date from a choice from a link). Table/tree `Column` already carries `field_type`; `CardField` (board cards, graph nodes) does not. Add `field_type` to `CardField` in `view_data::common` before slot-driven card/tooltip rendering can format dates/choices/links correctly and reuse `Cell.svelte`. (Surfaced by the graph node tooltip in [[remaining-read-views]], which shows the rendered body but no typed fields for exactly this reason.)

- **UI affordance for runtime override:**
  - Inline pickers per view page.
  - Per-view settings dialog.
  - URL params (good for sharing, awkward for many fields).

- **Interaction with `live-updates`:**
  - User-side overrides survive SSE invalidation.
  - `views.yaml` changes trigger a re-render.

## Acceptance

- Each view kind declares its display slots in `crates/core/src/view_data/<kind>.rs`.
- `views.yaml` schema gains an optional `display:` block per view, with shape derived from each kind's slots.
- A UI control lets the user override the configured fields per session.
- Choice persists across navigations (localStorage for v1; URL/session-scoped revisit later).
- The hardcoded board/table/etc. choices from the `server` milestone migrate to slot-driven.

## Out of scope

- Computed/derived display fields (e.g., "item's age from `created_at`") — defer.
- Field formatting customization (date format, number formatting) — separate concern.
