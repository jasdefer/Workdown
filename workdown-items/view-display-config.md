---
id: view-display-config
type: issue
status: done
title: Per-view-kind display configuration (which fields show where)
parent: view-presentation
depends_on: [remaining-read-views]
---

Every view kind makes implicit choices about which fields display where: which fields show on a board card, which columns appear in a table, what labels gantt bars carry, which fields populate graph node tooltips. The `server` milestone hardcodes these per view kind. This issue establishes one pattern for customizing them across every kind. Design settled in [ADR-008](../docs/adr/008-display-configuration.md).

## Design (settled)

A small **closed vocabulary of display roles** — `title`, `subtitle`, `fields` (ordered list), `color` (reserved; filled by [[color-field-type]]) — applies uniformly to the **item-presenting** kinds (board, tree, table, graph, gantt + variants). Aggregate/chart kinds take no display roles. The markdown body stays always-rendered; it is not a role.

Each role resolves by precedence — **runtime override › per-view `display:` (views.yaml) › `defaults:` (config.yaml) › per-kind hardcoded fallback** (today's behavior, so an absent `display:` renders exactly as now). Each kind renders the roles in its own idiom (card badges / table columns / graph tooltip / bar label); a role a kind cannot place is ignored.

Boundaries: field *type* stays in `schema.yaml` (already enforced by `views_check`); *structural* inputs (board `field`, gantt `start`/`end`, chart `x`/`y`) stay on `ViewKind`; `title` folds into the vocabulary but stays resolvable cross-view for `ItemRef`.

Decisions:
- **`columns` migrates to `display.fields`** — one vocabulary. Hard cutover (this repo is the only consumer).
- **Wire:** add `field_type` to `CardField` (as `Column` has), so card/tooltip roles render typed values via the shared `Cell.svelte`. (Surfaced by the graph node tooltip in [[remaining-read-views]].)
- **Validation:** an unknown/unresolvable role field is rejected at `views_check` (consistent with `columns`/gantt `start`); text roles accept any stringifiable field.

## Acceptance

Delivered in two commits under this issue:

**(a) Declarative model**
- Display roles resolved in core `view_data` (`title`/`subtitle`/`fields`), so `render` and `serve` produce identical output.
- `views.yaml` gains an optional per-view `display:` block; `config.yaml` `defaults:` gains role keys; `views.schema.json` updated.
- `CardField` carries `field_type`.
- `columns:` cut over to `display.fields:` in the model and this repo's `views.yaml`.
- Hardcoded board/graph/etc. choices become role-driven, with today's behavior as the fallback.

**Known follow-up (from (a)):** validating `defaults.display` against
the schema — split out as [[display-defaults-validation]].

**(b) Interactive override**
- A UI control lets the user override the configured fields per session.
- Choice persists across navigations (localStorage for v1; URL/session-scoped later).
- Overrides survive SSE invalidation; a `views.yaml` change still triggers a re-render.

## Out of scope

- Computed/derived display fields (e.g., "item's age from `created_at`") — defer.
- Field formatting customization (date format, number formatting) — separate concern.
- `color` field *type* itself — [[color-field-type]]; this issue only reserves the role.
