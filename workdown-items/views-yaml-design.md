---
id: views-yaml-design
type: issue
status: to_do
title: Design views.yaml shape
parent: foundation
---

Produce an initial design for `.workdown/views.yaml`. Output is a documented example file, a Rust struct representation, and a short design note — not the formal JSON Schema (that's the next issue).

## Starting shape (to validate in this issue)

```yaml
views:
  - id: status-board
    type: board
    field: status
    output:
      markdown: views/board.md
      html: views/board.html
  - id: dependency-graph
    type: graph
    field: depends_on
    output:
      mermaid: views/graph.md
```

## Questions to answer

- Minimal required fields per view entry (probably `id`, `type`, `field`)
- How view entries reference `schema.yaml` (by field name only, or richer)
- Multiple output formats per entry vs one per entry
- Do views support filters (e.g. `type=issue` only)? Same expression language as `workdown query --where`?

## Deliverables

- Example `views.yaml` committed as a fixture or doc
- Rust structs to parse it
- Short design note (section in a new `docs/views.md`)

## Out of scope

- Formal JSON Schema validation (next issue)
- Rendering (`renderers` milestone)
- Theming config — revisit when we know what we need
