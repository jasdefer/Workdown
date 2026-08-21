---
id: effort-timer
status: done
title: Stopwatch in the web app that records effort
parent: time-tracking
depends_on:
- effort-field-config
- duration-delta-absent-value
- confirm-dialog
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
   would be. When the schema contains no duration field at all, the
   slot shows nothing — a hint naming a config key that no existing
   field could satisfy is not actionable. (Revised: the hint
   originally appeared unconditionally.)
3. **Start and stop — no pause.** Start never writes anything; stop
   writes the elapsed time and returns the timer to zero. An
   interruption is a stop and a later start: the same total effort,
   written in pieces. A timer that is never stopped never changes a
   file — the Friday evening you forget about costs nothing until you
   deal with it on Monday. (Revised: this originally had a pause
   between start and stop. Pause bought only "one session, one write"
   and charged for it with a visibly distinct paused pill state, a
   resume control and a paused-since line — stop-then-start does the
   same job with machinery the feature already has.)
4. **A stop says what it did, and offers to take it back.** A toast
   reports the amount added and the value before and after, and stays
   until dismissed or until the next timer action — an undo that
   expires after a few seconds is hostile to exactly the case it
   exists for. *Undo* reverts the write: the effort returns to exactly
   the before-value, including becoming absent again if it was absent
   before. An ordinary write to the effort field. A stop that writes
   nothing (see 5) still gets a toast saying so — silence after
   pressing stop reads as breakage. The toast lives at the application
   level, not inside the item view, because a stop can happen from any
   page. (Revised: an *Adjust* control was originally here.
   Corrections beyond undo need no special machinery — the effort
   field is editable in the item form like any other field, which is
   also how an over-long forgotten session gets fixed.)
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
   ever written to a file. The timer is the server's, not a browser's:
   every browser connected to the same `workdown serve` sees and
   controls the same timer, and one can stop another's session —
   accepted for a local, single-user tool.
8. **Reachable from anywhere in the app** as a pill in the header that
   appears when a timer starts: the elapsed time, nothing else, plus
   an affordance to expand. Expanding opens the full controls: the
   item's title linking to it, the wall-clock start time, the elapsed
   time, stop, and the projected write — "effort: 2h → 2h 42min on
   stop" — naming the field it writes to and moving once a minute as
   the rounding dictates; under half a minute it says instead that
   stop writes nothing. Nothing more: no roll-up reminder (the
   confirmation at start was the decision point, see 9) and no further
   item fields (the title link is the door to those — this is a timer,
   not a second item panel). (Revised: the paused state, its distinct
   pill look and the paused-since line left with pause itself, see 3.)
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
    already provides the context. The slot has exactly three states:
    no timer running — the start button; this item being timed — it
    says so, and clicking opens the header pill's expanded panel
    rather than showing a second copy of the controls (one component,
    never visible twice); another item being timed — the behavior of
    decision 6.
15. **The start control is a split button with a mode slot.**
    Stopwatch is the only mode this item implements;
    [[pomodoro-timer]] wires the second mode into the same control.
    The pomodoro option may sit visibly disabled during development
    because the whole milestone lands as one pull request — nothing
    dead reaches a release.

## Implementation decisions

1. **One timer in server memory, behind a single lock.** Decision 7
   forces server memory; the lock is what makes two tabs pressing stop
   at the same moment safe — one stop takes the timer and writes, the
   other is told no timer is running. No double write possible.
2. **Timestamps, not counters.** The timer's whole state is *which
   item* and *when started*; elapsed time is current time minus start
   time, computed fresh whenever asked. Wall-clock time, not the
   machine's uptime counter — the uptime counter freezes while a
   laptop sleeps, and the forgotten weekend timer of decision 3 must
   keep counting. Elapsed time is clamped at zero so a backwards clock
   jump cannot go negative.
3. **Three endpoints:** get timer state, start (item plus an optional
   confirmed flag, see 6), stop. Wrong moves get clean refusals —
   start while a timer runs (the no-takeover rule of decision 6,
   enforced server-side too), stop when nothing runs.
4. **Stop writes on the server, as a duration delta** through the same
   code path as `workdown set --delta` — taking the timer and writing
   are one indivisible step. A delta rather than an overwrite: a hand
   edit made during the session survives, and an absent effort field
   starts from zero ([[duration-delta-absent-value]]). The stop
   response carries what the toast needs: field, amount added, value
   before and after, and any warnings the write caused.
5. **The effort field reaches the UI on the timer endpoint**, in one
   of three states: *unconfigured* (the slot shows the hint of
   decision 2, when a duration field exists to point at), *invalid*
   (the key names a field that is missing or not a duration — the
   slot says that instead of pretending the key is absent), *ready*
   (here is the field). No other part of `config.yaml` is exposed.
   Config is read once at server start, so the hint tells the user to
   restart after setting the key.
6. **The roll-up confirmation of decision 9 is decided by the
   server** — the browser cannot see an item's children. Start on a
   qualifying item is refused with "needs confirmation"; the app shows
   the dialog and sends start again with the confirmed flag. An item
   qualifies when the effort field aggregates and the item has at
   least one child over the aggregate's link field — children *with
   values* would make the dialog appear and vanish as children gain
   their first value. The refusal travels as a typed outcome, not an
   error: start's successful reply is one of two shapes, *started*
   (with the new timer state) or *needs confirmation*. The reply
   envelope keeps its two kinds, success and failure — a third
   envelope-level kind for one endpoint's dialog flow would burden
   every endpoint's contract, and the browser must never parse error
   text to detect a normal fork in the flow.
7. **Other tabs learn of timer changes via a second, timer-named
   message** on the live-update stream each tab already holds. The
   message carries no data; the tab refetches the timer state — the
   same ping-then-refetch pattern everything else uses. Not the
   generic file-change ping: that would refetch the timer on every
   file save and reload the whole page on every timer action.
8. **Rounding lives on the server** (nearest minute, thirty seconds
   rounds up, zero minutes means no write) and is mirrored as the same
   one-line rule in the browser for the projected write of decision 8,
   so projection and write can never disagree. Both sides are tested
   on the boundaries: 29s → nothing, 30s → 1min, 90s → 2min.
9. **Undo is a plain field write through the existing edit endpoint:**
   set the before-value back, or unset when the field was absent
   before. No server-side undo memory.
10. **A failed stop write keeps the timer running** and puts the error
    in the toast — stop again after fixing the cause, or start another
    timer to abandon the session deliberately. A transient failure
    must not discard measured time.
11. **Code placement follows the house pattern** (as field editing
    does): the message shapes both sides must agree on, and the
    rounding rule, live in the core crate — where TypeScript type
    generation already looks and where the write path's set machinery
    already is. The timer state machine lives in the server crate,
    written against an injected clock so tests never wait. In the UI:
    a timer store beside the schema store (ticking locally between
    refreshes, anchored as "server said X seconds, Y moments ago", so
    a wrong browser clock cannot skew it), the pill in the header's
    action area next to "+ New item", a single-slot toast in the app
    layout (decision 4 says one toast, replaced by the next action —
    a queue would be dead machinery), and the split start button with
    the disabled pomodoro entry of decision 15. The confirmation
    dialog of decision 9 comes from [[confirm-dialog]] — a shared
    component, not a timer-private one.
12. **Two time formats, by role.** The ticking elapsed time renders
    clock-style (`1:23:45`) — a "1h 23min" label does not visibly
    tick — and carries hours past twenty-four without wrapping (the
    forgotten weekend reads `65:12:03`). Everything the write touches
    — the projected write, the before and after values, the toast
    amounts — uses the duration formatting the field already has
    everywhere else in the app.

## Implementation plan

Four slices, each compiling and testing green on its own; nothing
user-visible before the last, which is fine — the milestone ships as
one pull request.

1. **Core:** the wire shapes both sides agree on (timer state, the
   two-shape start outcome, the stop result) and the rounding rule,
   with boundary tests (29s → nothing, 30s → 1min, 90s → 2min).
2. **Server:** the timer state machine, unit-tested against a fake
   clock (transitions, refusals, elapsed math, the backwards-clock
   clamp), and the three endpoints with integration tests like the
   existing ones — start conflict, needs-confirmation round trip,
   stop writes the delta, sub-half-minute stop writes nothing, stop
   on a deleted item keeps the timer running.
3. **Plumbing:** the timer-named event on the live-update stream.
4. **UI:** timer store, item slot, header pill and panel, toast,
   split start button; the dialog lands first via [[confirm-dialog]].
   UI correctness rides the existing gates; before declaring anything
   green, the full CI checklist runs in the dev container.

## Not in scope

- Billing, invoicing or rates of any kind.
- Approval workflows over recorded time.
- Anything that automatically infers effort from activity the user did
  not deliberately start.
- Effort per person, and any record of individual sessions.
- Working in intervals ([[pomodoro-timer]]) and being told when an
  interval is over ([[timer-notifications]]).
