# Changelog

User-facing changes per release. `dist` splices the section matching the
tagged version into the top of that version's GitHub release page, so
entries are written for people using workdown, not for people reading
its source — internal refactors are deliberately absent.

## Unreleased

### Fixed

- A required field filled by `pull` is no longer accused of being empty
  before the pull has had its chance to run — previously a false error
  when the pull succeeded, and a doubled or contradictory pair of errors
  when it could not. The check now waits for a pull the way it already
  waited for computed, rolled-up and conditional values, and an
  incomplete pull is reported exactly once.

## 0.2.3 - 2026-08-22

### Fixed

- Commands no longer fail when the work-items directory is absent. Git keeps
  no empty directories, so a fresh clone of a project with no items has none:
  `workdown add` now creates it, and every command that reads a project treats
  it as "no items yet". A directory that genuinely can't be read is still an
  error.
- Clicking an item in the graph and treemap views opens its detail panel, the
  way it already did in the board, tree, table and gantt views. Treemap
  rectangles are also reachable by keyboard; the graph is mouse-only, since it
  draws to a single canvas.

## 0.2.2 - 2026-08-21

### Added

- Edit and delete persisted views from the web UI. Creating a view and
  adjusting its filter were already possible; everything else about a saved
  view was a text-editor job. Deleting one also removes the rendered output
  file that `workdown render` leaves behind.
- An effort timer in the web UI: start, pause and stop recording time against
  an item, written to the project's duration field on stop, rounded to
  minutes. One timer runs at a time and its state lives in the running app,
  never in the repo.
- `defaults.effort_field` in `config.yaml` names the duration field the timer
  writes to. Leaving it unset disables the timer.

### Fixed

- A duration delta applied to an item with no value for that field starts from
  zero instead of being refused.

## 0.2.1 - 2026-08-05

### Added

- `pull` field config — read a field from the items a link points at, one hop
  forward, and reduce the values. With `end = start + duration` this completes
  forward scheduling from `depends_on` and `duration` alone: root items carry a
  manual start, everything downstream follows. A hand-written value always
  wins; `pull` only fills absences.

## 0.2.0 - 2026-08-03

### Changed

- **Breaking:** `=` in filters now always means literal equality. Previously a
  comma in the value silently turned it into an OR, while `!=` treated the same
  comma literally. List membership is now `in` / `not in`.
- An absent field satisfies the negative comparisons (`!=`, `not in`) and fails
  the positive ones, consistently. Add `field?` as a second clause to require
  the field be present.

### Added

- `when:` on a field derives its value from the first matching condition. A
  hand-written value always wins. The default schema ships status-derived card
  colors built on it.
- Comparisons, equality and booleans in the expression grammar.
- `$today` resolves once per run at a fixed evaluation date, so a given commit
  renders identically on any day (see ADR-010). Schema rules compare against it
  too.
- `workdown install-hooks` installs a pre-commit hook that keeps rendered views
  in sync with the items.
- Resource references are validated, and the web UI renders pickers for fields
  bound to a resource. Where-clause operands are checked against the field's
  value set.
- `id` is projected into every item's field map, so views and filters can use it
  like any other field.

## 0.2.0-alpha.2 - 2026-07-28

### Added

- Computed fields: a `compute:` expression derives a field from the same item's
  other fields, type-checked when the schema loads. Combined with `aggregate`,
  compute fills the leaves and the rollup fills the ancestors.
- Project constants — a reserved `constants` section in `resources.yaml` holding
  named typed scalars (a daily rate, work hours per day), referenced from
  expressions as `$constants.<name>`.

## 0.2.0-alpha.1 - 2026-07-27

### Added

- Display roles: a closed vocabulary (`title`, `subtitle`, `fields`, `color`)
  controls what item-presenting views show, resolved per role from the view,
  the project defaults in `config.yaml`, or a per-kind fallback (see ADR-008).
- A `color` field type, so an item can carry its own color and tint the cards
  and bars that represent it.

## 0.1.0-alpha.3 - 2026-07-14

### Added

- Create views from the web UI and save them to `views.yaml`, including a
  reusable filter builder for where-clauses.
- The schema endpoint serves resource option lists, so editors can offer the
  valid values for a field bound to a resource.

## 0.1.0-alpha.2 - 2026-06-11

### Added

- `workdown serve` — a local web app rendering every view kind, with an
  interactive editor: type-aware field editing, a create form driven by the
  schema, drag-and-drop on the board, and a detail panel reachable from cards,
  table rows, tree titles and gantt labels.
- Live updates: files changed on disk by an editor, the CLI, another tab or
  `git pull` push straight to every connected browser.
- Mutation commands: `set`, `unset`, `move`, `body`, and `rename` (which moves
  the file and rewrites every incoming reference).

## 0.1.0-alpha.1 - 2026-05-13

First installable prerelease.

### Added

- The CLI: `init`, `validate`, `add`, `query`, `render`, and `templates`.
- Work items as Markdown files — YAML frontmatter for structure, freeform body
  for everything else, one item per file, the repo as the single source of
  truth.
- A user-defined schema of fields and types driving CLI behaviour, with
  validation, generated defaults, aggregated fields, and generic link relations
  with cycle detection.
- Persisted views in `views.yaml`, rendered to Markdown by `workdown render`:
  board, tree, graph, table, gantt, bar and line charts, workload, metric,
  treemap and heatmap.
- Prebuilt binaries and shell/PowerShell installers for macOS, Linux and
  Windows.
