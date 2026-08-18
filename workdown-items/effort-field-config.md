---
id: effort-field-config
status: to_do
title: Project-level effort field in config.yaml
parent: time-tracking
depends_on:
- config-field-role-validation
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
   valid; `config.yaml` says what surfaces do with it. The `defaults.`
   prefix means what it already means for the other three: not a
   fallback that views inherit — a board view names its own field, and
   `board_field` is read only by `workdown move` — but the project's
   answer for surfaces that have nowhere else to ask. A timer is such a
   surface, so the workload view keeping its own per-view `effort:`
   slot is not a duplication.
2. **Not a flag on the field in `schema.yaml`.** A flag there would
   make the schema the place where interface behaviour is declared, and
   the next feature would add its own — field definitions would drift
   into a collection of surface hints.
3. **The field must be named explicitly; the tool never infers it.**
   Not even for a project with exactly one duration field. The reason
   is what that one field almost certainly is: workdown already covers
   the calendar half of [[time-tracking]] and not the work half, so a
   duration field that exists today is a *calendar* duration — and
   inference would aim a stopwatch at it in precisely the case where it
   fires. The comparable inference the tool does make, the `color`
   display role falling back to the first color field in schema order,
   survives being wrong because being wrong tints a card the wrong hue,
   visibly and reversibly; being wrong here adds measured work to a
   field meaning "this item spans three weeks" and gets committed.
   Inference is also unstable: a project whose timer works because
   there is one duration field would change behaviour the day someone
   adds an unrelated one, and behaviour that shifts with a field nobody
   was thinking about cannot be explained in a message.
4. **The key itself is optional, and unset is a normal state.** When no
   effort field is configured, the surfaces that need one are simply
   absent — the same way a board is absent from a schema with no
   `choice` field. Optional rather than mandatory like its three
   siblings: a missing mandatory key fails the whole config at load, so
   making this one mandatory would break every project that predates
   it.
5. **Duration fields only.** The key names where measured work lands,
   and a stopwatch produces time. The workload view's own effort slot
   keeps accepting integer and float as well — that one reads
   estimates, which is a different thing from a measurement. So the
   restriction is a consequence of what the key is for, not a
   narrowing of what a project may call effort. Accepting more types
   later is a backward-compatible widening; the reverse would not be.
6. **One field, not a list.** The concrete reason to have several
   effort fields is splitting effort by role or person, which is
   deliberately out of scope; and a list turns "start recording" into
   "start recording, as what?" on the common path. Accepting a list
   later is a backward-compatible widening; narrowing would not be.
7. **Validated eagerly, and non-blocking.** A named field that does not
   exist, or is not a duration, is reported by `workdown validate` and
   the serve banner, pinned to `config.yaml`. Its three siblings are
   validated only at use time, and for them that suffices: `workdown
   move` is a command, so the error arrives the moment you run it. Here
   it would not, because the only consumer is a timer in the web app
   and "no effort field" is a legitimate state — a typo and a
   deliberate blank would be indistinguishable, both showing no timer
   and saying nothing. No field-role key is validated at load today, so
   this waits on [[config-field-role-validation]].
8. **Validation checks two things: the field exists, and it is a
   duration.** Whether that field rolls up from its children, is
   computed, or is pulled from a linked item is not this check's
   business — writing to such a field is already allowed and already
   warns ([[duration-delta-absent-value]], [[effort-timer]]).
9. **`none` is not special here.** It is a reserved field name that
   display roles use as their "no field" sentinel, but unset already
   carries that meaning for effort, so `effort_field: none` is an
   undefined field and errors like any other typo. Two spellings of one
   state would only be something to learn for nothing.
10. **The shipped defaults gain nothing** — no effort field in the
    default schema, no key in the default config, not even as a
    comment. New projects add a duration field and name it. This
    matches the three siblings, which are documented nowhere either;
    documentation for the key belongs wherever the timer is documented.
    `workdown init` is unaffected.
11. **No ADR.** This is a fourth instance of the pattern ADR-008 and
    the three existing keys already establish, not a new architectural
    decision.

## Open questions

- A command-line surface for effort. `effort_field` would be the first
  field role no command reads: `board_field` powers `workdown move`,
  and the tree and graph keys drive rendered views. A one-shot delta —
  `workdown log <id> 30min` against the configured field — is not the
  command-line stopwatch [[effort-timer]] rules out, and
  [[duration-delta-absent-value]] already built the behaviour it would
  need. Undecided whether it is worth having; it would also give this
  item something observable to verify against, which it otherwise
  lacks until the timer exists.

## Not in scope

- Changing how the workload view takes its own per-view effort slot.
- Splitting effort by person, role or activity.
- Reaching the key from the web app. The server hands the UI schema,
  items and views and nothing else, so exposing it is part of
  [[effort-timer]].
