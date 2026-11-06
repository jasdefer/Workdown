// Placement store — where an item opened from a view is shown, the
// modal or the side panel. One reactive value over `itemPlacement.ts`,
// which owns the vocabulary and the localStorage persistence; the
// store only keeps the current choice and writes every change through.
//
// Read on first use, so a reload comes back with the reader's last
// choice; the view page picks its chrome from `value`.

import {
	loadPlacement,
	otherPlacement,
	savePlacement,
	type ItemPlacement
} from '$lib/items/itemPlacement';

let current = $state<ItemPlacement>(loadPlacement());

export const placementStore = {
	get value(): ItemPlacement {
		return current;
	},
	set(next: ItemPlacement): void {
		current = next;
		savePlacement(next);
	},
	toggle(): void {
		placementStore.set(otherPlacement(current));
	}
};
