---
id: effort-timer
status: to_do
title: Stopwatch in the web app that records effort
parent: time-tracking
depends_on:
- effort-field-config
- duration-delta-absent-value
---

## In plain words

A start/stop stopwatch in the web app that measures how long you
actually work on an item and adds that time to its effort, so effort
gets recorded as it happens instead of reconstructed from memory later.

Recording effort by hand means guessing after the fact, and people
guess badly — or skip it entirely, which is why the work side of
[[time-tracking]] stays empty in practice. A timer removes the
bookkeeping: you press start when you begin, press stop when you stop,
and the item's effort grows by itself. This is the same gesture the
Kanban-style tools offer, and it is the cheapest way to get honest
effort numbers into the repo. **Example:** you open an item in the
morning, hit start, get interrupted after 40 minutes and hit stop —
the item's effort goes from `2h` to `2h 40min` without you doing any
arithmetic.

## Why this belongs in workdown

Effort is the half of [[time-tracking]] that has no capture mechanism.
Calendar dates get written down, and a status change can be stamped as
it happens — but nothing anywhere records how long someone actually sat
with a problem. Only the person doing it knows, and only while they are
doing it.

## Decisions taken

1. **The timer moves a number, it does not keep a diary.** Stopping
   adds the measured time to the item's effort and nothing anywhere
   records that a session happened, who ran it, or when. A record of
   individual sessions is a different and much larger feature.
2. **It writes to the project's effort field** ([[effort-field-config]]).
   Where no effort field is configured, there is no timer.
3. **Start, pause, stop.** Start and pause never write anything; stop
   writes the accumulated time and returns the timer to zero. A timer
   that is never stopped never changes a file — the Friday evening you
   forget about costs nothing until you deal with it on Monday.
4. **A stop says what it did, and offers to take it back.** The app
   shows the amount added and the value before and after, with an undo
   and a way to correct the number — which is also how an over-long
   forgotten session gets fixed.
5. **Rounded to the minute** when the accumulated time is written.
   Anything under half a minute writes nothing at all.
6. **One timer at a time.** Starting on another item stops the running
   one first, with the same write and the same message.
7. **A running timer lives in the running app, not in the repo.** It
   survives page reloads and is the same timer in every open tab; it is
   lost when the server is stopped. Nothing about a timer in flight is
   ever written to a file.
8. **Reachable from anywhere in the app**, showing which item it is
   bound to and the time elapsed while it runs.
9. **Timing an item whose effort rolls up from its children is
   allowed** and treated as what it is — a hand-written value that
   overrides the roll-up. The timer says so before it starts rather
   than letting the warning arrive after the write.
10. **No relation to status.** The tool cannot know which of a
    project's own status values means "actively working", and guessing
    would take a config mapping that buys nothing.
11. **No command-line timer.** A stopwatch you cannot see is a
    stopwatch you forget, and the terminal has nowhere to show one
    ticking. Recording time from a terminal stays what it already is: a
    delta on the effort field.

## Not in scope

- Billing, invoicing or rates of any kind.
- Approval workflows over recorded time.
- Anything that automatically infers effort from activity the user did
  not deliberately start.
- Effort per person, and any record of individual sessions.
- Working in intervals ([[pomodoro-timer]]) and being told when an
  interval is over ([[timer-notifications]]).
