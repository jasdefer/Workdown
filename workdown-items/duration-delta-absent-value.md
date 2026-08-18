---
id: duration-delta-absent-value
status: to_do
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
   deliberate and gets written down where the error taxonomy is
   described, so it does not read as an oversight.

## Not in scope

- Any change to how deltas behave when the field *does* have a value.
