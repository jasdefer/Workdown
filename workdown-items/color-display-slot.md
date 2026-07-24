---
title: '`color:` display role — choose which color field tints a view'
type: issue
status: done
parent: view-presentation
depends_on:
- color-field-type
- view-display-config
effort: 1d
---

Promote the `color:` display role — reserved in [[view-display-config]]
and stubbed by [[color-field-type]]'s first-color-field convention —
to a real role, so a project with several `color`-typed fields can
choose which one tints each view (e.g. board tinted by `team_color`,
gantt by `risk_color`).

## Design

**Resolution ladder** — identical to the existing roles (`title`,
`subtitle`, `fields`):

1. per-session override (Display bar),
2. per-view `display: { color: <field> }` in `views.yaml`,
3. project default `defaults.display.color: <field>` in `config.yaml`,
4. fallback: the **first `color`-typed field in schema order** —
   today's behavior, so absent configuration changes nothing.

**Off switch:** `color: none` at any rung disables tinting for that
view outright (a dense table may want no wash regardless of schema).
`none` needs to be a sentinel since every other value names a field;
a field literally named `none` is the user's own foot-gun and may be
rejected outright.

**Validation:** a per-view `color:` role must name a `color`-typed
schema field (or the sentinel) — anything else is a `views_check`
config diagnostic, consistent with `fields`/`columns` references.
Validating rung 3 (`defaults.display`) is
[[display-defaults-validation]], which should land after this so it
covers all four roles at once.

**Wire contract unchanged:** per-item payloads keep their resolved
`background: Option<String>`; only *which field feeds it* becomes
view-resolved. `resolved_background` (core `view_data`) grows a
view-aware resolution instead of always scanning for the first color
field; `item_data` (the detail surface, which has no view in context)
applies the project-wide rungs only — `defaults.display.color` from
`config.yaml`, then the schema-order fallback. (Originally shipped
schema-order-only; the `defaults.display` rung was added in the
pre-PR review pass, since rung 3 is project-wide by definition.)

**Display bar:** gains a color-field selector (options: the schema's
color fields + "none" + "default"), stored per session with the same
localStorage mechanics as the other role overrides.

## Acceptance

- `views.yaml` accepts `display: { color: <field> }`; `config.yaml`
  accepts `defaults.display.color`; `views.schema.json` updated.
- The ladder resolves in override › view › default › first-color-field
  order; `color: none` renders the view untinted.
- A `color:` role naming a missing or non-color field produces a
  `views_check` diagnostic.
- With no configuration anywhere, every view renders exactly as today.
- The Display bar can switch the tint field (and off) per session, and
  the choice survives navigation and SSE refreshes.

## Out of scope

- Validating `defaults.display` (rung 3) — [[display-defaults-validation]].
- Multiple simultaneous color roles per view (e.g. stripe from one
  field, wash from another) — no known need.
- Tinting aggregate/chart views — unchanged from [[color-field-type]].
