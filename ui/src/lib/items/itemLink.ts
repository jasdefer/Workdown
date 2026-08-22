// Opening an item's detail panel from any view.
//
// The panel is driven by the `?item=` query param on the current view
// route, so navigation is just a relative URL change — SvelteKit keeps
// the path and re-runs the view's load, which mounts the slide-over.
// `itemHref` is for real `<a>` links (table cells, tree titles);
// `openItem` is for elements that can't be anchors (the draggable board
// card, an SVG rectangle, a canvas-drawn graph node), and
// `activateOnKey` gives those the keyboard half of the same gesture.

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
