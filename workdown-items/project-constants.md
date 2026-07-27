---
id: project-constants
type: issue
status: to_do
title: Project-level constants in resources.yaml
parent: time-tracking
effort: "4h"
---

Named scalar values defined once per project and referenced from the
schema: a daily rate for cost computation, the work-hours-per-calendar-
day convention that [[duration-comparison-rule]] needs to compare
mixed-unit durations. They live in `resources.yaml` — it is the
user-editable *data* file, and a rate is data that changes (yearly,
per client), unlike `schema.yaml` (structure) or `config.yaml` (CLI
wiring).

```yaml
constants:
  daily_rate:
    type: float
    value: 800
  work_hours_per_day:
    type: duration
    value: "8h"
```

## Scope

- `constants:` section in `resources.yaml`, alongside the entity
  lists. Each constant declares a type from the existing field type
  system and a value; the value is parsed and validated like a field
  value of that type.
- Constants exposed on the loaded model so schema-level consumers
  ([[computed-fields]] expressions via `$constants.<name>`, rule
  configs) can resolve them by name. Unknown-constant references are
  schema-load errors.
- `resources.schema.json`: formal definition of the section.

## Out of scope

- Per-entity values (a `daily_rate` attribute on each person in the
  `people` resource) — natural later extension, same file, no new
  mechanism needed then.
- Referencing constants from frontmatter values.
