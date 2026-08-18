---
id: pomodoro-timer
status: to_do
title: Pomodoro mode for the effort timer
parent: time-tracking
depends_on:
- effort-timer
---

## In plain words

A second way to run the timer: instead of an open-ended stopwatch, a
counted-down work interval followed by a short break, repeated for as
long as you keep going.

Working in fixed intervals is a widespread habit, and people who work
that way already run a second app next to their tools to do it. Since
[[effort-timer]] is already measuring the work, the interval is a
different face on the same measurement — and it makes starting easier,
because "twenty-five minutes on this" is a smaller commitment than an
open-ended session. **Example:** you start a session on an item, work
until the countdown reaches zero, keep going for another seven minutes
to finish the thought, press stop, and `32min` lands on the item.

## Why this belongs in workdown

The measurement, the item it belongs to and the field it lands in are
all already there. What is missing is the pacing — and pacing is
exactly the part that gets people to start the timer at all, which is
the whole problem [[effort-timer]] is trying to solve.

## Decisions taken

1. **What is recorded is always the measured time the work interval
   ran.** The countdown paces the work, it never decides the number: a
   session stopped at thirty-two minutes records thirty-two minutes,
   not the twenty-five it was aiming for.
2. **Reaching zero stops nothing.** The interval keeps running and
   shows the overrun as a negative remaining time. Only stop writes,
   exactly as in the stopwatch.
3. **A break follows a stop.** Stopping a work interval writes the
   effort and starts the break; the break can overrun into negative
   time in the same way.
4. **Break time is never recorded.** It goes nowhere and belongs to no
   item.
5. **Starting a new session** begins a fresh work interval on the same
   item.
6. **Twenty-five and five minutes, fixed.** Not configurable anywhere
   for now: interval lengths are a personal working habit rather than a
   project-wide policy, so they do not belong in the project's
   configuration, and putting them in the interface means deciding
   where a personal preference is kept — a decision worth making on its
   own, later.
7. **No long break after four sessions.** It needs counting across
   sessions and a third length; the two-phase loop carries the value.
8. **Pause works the same in both modes**, freezing the countdown and
   writing nothing.

## Not in scope

- Being told an interval has ended ([[timer-notifications]]).
- Adjustable interval lengths, and anywhere to keep them.
- Counting completed sessions, streaks or any other statistic over how
  the intervals went.
