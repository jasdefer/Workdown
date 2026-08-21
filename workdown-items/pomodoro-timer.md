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
   exactly as in the stopwatch. Nothing anywhere reacts to zero —
   phases change only on user action, so the server still never ticks.
3. **A break follows a stop, and the write lands as the break
   begins.** Stopping a work interval writes the effort and starts the
   break in one action: the stop toast reports the write while the
   break pill is already counting down. Exception: a session under
   half a minute writes nothing and starts no break — a stop that did
   nothing leaves nothing behind. Assumed misclick; the toast still
   says nothing was written, as in the stopwatch.
4. **Break time is never recorded.** It goes nowhere and belongs to no
   item.
5. **A break ends one of two ways.** *End* returns the timer to idle
   and writes nothing — no toast either, because nothing was written
   and nothing needs taking back. Or a new work interval starts: the
   break panel offers the previous item as the default, but any item's
   start button works. Starting early is how a break is skipped — no
   separate skip control. Without an exit the Friday-evening stop
   would leave a break counting into negative forever; End is the one
   extra click on the way out the door. (Revised: this originally
   said a new session begins "on the same item" — the same item is
   the default, not a constraint; finishing an item inside one
   interval and switching for the next is the normal case.)
6. **During a break, every item's slot shows the ordinary start
   button**, exactly as when no timer runs; starting anywhere ends the
   break and begins a work interval there. Safe because a break
   records nothing — there is no session a start could destroy, so
   the no-takeover rule has nothing to protect.
7. **Twenty-five and five minutes, fixed.** Not configurable anywhere
   for now: interval lengths are a personal working habit rather than a
   project-wide policy, so they do not belong in the project's
   configuration, and putting them in the interface means deciding
   where a personal preference is kept — a decision worth making on its
   own, later.
8. **No long break after four sessions.** It needs counting across
   sessions and a third length; the two-phase loop carries the value.
9. **The pill shows the countdown; the symbol says whether anything
   is being recorded.** The work phase keeps the filled recording dot
   and counts down (`18:42`), going negative in overrun (`−7:32`).
   The break shows a hollow ring — the recording dot's negation, same
   size, same slot, in a calm color that is never red — plus the word
   "Break", which carries the meaning the ring alone is too small to:
   `○ Break 4:12`. In overrun the clock text turns amber in both
   phases — the whole glanceability story until
   [[timer-notifications]]. The countdown carries hours without
   wrapping, like the stopwatch: a forgotten work interval reads
   `−64:47:10`. The break exists only in the header — no item is
   being timed, so the in-view recording markers disappear.
10. **The expanded panel, per phase.** Work: the item link and start
    time as in the stopwatch, the remaining time as the big number,
    and below it the measured elapsed time with the projected write —
    the projection stays visible because measured time is what gets
    recorded, not the twenty-five. The stop button says what it does
    in this mode: "Stop → break". Break: a heading saying so, the
    countdown, and the two actions of decision 5 — "next interval"
    naming and linking the previous item, and End.
11. **The mode is sticky and lives in server memory.** The split
    button starts in the last-used mode; stopwatch until a pomodoro
    session has ever been started. Server memory for the same reason
    the timer itself is the server's — every tab agrees — and not in
    config.yaml for the same reason as the interval lengths: a
    personal habit is not project policy.
12. **The roll-up confirmation carries through a loop.** The first
    start on a qualifying item confirms as in the stopwatch; starting
    the next interval on the same item while its break runs does not
    ask again. Switching to a different qualifying item re-confirms.
13. (Deleted: pause. An earlier version inherited the stopwatch's
    pause; the stopwatch itself dropped pause before shipping, so
    there is nothing to inherit. An interruption in pomodoro mode is
    a stop — the break it starts is cheap to leave, see 5.)

## Implementation decisions

1. **A phase on the existing session, not a second machine.** The
   server's one timer gains a mode and a phase: work (item, started
   at) or break (started at, the item it followed). Same single lock,
   same snapshot-on-demand — no timed transitions exist, so nothing
   ever ticks server-side (decision 2).
2. **The interval length travels on the wire.** `TimerState` carries
   the mode, the phase, and the running phase's length in seconds;
   the UI computes remaining time from the same elapsed anchor it
   already ticks. The 25/5 constants live once, in the core crate
   beside the rounding rule — the browser never hardcodes them.
   `StartTimer` gains the mode.
3. **A dedicated endpoint ends a break:** `POST /api/timer/break/end`.
   Stop's contract stays "take the timer and write effort" and is
   refused during a break; break-end is a state-only transition and is
   refused otherwise. Overloading stop would make every field of
   `TimerStopResult` optional-and-meaningless in the break case and
   force the toast to branch on it.
4. **Start during a break is one transition** — end the break, start
   the work interval — under the lock, through the existing start
   endpoint.
5. **The confirmation carry of decision 12 is server-side:** while a
   break runs, start on the item the break followed is auto-confirmed;
   any other item goes through the normal needs-confirmation round
   trip.
6. **The misclick rule of decision 3 is the server's,** sitting
   beside the rounding rule it derives from: a stop whose rounded
   write is zero transitions to idle, not to break.
7. **A failed stop write keeps the work interval running** (inherited
   from the stopwatch); the break starts only when the write
   succeeded.
8. **Sticky mode is one field in server memory,** set by every start,
   returned on `GET /api/timer`. The split button's menu selection
   before a start is local UI state; the server remembers what was
   actually started.
9. **No new plumbing.** Every transition is a user action on an
   endpoint, so the existing timer-named live-update ping already
   covers all of it.
10. **UI mechanics:** the countdown formatter joins `timerMath` (the
    clock format, signed); pill, panel and item slot branch on the
    phase; the split button's pomodoro entry loses its `disabled`.
11. **The timer's status report has three shapes — idle, work,
    break — each carrying only the fields that exist in that phase.**
    Not one "running" block with sometimes-empty fields: a break times
    no item and writes nothing, and sometimes-meaningless fields force
    every reader to guess (the argument of decision 3, applied to the
    state shape).
12. **The mode is stated twice on the wire.** A top-level last-used
    mode, always present even when idle (the split button's default),
    and the work phase's own mode (countdown vs. stopwatch face).
    They are always equal by construction — every start sets both —
    but neither reader has to rely on that hidden rule.
13. **A start request always names the mode.** No default: the only
    client is our own UI, so there is nothing to stay compatible
    with, and an explicit request reads unambiguously in tests.
14. **Cosmetics:** overrun uses the typographic minus (U+2212, as in
    `−7:32`), and overrun amber reuses the existing warning color
    token rather than introducing a new one.

## Implementation plan

Three slices, each compiling and testing green on its own; the
existing live-update plumbing needs nothing.

1. **Core:** mode, phase and interval length on the wire shapes; the
   25/5 constants beside the rounding rule; regenerated TypeScript
   types.
2. **Server:** the phase on the state machine and the break-end
   endpoint, tested against the fake clock — stop transitions to
   break and writes, sub-half-minute stop goes idle without a break,
   break-end refusals in both directions, start during a break ends
   it, auto-confirm on the followed item, sticky mode across
   sessions.
3. **UI:** countdown math with boundary tests, the per-phase pill and
   panel, the hollow-ring break treatment, the amber overrun state,
   the enabled pomodoro entry in the split button. The full CI
   checklist runs in the dev container before anything is called
   green.

## Not in scope

- Being told an interval has ended ([[timer-notifications]]).
- Adjustable interval lengths, and anywhere to keep them.
- Counting completed sessions, streaks or any other statistic over how
  the intervals went.
- A long break after N sessions, and any counting toward one.
