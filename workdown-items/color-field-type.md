---
id: color-field-type
title: Add `color` field type with background tinting
type: issue
status: to_do
parent: view-authoring
depends_on:
- mutations-slice
effort: 2d
---

Introduce `color` as a built-in field type holding a single color value,
and use it to tint the surfaces that represent an item — board/graph
cards, table rows, and the standalone/detail editor. Follows the
generic-type philosophy (ADR-002): the type drives behaviour, no field
name is magic. By convention the **first** `color` field in schema order
backs the background, mirroring how the first compatible `choice` field
backs a board.

Precedent for adding a type: [[duration-field-type]]. The
"which field tints what" question is the display-slot concern owned by
[[view-display-config]] — this item ships the simple first-color-field
convention now and graduates to a real `color:` slot when that lands.
Depends on [[mutations-slice]] for the editing surface and the
`--card-bg` hook the card styling leaves in place.

## Design

**Storage & input:** a CSS hex color string, validated on coerce.
Accept `#rgb` / `#rrggbb` (case-insensitive); reject anything else with
an `InvalidColor` field-value error (save-with-warning per ADR-001 — the
file still writes). Stored and round-tripped verbatim as the lowercased
`#rrggbb` form.

**Editor:** native `<input type="color">` in `FieldEditor`, plus a small
clear affordance for optional fields (an unset color field has no tint).

**Background rendering:** the surface sets `--card-bg` (the hook added
with the card styling) to the color, and picks black or white text by
the color's relative luminance:

- linearize the sRGB channels, `L = 0.2126·R + 0.7152·G + 0.0722·B`;
- text is black when `L` is above the contrast threshold, white below.

The chosen color is **absolute data** (like a label color), so it renders
the same in light and dark mode — only the contrasting text flips
per-color, not per-theme. The neutral default (no color set) is the
ordinary card background, so dark/light still applies normally there.

## Pieces

- `FieldType::Color` + `FieldTypeConfig::Color` in
  `core::model::schema`; `FieldValue::Color(String)`.
- Coerce path validates the hex grammar; new
  `FieldValueError::InvalidColor { value }`.
- `defaults/schema.schema.json`: add `color` to the type enum.
- `schema_data::FieldSchema` already carries `field_type`; the UI editor
  adds a `color` arm. Regenerate TS types.
- Luminance → text-color helper in the UI (one small pure function,
  unit-tested).
- Card / cell / standalone surfaces read the first color field and set
  `--card-bg` + computed text color; neutral default when absent.

## Acceptance

- `schema.yaml` accepts `type: color`; invalid hex saves-with-warning.
- The color editor sets/clears the value; the file changes on disk.
- An item with a value tints its card/row/detail background, with
  readable (auto black/white) text in both themes.
- Items without a color keep the neutral default background.
- Existing field types unaffected (regression-tested).

## Out of scope

- Named colors / `rgb()` / `hsl()` / alpha — hex only for v1.
- A per-view `color:` display slot — that's [[view-display-config]];
  this uses the first-color-field convention until then.
- Tinting aggregate/chart views (bar/line/heatmap/treemap/metric/
  workload) — they don't render discrete per-item surfaces.

## Note on scheduling

No `start_date`/`end_date`/`duration` here on purpose: the parent
milestone [[view-authoring]] sets those manually as aggregates, so
repeating them on a child would raise an `AggregateChainConflict`. The
milestone owns the calendar span; this leaf carries only `effort`, which
rolls up as a sum.
