---
id: readme-visualization-update
type: issue
status: done
title: "README: document the visualization workflow"
parent: polish
effort: "2h"
---

The README predates the visualization phase. `workdown render` appears
only as a bare command in the quick start — nothing says what it
produces, where the output lands, or that the rendered views are meant
to be committed. `views.yaml` is one table row. The Documentation
section links only the ADRs, although `docs/schema.md` and
`docs/views.md` exist. The Status section still names `v0.1.0-alpha.1`
as the first release.

## Scope

- A views-and-rendering section: defining a view in `views.yaml`,
  `workdown render` producing committed Markdown/SVG, `workdown serve`
  as the live counterpart.
- Link `docs/schema.md` and `docs/views.md` from the Documentation
  section.
- Fix the stale Status section.

## Decisions taken (2026-08-03)

1. **The README stays a tour, `docs/views.md` keeps the depth.** One
   views-and-rendering section with a single board example; the view
   kind catalog is linked, not repeated. Duplicating the catalog would
   create a second copy to keep current.
2. **The section shows a tiny before/after** — a three-line view
   definition and the Markdown it renders to. Self-demonstrating, no
   binary assets; a UI screenshot can come later and shouldn't block.
3. **The README names this repo as a living example** — one sentence
   pointing at `workdown-items/` and `views/`. The items are public in
   the repo anyway, and a real example beats a synthetic one.

## Out of scope

- Documenting individual schema features (computed fields, `when:`,
  resources) — `docs/schema.md` owns that depth.
