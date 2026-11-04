// Where an item's detail opens: the dialog over the current view, or
// the standalone page.
//
// The dialog is driven by the `?item=` query param on the current view
// route, so navigation is just a relative URL change — SvelteKit keeps
// the path and re-runs the view's load, which mounts the dialog.
// `itemHref` is for real `<a>` links (table cells, tree titles);
// `openItem` is for elements that can't be anchors (the draggable board
// card, an SVG rectangle, a canvas-drawn graph node), and
// `activateOnKey` gives those the keyboard half of the same gesture.
// `standaloneItemHref` is the page an item has outside any view.

import { goto } from '$app/navigation';

export function itemHref(id: string): string {
	return `?item=${encodeURIComponent(id)}`;
}

export function openItem(id: string): void {
	void goto(itemHref(id), { keepFocus: true, noScroll: true });
}

// Enter and Space activate a `role="button"` element — the keyboard
// equivalent of the click. Every non-anchor opener needs it, so it
// lives here rather than being re-inlined per view.
export function activateOnKey(event: KeyboardEvent, activate: () => void): void {
	if (event.key === 'Enter' || event.key === ' ') {
		event.preventDefault();
		activate();
	}
}

// The standalone page's address — for leaving the view (the dialog's
// open-standalone link) and for following a reference from that page,
// where there is no dialog to swap.
export function standaloneItemHref(id: string): string {
	return `/items/${encodeURIComponent(id)}`;
}
