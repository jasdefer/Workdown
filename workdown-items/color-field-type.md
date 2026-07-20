---
id: color-field-type
title: Add `color` field type with background tinting
type: issue
status: in_progress
parent: view-presentation
depends_on:
- mutations-slice
effort: 2d
---

Introduce `color` as a built-in field type holding a single color value,
and use it to tint the surfaces that represent an item across all views
that render a discrete per-item surface. Follows the generic-type
philosophy (ADR-002): the type drives behaviour, no field name is magic.
By convention the **first** `color` field in schema order backs the
background, mirroring how the first compatible `choice` field backs a
board.

Precedent for adding a type: [[duration-field-type]]. The
"which field tints what" question is the display-slot concern owned by
[[view-display-config]] — this item ships the simple first-color-field
convention now and graduates to a real `color:` slot when that lands.
Depends on [[mutations-slice]] for the editing surface and the
`--card-bg` hook the card styling leaves in place.

## Design

**Values:** either a CSS hex color (`#rgb` / `#rrggbb`,
case-insensitive) or a name from the built-in palette. The palette is
hardcoded in core (GitHub-label-style): `red`, `orange`, `yellow`,
`green`, `blue`, `purple`, `pink`, `gray` — each pinned to one fixed
hex value. No schema configuration of the palette in v1.

**Canonical form / round-trip:** hex is lowercased and `#rgb` expanded
to `#rrggbb`; palette names are stored verbatim (lowercased) — `red`
stays `red` in the frontmatter, so if the pinned palette values are ever
tuned, existing items pick the change up automatically.

**Resolution:** core owns the name→hex map and a single resolution
function. Names are an *authoring* form only — the moment a value enters
any computation (filtering, rendering, comparison) it is resolved to hex
first. Nothing outside core ever interprets a name.

**Validation (save-with-warning per ADR-001):** anything that is not
valid hex or a palette name — `#rrggbbaa`, `rgb()`, `hsl()`, unknown
names like `teal` — still writes the file and emits a new
`FieldValueError::InvalidColor` carrying the offending value and the
allowed names, so the diagnostic teaches the palette. An invalid stored
value renders as unset (no tint).

**Query semantics:** the filter builder offers `eq` / `ne` plus the
universal `is_set` / `is_not_set`. Equality compares **resolved hex**,
so `color == red` matches an item that stores red's hex literally. No
ordering, no `contains`, no aggregation. A color field is *not*
board-compatible, despite superficially resembling a choice field.

**API contract (settled in the backend phase, so the UI phase never
reopens core):**

- Every per-item view payload — board card, table row, item detail,
  tree node, graph node, gantt bar, treemap leaf — carries
  `background: Option<String>`: the resolved `#rrggbb` of the item's
  first `color` field in schema order, absent when unset/invalid.
- The schema payload carries the palette (name + resolved hex) so the
  editor can render swatches without a second hardcoded copy in TS.

**Editor:** a swatch row built from the served palette (clicking a
swatch writes the *name* into frontmatter) plus a native
`<input type="color">` for custom hex, plus a clear affordance for
optional fields (an unset color field has no tint).

**Background rendering:** the surface applies the resolved color and
picks black or white text by the color's relative luminance:

- linearize the sRGB channels, `L = 0.2126·R + 0.7152·G + 0.0722·B`;
- text is black when `L` is above the contrast threshold, white below.

The chosen color is **absolute data** (like a label color), so it
renders the same in light and dark mode — only the contrasting text
flips per-color, not per-theme. The neutral default (no color set) is
the ordinary background, so dark/light still applies normally there.

*How* the color manifests (full background fill vs. subtle tint vs.
edge stripe vs. border/glow) is decided per view, visually, during the
UI phase — a full fill may suit a board card while a stripe reads
better on a dense table row.

**Decided (board cards): stripe + tint.** A 4px full-strength stripe on
the card's left edge carries the exact hue; the surface mixes the color
into the theme background via `color-mix` at the shared
`--tint-strength` token (14%). Normal theme text stays (no black/white
flip needed at that strength); the stripe keeps its hue on hover while
the neutral borders go muted. Chosen against full-fill (kills the muted
text hierarchy, loud in a column), tint-only (hues blur), and
stripe-only (reads as a label, not a tinted surface).

## Plan

**Phase 1 — backend.** Type, coercion + validation, palette +
resolution, query semantics, and the API contract above (including
regenerated TS types). After this phase the UI only ever sees finished
hex strings.

**Phase 2 — UI, view by view.** All views with a per-item surface:
board cards, table rows, standalone/detail editor (these already have
the `--card-bg` hook or are closest to it), then tree nodes, graph
nodes (cytoscape currently bakes colors from tokens at style-build
time — needs per-node data-driven styling), gantt bars, treemap leaves.
Pick the rendering treatment per view by trying variants; hover/focus
states must remain visible on tinted surfaces.

## Pieces

- `FieldType::Color` + `FieldTypeConfig::Color` in
  `core::model::schema`; `FieldValue::Color(String)` (canonical form).
- Palette map + resolution function in core; equality on resolved hex.
- Coerce path validates hex grammar and palette names; new
  `FieldValueError::InvalidColor { value, allowed }`.
- `defaults/schema.schema.json`: add `color` to the type enum.
- Curated operator set for color in the filter builder (`eq`, `ne`,
  `is_set`, `is_not_set`).
- Per-item view payloads gain resolved `background`; schema payload
  gains the palette. Regenerate TS types.
- Luminance → text-color helper in the UI (one small pure function,
  unit-tested).
- `FieldEditor` color arm: palette swatches + native hex picker +
  clear affordance.
- Per-view surface wiring (phase 2 list above); neutral default when
  absent.

## Acceptance

- `schema.yaml` accepts `type: color`; invalid values (bad hex, unknown
  names) save-with-warning and the diagnostic lists the allowed names.
- `red` round-trips as `red` in the frontmatter; `#ABC` round-trips as
  `#aabbcc`.
- Filtering `color == red` matches items storing the equivalent hex.
- The color editor sets (swatch → name, picker → hex) and clears the
  value; the file changes on disk.
- An item with a value tints its per-item surface in every phase-2
  view, with readable (auto black/white) text in both themes.
- Hover/focus states remain visible on tinted surfaces.
- Items without a color keep the neutral default background.
- Existing field types unaffected (regression-tested).

## Out of scope

- CSS color forms beyond hex + palette names: `rgb()` / `hsl()` /
  alpha / arbitrary CSS named colors.
- User-configurable palette (schema-defined names/values).
- A per-view `color:` display slot — that's [[view-display-config]];
  this uses the first-color-field convention until then.
- Tinting pure aggregate views (bar / line / heatmap / metric /
  workload) — they render aggregates, not per-item surfaces. (Treemap
  *leaves* are per-item and are in phase 2, unlike the earlier draft of
  this item claimed.)

## Note on scheduling

No `start_date`/`end_date`/`duration` here on purpose: the parent
milestone [[view-presentation]] sets those manually as aggregates, so
repeating them on a child would raise an `AggregateChainConflict`. The
milestone owns the calendar span; this leaf carries only `effort`, which
rolls up as a sum.
