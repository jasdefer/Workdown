# ADR-008: Display roles — one vocabulary, one resolution ladder

**Status:** Accepted
**Date:** 2026-07-24

## Context

Every item-presenting view kind makes choices about which fields display where: what a board card shows, which columns a table has, what a graph tooltip lists, which field tints an item's surface. Before this decision those choices were made per kind and ad hoc — `table`/`tree` had a `columns:` slot, every kind had a top-level `title:` slot, cards hardcoded "all schema fields", and the color tint was pinned to the first `color`-typed field. Each new presentation knob would have grown another per-kind slot, and there was no way to set a project-wide preference once.

## Decision

A **closed vocabulary of display roles** — `title`, `subtitle`, `fields` (ordered list), `color` — applies uniformly to the item-presenting kinds (board, tree, table, graph, gantt and variants). Aggregate/chart kinds accept the block but ignore item-level roles. Each kind renders the roles in its own idiom and ignores roles it cannot place.

Each role resolves independently down a ladder, first match wins:

1. per-session runtime override (`?display=` on the serve API; never persisted),
2. the view's `display:` block in `views.yaml`,
3. project-wide `defaults.display` in `config.yaml`,
4. per-kind hardcoded fallback — exactly the pre-role behavior, so absent configuration changes nothing.

Boundaries: *structural* inputs that define a view (board `field`, gantt `start`/`end`, chart `x`/`y`) stay on `ViewKind`; roles are presentation only. Field *types* stay in `schema.yaml`.

Semantics fixed here: `fields` distinguishes absent (unset, inherit) from an explicit `[]` (show no fields) — it is an `Option` in the model. The `color` role takes a field name or the sentinel `none` (tint off at any rung); `none` is therefore a reserved field name, rejected by schema validation. The text roles are existence-only (any value renders as text, the virtual `id` included); `color` must name a `color`-typed field — `id` is rejected as it can never feed a tint. Role references are validated in both places they can be written — per view (`views_check`) and in the config defaults (`config_check`) — by one shared rule module, so the two scopes cannot drift. Surfaces without a view in context (the item detail) apply only rungs 3–4.

## Rationale

One vocabulary replaces N per-kind slots: a new view kind picks up every role for free, and users learn one concept instead of per-kind quirks. Per-role (rather than per-block) inheritance lets a view override just its tint while inheriting the project's title choice. Making rung 4 the exact legacy behavior made the whole feature adoption-optional. The sentinel is parsed once at the serde boundary into a real enum variant (`ColorRole::None`) so downstream code never compares magic strings.

A rejected alternative was keeping `columns:`/`title:` as-is and adding roles alongside — two vocabularies for the same concern. `columns` and `title` were folded into the block as a hard cutover instead (this repo was the only consumer at the time).

## Consequences

- `views.yaml`: top-level `title:`/`columns:` are rejected; `display.title`/`display.fields` replace them. A bare `type: table` is valid (columns fall back).
- A config-defaults role error is project-wide and non-blocking (views degrade to their fallback); a per-view role error marks that one view unrenderable — consistent with ADR-007's scope typing.
- The wire contract stays resolved-only: extractors emit finished values (e.g. `background` as `#rrggbb`), so renderers and the UI never re-implement the ladder.
- Session overrides can outlive the schema; extraction drops stale role references defensively instead of erroring.
