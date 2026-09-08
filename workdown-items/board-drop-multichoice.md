---
status: done
tags: [bug]
parent: misc-work
title: Dragging a card on a multichoice board wipes its other values
---

## In plain words

A board can group by a field where one item holds several values at
once, and such an item shows up in every column its values name.
Dragging that card to a different column should *add* the new value —
instead it throws away everything the item had and writes the target
column's single value in its place. Worse, it writes it in the wrong
shape, so the item is left failing validation. **Example:** an item
whose `platforms` field holds `[ios, android]` appears in both the
`ios` and the `android` column. Drag it onto `web` and the item ends
up with `platforms: web` — a bare value where a list is required.
Both original values are gone and the file is now invalid.

`moveCard` in `ui/src/lib/views/board/BoardView.svelte` sends
`{ op: 'replace', value: <string> }` for every board, but
`coerce_multichoice` (`crates/core/src/coerce.rs`) requires a sequence.
`Replace` writes first and validates on reload (the save-with-warning
rule in `crates/core/src/operations/set/mod.rs`), so the broken value
reaches disk and is only flagged afterwards.

Read from the code, not reproduced — confirm before fixing.

## Scope

Dropping onto a column of a multi-valued grouping field should append
that value rather than replace the whole field. How a value gets
*removed* still needs deciding (an affordance on the card, or leaving
it to the item editor); until that is settled, disabling the drop on
multi-valued boards is the safe stop-gap and fixes the data loss on
its own.

## Outcome (verified 2026-09-02)

Done, by the stop-gap this item named as sufficient — landed in the item
audit PR (#55), not in the tour PR that followed it.

`boardDragEnabled` (`ui/src/lib/views/board/dragPolicy.ts`) returns false
for `multichoice`, so the cards on such a board are not drag sources
(`Card.svelte` passes it to `use:draggable`) and `moveCard` re-checks it
before writing — a second guard, because a card dragged from another
tab's board carries only a bare MIME string and lands in the drop handler
without passing this board's drag sources at all. In place of the
affordance the board prints why it is off, so a board that ignores every
drag does not read as broken. `dragPolicy.test.ts` covers both halves.

The `{ op: 'replace', value: <string> }` write can no longer reach a
multi-valued field from a board, so the data loss — and the invalid
bare-value-where-a-list-is-required file it left behind — is gone.

Deliberately *not* done: making the drop append instead. This item left
that open on its own terms — the gesture cannot say whether it means add,
swap or discard, and how a value gets *removed* was never settled — so
the board is read-only rather than guessing. If appending is ever wanted
it needs its own item, with the removal affordance decided alongside it.
