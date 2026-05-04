---
id: time-tracking
type: milestone
status: to_do
title: Time tracking
---

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
