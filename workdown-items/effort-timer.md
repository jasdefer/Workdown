---
id: effort-timer
status: in_progress
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
   Where no effort field is configured, there is no timer — but the
   place the timer would be says so, naming the config key instead of
   showing nothing. That is the whole discoverability answer for a key
   that appears in no shipped default and no editor autocomplete:
   [[config-field-role-validation]] considered putting a hint in
   `workdown validate` and rejected it, because a project that uses
   durations for calendar planning and wants no timer would carry the
   warning forever. Someone looking for a timer looks where the timer
   would be.
3. **Start, pause, stop.** Start and pause never write anything; stop
   writes the accumulated time and returns the timer to zero. A timer
   that is never stopped never changes a file — the Friday evening you
   forget about costs nothing until you deal with it on Monday.
4. **A stop says what it did, and offers to take it back.** A toast
   reports the amount added and the value before and after, and stays
   until dismissed or until the next timer action — an undo that
   expires after a few seconds is hostile to exactly the case it
   exists for. *Undo* reverts the write; *Adjust* turns the added
   amount into an editable duration pre-filled with what was written,
   and confirming makes the effort the before-value plus the corrected
   amount — which is also how an over-long forgotten session gets
   fixed. Adjusting to zero is undo. Both are ordinary writes to the
   effort field. The toast lives at the application level, not inside
   the item view, because a stop can happen from any page.
5. **Rounded to the minute** when the accumulated time is written.
   Anything under half a minute writes nothing at all.
6. **One timer at a time, and no takeover.** While a timer runs, no
   other item offers a start button; its timer slot names the item
   being timed and offers to stop the running timer or to open it
   instead. Switching items is stop first, then start — one click more
   than a takeover, but every write is the result of an explicit stop,
   and no click on the wrong item can silently end a session.
   (Revised: this originally allowed one-click takeover.)
7. **A running timer lives in the running app, not in the repo.** It
   survives page reloads and is the same timer in every open tab; it is
   lost when the server is stopped. Nothing about a timer in flight is
   ever written to a file.
8. **Reachable from anywhere in the app** as a pill in the header that
   appears when a timer starts: the elapsed time, nothing else, plus
   an affordance to expand — and a visibly distinct paused state,
   because a frozen number is indistinguishable from a ticking one at
   a glance. Expanding opens the full controls: the item's title
   linking to it, the wall-clock start time, the elapsed time, pause
   and stop, and the projected write — "effort: 2h → 2h 42min on
   stop" — naming the field it writes to and moving once a minute as
   the rounding dictates; under half a minute it says instead that
   stop writes nothing. While paused it also says since when. Nothing
   more: no roll-up reminder (the confirmation at start was the
   decision point, see 9) and no further item fields (the title link
   is the door to those — this is a timer, not a second item panel).
9. **Timing an item whose effort rolls up from its children is
   allowed** and treated as what it is — a hand-written value that
   overrides the roll-up. The timer says so before it starts rather
   than letting the warning arrive after the write: on such an item,
   start opens a small confirmation naming the override; on a leaf
   item, start just starts and no popup ever appears.
10. **No relation to status.** The tool cannot know which of a
    project's own status values means "actively working", and guessing
    would take a config mapping that buys nothing.
11. **No command-line timer.** A stopwatch you cannot see is a
    stopwatch you forget, and the terminal has nowhere to show one
    ticking. Recording time from a terminal stays what it already is: a
    delta on the effort field.
12. **The app has to be told which field it is.** The server hands the
    UI the schema, the items and the views and nothing else — today
    `defaults.display` is resolved server-side and only its output
    reaches the app, so no part of `config.yaml` is readable from the
    front end. Exposing `defaults.effort_field` is part of this item,
    not of [[effort-field-config]].

13. **No field picker.** A control on the timer letting a session
    write to a different duration field was considered — deviating
    from a default is a normal thing to want. Rejected: that is how
    someone would split effort by activity, which this item excludes,
    and it puts calendar duration one click away from receiving
    measured work — the same wrong write [[effort-field-config]]
    decision 3 refuses to make by inference, only user-triggered. A
    one-off session that belongs elsewhere is fixed afterwards with a
    delta on the right field.
14. **The timer starts from the item, in a fixed slot of the item
    editing surface** — the one component the slide-over panel and the
    standalone item page already share, so both get it for free. The
    slot sits in the same place on every item regardless of schema,
    which makes it the natural "place the timer would be" that
    decision 2 needs: with no effort field configured, this slot is
    where the hint naming `defaults.effort_field` appears. No separate
    timer window or pane — a timer with three buttons does not carry
    enough content to justify its own surface, and the item view
    already provides the context.
15. **The start control is a split button with a mode slot.**
    Stopwatch is the only mode this item implements;
    [[pomodoro-timer]] wires the second mode into the same control.
    The pomodoro option may sit visibly disabled during development
    because the whole milestone lands as one pull request — nothing
    dead reaches a release.

## Not in scope

- Billing, invoicing or rates of any kind.
- Approval workflows over recorded time.
- Anything that automatically infers effort from activity the user did
  not deliberately start.
- Effort per person, and any record of individual sessions.
- Working in intervals ([[pomodoro-timer]]) and being told when an
  interval is over ([[timer-notifications]]).
