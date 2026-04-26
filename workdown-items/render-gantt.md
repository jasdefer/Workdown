---
id: render-gantt
type: issue
status: to_do
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
- `views_check`: constrain the `group` slot to non-link field types
  (string, choice, multichoice, list). Hierarchy grouping belongs to
  `render-gantt-by-initiative` / `render-gantt-by-depth`, not the basic
  Gantt.
- `GanttData`: keep current shape (`start_field`, `end_field`,
  `group_field`, `bars`, `unplaced`). No new fields here.
- `render_gantt(&GanttData) -> String` in
  `crates/cli/src/render/gantt.rs`, wired into `commands/render.rs`'s
  dispatch.

## Output

- Heading: `# Gantt`.
- Mermaid `gantt` block with `dateFormat YYYY-MM-DD`.
- Sections: one `section <value>` per distinct `group` value when
  `group` is set; deterministic order matching extractor; missing-value
  section last as `(no <field>)`. No sections at all when `group` is
  unset.
- Bars: `<title> :<id>, <YYYY-MM-DD>, <YYYY-MM-DD>`. `title` from
  `Card.title`, falls back to `id`. Workdown ids are a strict subset of
  Mermaid task-id syntax — no escaping needed.
- Empty `bars` → heading + empty Mermaid block (or heading only — pick
  during implementation).
- Inline unplaced footer when `unplaced` is non-empty:
  `> _<n> items dropped: <reason summary>._`
- Orchestrator (`commands/render.rs`) emits one `output::warning` per
  view with non-empty `unplaced`. Pattern reused for chart views later.

## Acceptance

- `render_gantt(&GanttData) -> String` exists, wired into
  `workdown render`.
- `views_check` rejects `group` slots pointing to `link`/`links` fields.
- Snapshot tests for: empty, bars only, bars with sections, with
  unplaced footer.
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
