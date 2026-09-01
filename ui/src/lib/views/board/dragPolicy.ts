// Whether a board's cards may be dragged between columns.
//
// A board groups by a `choice`, `multichoice` or `string` field. The first
// and last hold one value per item, so a card sits in exactly one column
// and dragging it reads unambiguously: set the field to the target
// column's value.
//
// A `multichoice` field does not. Such an item appears in *every* column
// its values name (`crates/core/src/view_data/board.rs`), so a drop could
// mean "add this value", "swap the one you dragged out of for this one",
// or "discard the rest" — three different writes, with nothing in the
// gesture to choose between them. Rather than guess, these boards are
// read-only: the cards render, and values are edited in the item panel.
//
// The server reports the grouping field's type as a fact; the decision
// below is the UI's.

import type { FieldType } from '$lib/api/generated/FieldType';

/**
 * True when a card on a board grouped by `fieldType` may be dragged to a
 * different column.
 */
export function boardDragEnabled(fieldType: FieldType): boolean {
	return fieldType !== 'multichoice';
}

/**
 * Why dragging is off, for the board to show in place of the affordance.
 * `null` when dragging is available and there is nothing to explain.
 */
export function boardDragDisabledReason(fieldType: FieldType): string | null {
	if (boardDragEnabled(fieldType)) return null;
	return 'Grouped by a multi-value field, so cards can hold several columns at once — edit their values in the item panel.';
}
