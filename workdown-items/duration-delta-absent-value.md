---
id: duration-delta-absent-value
status: done
title: Duration delta starts from zero on an absent field
parent: time-tracking
---

## In plain words

Adding to a duration field that has no value yet should work, and
should behave as if the field had been zero.

Today `workdown set effort --delta 30min` on an item that has never
had an effort value is refused with "set an initial value first". For a
duration that is a pointless hurdle: an absent duration and `0s` reach
the same answer, and the first time anyone records effort on an item is
exactly the moment the field is absent. **Example:** an item with no
`effort` gets `--delta 30min` and comes out with `effort: 30min`,
instead of an error telling you to write `0s` first and try again.

## Why this belongs in workdown

Every way of recording effort — by hand on the command line, or
measured by a timer — hits the empty field on its very first use. The
refusal makes the common case the awkward one.

## Decisions taken

1. **An absent duration field is treated as `0s`** and created by the
   delta, instead of being refused.
2. **Date fields stay strict.** A date has no zero to count from, so a
   delta on an absent date remains an error.
3. **Integer and float fields are left as they are** for now. The same
   argument could be made for them, but nothing needs it yet, and
   "absent" for a count can be a deliberate statement.
4. The resulting asymmetry between duration and date deltas is
   deliberate and gets written down under `--delta` in `workdown set
   --help`, so it does not read as an oversight. The error message for
   the date case stays as it is — `--delta` does work on dates, it just
   needs a value to move.
5. **A field written with no value counts as absent** and is created at
   zero the same way. So does a field holding an empty string — the same
   accident with a quote added, and an empty string holds no evidence to
   destroy. A field holding anything else that isn't a duration stays an
   error: replacing a typo with a measured number destroys the evidence
   that something was wrong.
   **"Absent" means one thing across every field type**, and only the
   consequence differs. Null and empty string count as absent for
   integer, float, date and boolean too — those still refuse, but now
   with "absent field — set an initial value first" instead of the
   misleading "current value is not a valid number". There is no value
   there to be invalid.
6. **Zero is the only starting point.** An item whose effort rolls up
   from its children, or is computed, or is pulled from a linked item,
   still starts the delta at zero and writes the result as a
   hand-written value. Starting from the derived number would silently
   freeze it into the file, where it would go stale the next time a
   child changed. The write raises the usual warning about a manual
   value competing with a roll-up, so nothing happens quietly.
7. **A field's schema default is not the starting point either.**
   Defaults are stamped in when an item is created; an item that
   predates a default, or had its value cleared, does not reacquire it
   through arithmetic. This turned out to be vacuous for durations: a
   `duration` field cannot declare a default at all today — no default
   shape passes schema validation for the type — so there is nothing
   stamped in for a delta to pick up. That gap is its own piece of work,
   not fixed here.
8. **A negative delta on an absent field writes a negative duration.**
   Zero minus thirty minutes is minus thirty minutes, exactly as it
   would be if the field held `0s`. Negative durations are already
   valid values, and a project that considers them nonsense constrains
   the field with a minimum in its schema.
9. **A delta of zero creates the field at `0s`.** A delta always
   writes. The timer's rule that a very short session records nothing
   is the timer's, applied before it ever asks for a delta.
10. **The confirmation line shows what was on disk:**
    `effort: (unset) + 30min = 30min`. Rendering it as `0s` would claim
    the file said something it did not. A field written with no value
    reads `(unset)` too, rather than getting a marker of its own: it
    held nothing, and a second word for it would only make the user
    learn the difference between two spellings of one accident.

## Not in scope

- Any change to how deltas behave when the field *does* have a value.
