// Selection store — which item's detail is open, and which was opened
// last. Every item-presenting view asks `markOf` for each item it
// draws, so the open item is ringed in the view behind its dialog and
// the trace of the last one stays after the dialog closes.
//
// The open id follows the `?item=` query param: the page that reads
// the param hands it over (and hands over `null` when it leaves). The
// last id is session-local and project-wide — it survives a change of
// view, so the item is still ringed where it appears next, and is gone
// on reload. Nothing is persisted.

import { markOf, type ItemMark } from '$lib/items/itemMark';

class SelectionStore {
	openId = $state<string | null>(null);
	lastId = $state<string | null>(null);

	/** The item whose detail is open, or `null` when none is. Opening
	 * one also makes it the last opened. */
	setOpen(id: string | null): void {
		this.openId = id;
		if (id !== null) this.lastId = id;
	}

	markOf(itemId: string): ItemMark {
		return markOf(itemId, this.openId, this.lastId);
	}
}

export const selectionStore = new SelectionStore();
