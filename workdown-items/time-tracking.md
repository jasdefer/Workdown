---
id: time-tracking
status: in_progress
title: Time tracking
---

## In plain words

Make the difference between how long something took on the calendar and
how much actual work went into it a first-class part of the tool.

"This took three weeks but only three hours of real work" is a normal
thing to want to record, and the gap between the two numbers is a
well-known measure of how smoothly work is flowing. Workdown can
already express the calendar side; the work side is missing, and noting
when something actually started or finished is entirely manual.
**Example:** with both halves recorded, the tool could show that an
item spent twelve of its fourteen days simply waiting — a strong hint
that the delay was not the work itself. Breaking this area down further
waits until the first children have landed.

Make the difference between calendar time and work time first-class —
"this took three weeks but only three hours of actual work." PM literature
distinguishes duration (calendar) from effort (work); Lean / Kanban
surfaces the gap as flow efficiency. Workdown today has primitives for the
calendar side (`duration`, gantt modes) but not for the work side, and
tracking when something actually started or finished is fully manual.

Phase 04 explicitly parked this theme; this milestone picks it up.

## Themes

- Express effort separately from calendar duration, with a way to keep
  the two in sane relation to each other.
- Capture actual start / completion timestamps without manual upkeep —
  the repo already records the truth via git history.
- Surface derived measurements (lead time, cycle time, flow efficiency)
  once both halves exist.

Decomposition beyond the seeded children is deferred until those land.
