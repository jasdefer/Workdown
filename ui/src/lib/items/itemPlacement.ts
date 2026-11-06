// Where an item opened from a view is shown: as the centered modal
// (the default, for reading one item properly) or docked at the side
// of the view (for clicking through many items, the view staying in
// sight and the next card one click away).
//
// The choice is the reader's, project-wide, and kept in localStorage
// like the theme — not in the URL, because a placement is a reading
// habit, not a location. `?item=` stays the one truth about which
// item is open; this only says where. Unknown or missing storage
// values fall back to the modal so a stale key can never leave the
// detail unreachable.

export type ItemPlacement = 'dialog' | 'side';

export const DEFAULT_PLACEMENT: ItemPlacement = 'dialog';

const STORAGE_KEY = 'workdown.itemPlacement';

/** The placement a stored value names; anything else is the default. */
export function placementOf(raw: string | null): ItemPlacement {
	return raw === 'side' ? 'side' : DEFAULT_PLACEMENT;
}

/** The other placement — what the switch button leads to. */
export function otherPlacement(placement: ItemPlacement): ItemPlacement {
	return placement === 'dialog' ? 'side' : 'dialog';
}

/** The switch button's label, naming where the item goes. */
export function switchLabel(placement: ItemPlacement): string {
	return placement === 'dialog' ? 'Dock to side' : 'Open as dialog';
}

export function loadPlacement(): ItemPlacement {
	if (typeof localStorage === 'undefined') return DEFAULT_PLACEMENT;
	try {
		return placementOf(localStorage.getItem(STORAGE_KEY));
	} catch {
		return DEFAULT_PLACEMENT;
	}
}

export function savePlacement(placement: ItemPlacement): void {
	if (typeof localStorage === 'undefined') return;
	try {
		localStorage.setItem(STORAGE_KEY, placement);
	} catch {
		// A blocked store only loses the memory, not the switch itself.
	}
}
