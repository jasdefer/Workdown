---
id: effort-field-config
status: to_do
title: Project-level effort field in config.yaml
parent: time-tracking
---

## In plain words

One project-wide answer to the question "which field carries effort",
so that surfaces which record or read effort have a field to point at
without anyone naming it in two places.

Nothing in workdown is called `effort` — the tool has a `duration`
type, and a project decides what it names its fields. That is the right
default, but it leaves anything effort-shaped with no way to find its
field. Views that need it ask per view; a timer that records effort has
nowhere to ask at all. **Example:** a project sets
`defaults.effort_field: effort` once, and every surface that records
measured work knows where to put it.

## Why this belongs in workdown

The tool already answers exactly this question three times over —
`board_field`, `tree_field`, `graph_field` — for the same reason: a
generic type system means the project has to say which field plays
which role. Effort is one more role, and it is the missing piece for
recording work time at all.

## Decisions taken

1. **It goes in `config.yaml`, as `defaults.effort_field`,** next to
   its three siblings. `schema.yaml` says what data is and when it is
   valid; `config.yaml` says what surfaces do with it.
2. **Not a flag on the field in `schema.yaml`.** A flag there would
   make the schema the place where interface behaviour is declared, and
   the next feature would add its own — field definitions would drift
   into a collection of surface hints.
3. **Validated like the other field roles:** the named field must
   exist and must be of type `duration`.
4. **Unset is a normal state.** When no effort field is configured, the
   surfaces that need one are simply absent — the same way a board is
   absent from a schema with no `choice` field.
5. **One field, not a list.** The concrete reason to have several
   effort fields is splitting effort by role or person, which is
   deliberately out of scope; and a list turns "start recording" into
   "start recording, as what?" on the common path. Accepting a list
   later is a backward-compatible widening; narrowing would not be.
6. **The shipped default schema does not gain an effort field.** New
   projects opt in by adding a duration field and naming it here; the
   default config carries the key as a documented example.

## Not in scope

- Changing how the workload view takes its own per-view effort slot.
- Splitting effort by person, role or activity.
