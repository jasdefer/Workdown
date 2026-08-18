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
   window, or in a background tab.
3. **An open tab is required.** Nothing is announced when the app is
   closed or the server is stopped, and that limit is stated rather
   than worked around.
4. **A notification never writes anything** and never stops, starts or
   changes a timer. It is an announcement, and every state change stays
   in the user's hands.

## Not in scope

- Notifications about anything other than the timer.
- Reminders, nudges or anything that fires when no timer is running.
- Delivering anything when the app is not open.
