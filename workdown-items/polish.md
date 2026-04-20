---
id: polish
type: milestone
status: to_do
title: Polish & dogfood
parent: phase-04-visualization
depends_on: [frontend]
---

Docs, automation, and release readiness. Decomposition deferred — scope depends on what actually shipped.

## Known scope (for context, not as issues yet)

- ADR documenting the visualization architecture (shared core, renderer adapters, UI-as-CLI-shell)
- README update: visualization workflow, `workdown serve` and `render`
- Optional `workdown init --install-hooks` for pre-commit render
- Reusable GitHub Action so teams can keep rendered views in sync via CI
