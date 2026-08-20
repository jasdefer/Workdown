---
status: in_progress
title: Shared confirmation dialog component
parent: time-tracking
---

## In plain words

A small dialog that asks the user one question and takes confirm or
cancel — the app has no such component today (the slide-over panel
presents an item; it does not ask a question). **Example:** starting a
timer on an item whose effort rolls up from its children opens "Timing
this item overrides its roll-up — start anyway?" with *Start* and
*Cancel*.

## Why a component of its own

The first consumer is the effort timer's roll-up confirmation
([[effort-timer]], UX decision 9), but "ask one question before
acting" is not a timer-shaped need — the next feature that has to
confirm something should reuse this dialog, not build a second one.
Splitting it out keeps the timer's change focused and gives the
component a home among the shared UI pieces.

## Decisions taken

1. **An anchored popover, not a centered modal.** Both are industry
   standards; the published guidance splits them by stakes — the
   full-screen interruption for rare, high-stakes decisions, the
   popover at the triggering control for lightweight contextual ones.
   Every confirmation this app has or plans is a one-sentence question
   triggered by a button click (so an anchor always exists), and even
   the destructive one — deleting a view — removes an entry from a
   YAML file in a git repo. The popover keeps the eye where the click
   happened. The page behind it is inert while it is open: a stray
   click outside cancels the dialog and never also lands on whatever
   it happened to hit. The backdrop is invisible — no dimming.
2. **Focus lands on Cancel.** "Nothing is ever confirmed by accident"
   decides this: the reflexive second Enter from the keypress that
   opened the dialog must not confirm it unseen. Enter and Space
   activate the focused button, nothing more — no global confirm
   shortcut. Tab cycles inside the dialog; on close, focus returns to
   the element that opened it.
3. **Cancel left, confirm right.** The confirm button is the filled
   primary, cancel the quiet one. The caller supplies the confirm
   label; the cancel label defaults to "Cancel" and is overridable.
4. **One `destructive` flag** turns the confirm button red. Two visual
   states, nothing more — no severity ladder, no icons.
5. **The view-deletion confirm migrates here.** The app's only
   existing confirmation is the browser-native `confirm()` on view
   delete — unstyled and inconsistent with everything else. Migrating
   it in this item proves the component is generic rather than
   timer-shaped and gives it two genuinely different consumers on day
   one.
6. **Confirm closes the dialog immediately.** What happens next —
   including a failing server call — is the caller's story, surfaced
   wherever that caller surfaces errors. No pending state, no error
   rendering inside the dialog.
7. **Plain text only.** Title and body are strings; no markup, no
   slots. The moment the body takes arbitrary content this is a
   generic modal shell, which is a different component. Extended only
   when a consumer actually needs more.

## Implementation decisions

1. **A native `<dialog>` opened with `showModal()`, positioned at the
   anchor** instead of the centered default, its `::backdrop` styled
   fully transparent. That buys top-layer rendering (immune to z-index
   fights and to `position: fixed` inside transformed ancestors — the
   slide-over panel is a stacking context the timer will open this
   from), an inert page behind it, Escape-cancels, focus containment,
   and focus restored to the trigger on close — all natively.
2. **Positioning is a hand-rolled pure function**, no dependency:
   below the anchor, flipped above when the viewport runs out, clamped
   to the viewport edges as a last resort. Lives beside the component
   and is unit-tested on its boundaries.
3. **Caller-mounted, props and callbacks — no global service.**
   `anchor`, `title`, `body`, `confirmLabel`, `cancelLabel` (defaults
   to "Cancel"), `destructive` (defaults to false), `onconfirm`,
   `oncancel`. The caller renders it conditionally and drops it on
   either callback. A promise-returning singleton was rejected as
   machinery the two consumers do not need.
4. **`ui/src/lib/ui/ConfirmDialog.svelte`.** The ARIA role is dialog
   regardless of presentation, and the name should not encode a
   presentation choice that could be revisited.
5. **The ViewToolbar migration keeps everything downstream:** the
   dialog closes on confirm (decision 6), so the existing in-flight
   state and error line keep doing their jobs.
6. **Tested where the infrastructure reaches:** the positioning
   function gets unit tests (fits below, flips above, clamps at the
   edges); the component itself is verified by hand in the running
   app — the UI test setup covers pure modules only, and jsdom's
   `<dialog>` support is unreliable. Everything rides the existing
   gates.

## Scope

- A title, one sentence of body text, a confirm and a cancel action
  with caller-supplied labels.
- Escape and clicking outside cancel; nothing is ever confirmed by
  accident.
- Lives with the other shared UI components.
- Replaces the browser-native `confirm()` on view deletion.

## Not in scope

- Multi-step flows, forms, or input fields inside the dialog.
- Any timer-specific behavior — the timer is a caller like any other.
