---
id: same-origin-guard-everywhere
status: to_do
title: Apply the same-origin check to every mutating endpoint, not just the git ones
tags:
- security
---

## In plain words

While `workdown serve` runs, any other website open in the same browser
can quietly trigger one action in your project: stopping your running
timer. It cannot read anything back, and it cannot change an item — but
the stop goes through, and the measured time gets written into the
file.

The fix already exists in the codebase. It was written for the new git
buttons and simply was not applied to the older endpoints.

## Why it happens

Before a website may send an unusual request to a different address,
the browser asks that address for permission first, and refuses to send
anything if no permission comes back. workdown never grants permission,
so those requests never leave the browser.

But the browser skips that question for *ordinary* requests — and a
POST that carries no data at all counts as ordinary. It just sends it.

That splits the API in two:

| Endpoint | Sends data? | Reachable from a foreign site? |
|---|---|---|
| create item, set field, start timer | yes, JSON | no — browser asks first, gets refused |
| view create / update / delete | yes | no — same |
| `POST /api/timer/stop` | **no** | **yes** |
| `POST /api/timer/break/end` | **no** | **yes** |

Both stop-shaped calls need no data — there is only ever one running
timer, so "stop" carries all the information required. Nothing on the
server side looks at where the request came from, so it runs: the
elapsed time is added to the item's effort field, the timer is cleared,
and every open tab refreshes showing it stopped.

## How bad it is

Mild, and worth being precise about rather than alarmed by.

- The server listens on `127.0.0.1` only, so nothing is reachable over
  the network.
- The attacking page cannot read the response — the browser still
  blocks that. It learns nothing, not even whether a server was there.
- It does not know the port either, though guessing is cheap: the
  default is 3141 and workdown scans up to ten ports forward on
  conflict, so ten blind attempts cover the normal range.
- Projects with no `defaults.effort_field` have no timer at all and are
  unaffected — including workdown's own repo.

So: a blind nuisance write against projects that use the timer, not a
data leak. Shipped in 0.2.5.

## The fix

`crates/server/src/api/git.rs` already contains the check — it reads
the header the browser attaches to every request naming the originating
site, and refuses anything that is not localhost. It is applied to the
three git endpoints and nowhere else.

Two ways to spread it, to be picked when the work is done: move the
helper somewhere shared and call it from the timer and item handlers,
or wrap every mutating route in one layer so a future endpoint is
covered by default. The second is the reason to prefer it — this gap
appeared because a guard had to be remembered per handler.

Non-browser clients (curl, scripts) send no such header and must keep
working.
