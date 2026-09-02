---
status: to_do
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
