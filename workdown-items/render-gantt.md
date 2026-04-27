---
id: render-gantt
type: issue
status: done
title: Gantt renderer
parent: renderers
depends_on: [view-data-intermediate]
---

Render the basic `GanttData` (start + end mode only) as a Markdown file
written to `views/<id>.md` via a Mermaid `gantt` block. Future input
modes (duration, predecessor) and split variants (by-initiative,
by-depth) land as separate issues — see "Out of scope".

## Pieces

- `views.yaml`: existing `start`, `end`, optional `group` slots on Gantt
  views are already modelled and extracted.
- `views_check`: `group` slot accepts `string`, `choice`, `multichoice`,
  `list`, `link`, `links`. One-hop link grouping is flat bucketing on
  the linked id — different from the chain-walking handled by
  `render-gantt-by-initiative` / `render-gantt-by-depth`.
- `GanttData`: keep current shape (`start_field`, `end_field`,
  `group_field`, `bars`, `unplaced`). No new fields here.
- Extractor: sort bars by `(section_index, start, id)`. `section_index`
  is the position in the schema-declared `values:` list for `choice`
  fields, the alphabetical rank of the group string for everything
  else, and `usize::MAX` for bars whose group value is missing
  (synthetic last section).
- `render_gantt(&GanttData) -> String` in
  `crates/cli/src/render/gantt.rs`, wired into `commands/render.rs`'s
  dispatch.

## Output

- Heading: `# Gantt`.
- Mermaid `gantt` block with `dateFormat YYYY-MM-DD`.
- Sections: one `section <value>` per distinct `group` value when
  `group` is set. Order: schema-declared for `choice`; alphabetical
  for everything else; missing-value section last as `(no <field>)`.
  No sections at all when `group` is unset.
- Bars: `<title> :<id>, <YYYY-MM-DD>, <YYYY-MM-DD>`. `title` from
  `Card.title`, falls back to `id`. Workdown ids are a strict subset of
  Mermaid task-id syntax — no escaping needed for the id. Title
  sanitizer replaces `:` `,` `#` `\n` `\r` with spaces and collapses
  consecutive whitespace.
- Empty `bars` → heading only, no Mermaid block. Mermaid renders empty
  `gantt` blocks inconsistently across viewers; a bare heading is the
  safe shape and matches `render_graph`'s precedent.
- Inline unplaced footer when `unplaced` is non-empty:
  ```
  > _<n> items dropped:_
  > _- missing 'start': "Title A", "Title B"_
  > _- invalid range: "Title C"_
  ```
  Item names are titles with id fallback; titles get `_` escaped to
  keep the blockquote italic intact. Reasons grouped per
  `UnplacedReason` discriminant; within a group, items in id order.
- Orchestrator (`commands/render.rs`) emits one `output::warning` per
  view with non-empty `unplaced`. Pattern reused for chart views later.

## Acceptance

- `render_gantt(&GanttData) -> String` exists, wired into
  `workdown render`.
- `views_check` accepts `link`/`links` for the `group` slot; the v1
  section heading is the linked id (no store-side title resolution).
- Snapshot tests for: empty, bars only, bars with choice sections in
  declared order, bars with link sections, missing-value section, and
  unplaced footer with multiple reason groups.
- Output renders correctly in GitHub preview.

## Out of scope (followups)

- `gantt-duration-mode`: `start + duration` input mode in the converter.
  Requires `duration-field-type`.
- `gantt-predecessor-mode`: `after + duration` via topo sort, same-day
  anchor.
- `render-gantt-by-initiative` (option F): new view type, multi-chart
  output partitioned by root of a link chain.
- `render-gantt-by-depth` (option E): new view type, multi-chart output
  partitioned by tree depth.
- Status mapping (`done`/`active`/`crit`), milestones (`:milestone`),
  and `group_by` (link-chain) on the basic Gantt are all deferred.
