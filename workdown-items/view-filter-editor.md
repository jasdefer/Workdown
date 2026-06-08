---
id: view-filter-editor
type: issue
status: to_do
title: Interactive `where:` clause editor in the view UI
parent: phase-04-visualization
depends_on: [remaining-read-views]
---

Today a view's `where:` clauses are static in `views.yaml`. Adjusting filters means editing the YAML and reloading. Once multiple view kinds exist and users routinely narrow boards/tables/etc. to "items I'm working on this week", "items for team X", and so on, a UI affordance for filters becomes a real need.

Picture a filter chip bar above each view: each chip is one clause (`type=issue`, `assignee=alice`, …), clicking opens an editor that picks the field, operator, and value from the schema; an `+` button adds a new chip. The bar reads and writes the same `where:` grammar the CLI uses.

Belongs alongside [[view-display-config]] — both are "per-view UI controls" with the same open questions:

- **Persistence:** per-view in `views.yaml` (declarative, shared) vs per-user via localStorage (personal, transient) vs hybrid?
- **UI surface:** inline chip bar, separate dialog, URL params (good for sharing), or a mix?
- **Grammar coverage:** all `parse_where` operators (regex, ranges, set membership) or a subset for the chip UI with an "advanced" raw-text escape hatch?

## Scope

- Filter chip bar above the view, populated from the view's current `where_clauses`.
- Per-chip editor that respects the schema (choice fields → dropdown, dates → picker, strings → text).
- Add / remove / edit chips with immediate re-render against the current view data.
- Persistence shape settled here and shared with `view-display-config`.

## Acceptance

- A user can narrow any view by adding chips without touching `views.yaml`.
- Chips serialize/deserialize against the same `parse_where` grammar the CLI uses.
- Choice persists across navigations.

## Out of scope

- Full SQL-like query builder — chips cover the 80% case; complex predicates remain edit-the-YAML.
- Saved-filter library / shared filter presets — defer.
