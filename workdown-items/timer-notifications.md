---
id: timer-notifications
status: to_do
title: Tell the user when a timed interval is over
parent: time-tracking
depends_on:
- pomodoro-timer
---

## In plain words

When a work interval or a break reaches zero, the app says so — audibly
and visibly — instead of waiting to be looked at.

An interval you have to watch is worse than no interval at all: the
point of counting down twenty-five minutes is that you can forget the
clock and stay in the work. [[pomodoro-timer]] gives the countdown, but
a countdown nobody is looking at passes unnoticed. **Example:** you are
deep in an editor with the app in a background tab; at zero a sound and
a notification tell you the interval is over, and the tab itself shows
it at a glance.

## Why this belongs in workdown

Every other part of the timer is silent by design — it measures, it
writes, it never interrupts. This is the one moment where interrupting
is the entire feature.

## Decisions taken

1. **Both ends are announced:** the work interval reaching zero, and
   the break reaching zero.
2. **Sound, a system notification, and something visible in the tab
   itself**, so it lands whether the app is in front, behind another
   window, or in a background tab. The three channels are redundancy,
   not decoration: sound fails silently on a muted machine, the tab
   title is invisible behind a fullscreen editor, and the notification
   is the one channel the OS draws over every other app — and the only
   one that persists (in the notification center) if the moment itself
   was missed.
3. **An open tab is required.** Nothing is announced when the app is
   closed or the server is stopped, and that limit is stated rather
   than worked around.
4. **A notification never writes anything** and never stops, starts or
   changes a timer. It is an announcement, and every state change stays
   in the user's hands.
5. **The chime is synthesized, not shipped.** A short Web Audio
   two-tone — descending when the work interval ends (go rest),
   ascending when the break ends (back to it). No binary asset in the
   repo or the embedded dist. Accepted limitation: a tab that has never
   seen a user interaction cannot play sound (browser autoplay policy);
   the tab that started the timer always has one.
6. **Notification permission is asked for on the first pomodoro
   start** — a user gesture, and the moment the permission starts
   buying something. Denying it is the built-in opt-out: sound and tab
   title still work, and nothing ever re-asks. The notification itself
   is marked silent so the OS sound never doubles the chime.
7. **The tab title carries the countdown whenever a pomodoro phase
   runs** (`18:42 · workdown`) and flips to an alarm form at zero
   (`⏰ Break over · workdown`); the plain title returns when the
   phase does. No favicon swap — the title carries the meaning. This
   is the "visible in the tab itself" of decision 2, glanceable before
   zero rather than only at it.
8. **The wording never implies a stop happened** — reaching zero stops
   nothing ([[pomodoro-timer]] decision 2). Work end: "Interval over
   on ‹item› — still recording until you stop." Break end: "Break
   over — start the next interval." Clicking the notification focuses
   the tab and opens the timer panel: a shortcut to where the actions
   are, never an action itself (decision 4).
9. **Each crossing is announced exactly once, and only when observed
   live.** No repeat, no nag — the amber overrun display is the
   persistent reminder. A tab that opens into an already-overrun phase
   shows the state and stays quiet: a chime for something that happened
   twenty minutes ago is noise. (The concrete edge of decision 3.)
10. **No mute and no toggle.** Tab mute and the notification
    permission are the knobs that already exist; a home for personal
    preferences remains its own decision, exactly as with the interval
    lengths.

## Implementation decisions

1. **A pure UI slice.** The server never ticks and phase changes only
   happen on user action, so the zero-crossing is detected by the
   browser's own ticking; `phase_length_seconds` is already on the
   wire for both phases. No core, server or wire change.
2. **A dedicated Web Worker fires the deadline.** Background tabs
   throttle `setInterval` to once a minute after a few minutes hidden —
   precisely the scenario this feature exists for; worker timers are
   exempt. The worker holds one timeout aimed at the running phase's
   zero and posts "check now"; the one-second UI tick is unchanged,
   and the worker's ping also refreshes the tab title so a hidden
   tab's title stays honest.
3. **One announcer per crossing, across tabs.** A crossing is keyed by
   phase identity (phase kind plus its start moment, both on the
   wire). The first tab to claim the key — a Web Lock around a
   localStorage flag — plays the chime and posts the notification;
   every tab updates its own title. Locks release when a tab closes,
   so a vanished leader costs nothing.
4. **The mechanics sit behind a thin announcer adapter.** Crossing
   detection — remaining time crossed zero between two live
   observations of the same phase, key unclaimed — is pure and joins
   the `timerMath`-style tests; audio, Notification and the worker
   live behind an interface tests replace. Nothing asserts on an
   actual chime.

## Implementation plan

One UI slice, no server or core work.

1. **Pure logic:** crossing detection, the once-per-phase-identity
   rule, and the title formatting, in a DOM-free module beside
   `timerMath`, with boundary tests (just before zero, at zero, load
   into overrun, phase change resets).
2. **The announcer adapter:** the worker, the Web Audio chime pair,
   the Notification wiring with click-to-focus-and-open-panel, and
   the permission request on the first pomodoro start.
3. **Store integration:** the timer store observes crossings and
   drives the title; the full CI checklist runs in the dev container
   before anything is called green.

## Not in scope

- Notifications about anything other than the timer.
- Reminders, nudges or anything that fires when no timer is running.
- Delivering anything when the app is not open.
- A favicon change — the tab title carries it.
- A mute or sound preference, and anywhere to keep one.
