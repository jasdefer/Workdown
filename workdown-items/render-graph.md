---
id: render-graph
type: issue
status: done
title: Graph renderer
parent: renderers
depends_on: [view-data-intermediate]
---

Render `GraphData` as a Markdown file written to `views/<id>.md` as a
Mermaid `flowchart TD` block. Supports an optional `group_by` slot on
graph views for subgraph nesting (hierarchy as containment) — when set,
the chosen `Link` field becomes a forest of `subgraph` boxes containing
their members; arrows for `field` are drawn between the leaf nodes and
across box borders. Single-arrow style; multi-relation overlays are
deferred to a follow-up issue.

## Pieces

- `views.yaml`: optional `group_by: <field>` on graph views (Link only,
  `allow_cycles: false`, no inverse names) — model + parser + JSON Schema.
- `views_check`: validation rules for `group_by` (unknown / wrong type /
  cyclic / inverse).
- `GraphData`: extra `group_by: Option<String>` and
  `groups: Option<TreeData>` fields; the tree is built by reusing the
  Tree extractor's forest walker, scoped to the filtered set.
- `render_graph(&GraphData) -> String`: heading + Mermaid block.
  - Workdown ids are a strict subset of Mermaid node-id syntax, used
    directly without aliasing.
  - Quoted labels (`A["..."]`) with minimal sanitization: `"` → `'` and
    `\n`/`\r` → space.
  - Item with children in the filtered tree → `subgraph id ["label"]`;
    item without children → leaf node `id["label"]`. Applied recursively.
  - Edges follow node declarations as plain `from --> to`.
  - Empty graph (no nodes) → heading only, no Mermaid block.

## Acceptance

- `render_graph(&GraphData) -> String` exists and is wired into
  `workdown render`.
- `views.yaml` accepts `group_by`; validated via `views_check`.
- Output renders correctly in GitHub preview for: empty, flat (no
  `group_by`), grouped, nested, antiparallel, self-loop cases.

## Out of scope (followups)

- `graph-multi-relation`: solid + dashed arrows for two relation fields.
- `graph-direction-config`: per-view `direction` (TD/LR/...).
- `graph-component-split`: split disconnected components into their own
  Mermaid blocks under per-component headings.
