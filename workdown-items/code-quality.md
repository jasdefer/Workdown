---
id: code-quality
type: milestone
status: done
title: Code-quality cleanup
parent: phase-04-visualization
---

Targeted cleanup pass collecting maintainability findings that surfaced after the renderer set landed. Scope grows as items are added; nothing time-boxed.

## Themes

- Diagnostic system: collapse parallel variants, make source-routing structural rather than enumerative
- Walker primitives: unify the four single-Link upward chain walks
- Render module hygiene: shared escape helpers, test fixtures, `common.rs` naming
- Cross-cutting helpers in odd locations (e.g. `format_field_value` lives in `query/`)

Each theme lands as one or more issues under this milestone. Items are independent — order is by appetite, not dependency, except where explicitly noted.
