---
id: display-defaults-validation
type: issue
status: in_progress
title: Validate `defaults.display` in config.yaml against the schema
parent: view-presentation
depends_on: [view-display-config]
---

`views_check` validates the display roles a view sets in `views.yaml`,
but the project-wide role defaults in `config.yaml` (`defaults.display`)
are validated nowhere. A typo'd field name there is silently skipped at
render time — `effective_fields` filters unresolvable names defensively
so the extractor cannot panic, and unknown `title`/`subtitle` fields
quietly fall back — but the user gets no signal that their config
default is dead.

## What we want

- A config-scoped diagnostic (severity: error, consistent with view
  role references) when a `defaults.display` role names a field that
  resolves neither in `schema.yaml` nor to the virtual `id`.
- Surfaced through `workdown validate` and the serve diagnostics
  banner like every other config diagnostic.

## Why it isn't trivial

`ConfigDiagnostic` carries a `source_path`, but `load_project` receives
the parsed `Config` without knowing which file it came from — the
config path has to be threaded in (or the check has to live where the
config is loaded, in the CLI/server entry points).

## Out of scope

- Validating the *type* of the text role fields (`title`, `subtitle`,
  `fields`) — those are existence-only by design (any value renders
  as text). The `color` role is the exception since
  [[color-display-slot]] landed: `defaults.display.color` must name a
  `color`-typed field (or be the `none` sentinel, which is always
  valid), mirroring what `views_check` enforces for the per-view role.
  That type check is **in** scope here.
