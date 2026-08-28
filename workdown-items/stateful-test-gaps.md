---
id: stateful-test-gaps
status: done
title: Cover the browser-side paths that fail silently
parent: maintenance-review-2026-08
---

## In plain words

The web app's tests all target pure calculation modules; the parts that
hold live state have none. Most of that is fine — a broken toast or a
stuck button announces itself immediately to the only person using the
tool. Two paths do not. **Example:** undo has to put back exactly what
the field held before a stop, including "nothing at all"; get that
branch wrong and it writes a wrong duration into a work item and reports
success, with nothing at all to notice.

The CLI half of this item moved to [[testing-strategy]]. Testing the
built binary is the question that milestone exists to answer, and
answering it here would pre-empt it.

## What is actually at risk

The timer store's dependencies are five outside things: the API client,
the clock (`performance.now`), the once-a-second tick, the announcer
(chime + notification + cross-tab claim), and the deadline Worker.
Sorting its behaviours by whether a regression is *visible*:

**Silent — worth a test:**

- **Undo writing the wrong value.** Undo must restore exactly the prior
  value, including "absent" when the field had none (`op: unset` vs
  `op: replace`). Getting this wrong writes a wrong duration into a
  work item and reports success. Data damage with no symptom.
- **The stale-tab stop (409).** Only fires in a race that daily use
  will not hit, so dogfooding genuinely does not cover it. The path is
  deliberately different from other failures: different advice line,
  plus a resync.

**Loud — not worth a test.** Toast lifecycle, the `busy` flag that
disables the controls, the ticking countdown display, the zero-crossing
chime. These break in front of the only user, immediately. Testing them
is also where all the setup cost sits: a fake clock and a fake announcer
exist only to serve the countdown/announcement tests, and the crossing
*math* they would re-cover already has tests in `announcements.ts`.

## Decisions taken

### Decision 1 — how the timer store becomes testable

Originally settled as: wrap the store in a factory,
`createTimerStore({ api })`, injecting the API client and nothing else.
Rejected at the time: resetting the module registry between tests
(`vi.resetModules()` plus a dynamic import), which needs no production
change but couples tests to import mechanics and leaves state bleeding
between tests by default rather than by accident.

**Revised.** Both silent paths turned out to be pure decisions wrapped
in async plumbing, so they were extracted instead — the same split
`announcements.ts` already makes, and the reason the rest of `ui/` is
testable without machinery at all. That took the factory off the
critical path; whether to do it anyway is Decision 8 below.

### Decision 2 — no DOM environment, no Svelte plugin

`ui/vitest.config.ts` stays on the `node` environment, and its comment
about deferring DOM setup remains accurate. With the decisions
extracted, nothing in scope loads a `.svelte.ts` module, so
`@sveltejs/vite-plugin-svelte` is not needed either — the one piece of
new test machinery the earlier scope would have forced.

## What was done

- **`ui/src/lib/timer/stopOutcome.ts`** — new pure module holding the
  two decisions: `undoMutation` (replace with the previous value
  verbatim, or unset when the field held nothing) and `stopFailure`
  (the 409 family versus a genuinely failed write). The store calls
  both and keeps the plumbing.
- **`ui/src/lib/timer/stopOutcome.test.ts`** — the payload table,
  including the branch a truthiness test would get wrong: `0`, `false`
  and `''` are values the field held and come back as values, not as an
  unset.
- **`ui/src/lib/api/client.test.ts`** — envelope normalization with
  `fetch` stubbed. Needed no refactor. An empty body becomes the empty
  envelope, `data` and `error` are absent rather than `undefined`
  (`exactOptionalPropertyTypes`), a truncated reply becomes an error
  result, and an unreachable server becomes `status: 0` rather than a
  rejection — which would otherwise escape the fire-and-forget call
  sites and strand the timer's `busy` flag, disabling its controls for
  the life of the tab.

The UI suite runs 133 tests, up from 112. No `ci.yml` change was needed:
`cargo xtask build-ui` already runs `npm run test`.

## What this leaves open, and where it went

Extraction covers the two decisions but not the plumbing around them: a
store that called `undoMutation` and then ignored its answer, or read
`nothingToStop` and resynced on the wrong branch, would pass everything
written here. Whether that plumbing is worth driving — and so whether
the factory from Decision 1 gets built after all — is a question about
how this project tests stateful browser modules in general, not about
these two paths. It is handed to [[testing-strategy-design]], which
asks exactly that.

## Out of scope

- **Component (rendered-DOM) tests for Svelte views** — separate
  decision, separate cost.
- **Timer-store toast lifecycle, `busy`-flag, countdown-display and
  zero-crossing tests** — cut deliberately, not overlooked. Every one of
  these fails visibly and immediately for the only user of the tool,
  and together they are what would force a fake clock, a fake announcer
  and a DOM environment into the test setup. Revisit if Workdown gains
  users who are not the author, or if the store starts changing often.
- **Everything about the CLI** — [[testing-strategy]].
