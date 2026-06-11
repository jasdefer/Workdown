// Opening an item's detail panel from any view.
//
// The panel is driven by the `?item=` query param on the current view
// route, so navigation is just a relative URL change — SvelteKit keeps
// the path and re-runs the view's load, which mounts the slide-over.
// `itemHref` is for real `<a>` links (table cells, tree titles);
// `openItem` is for elements that can't be anchors (the draggable board
// card).

import { goto } from '$app/navigation';

export function itemHref(id: string): string {
	return `?item=${encodeURIComponent(id)}`;
}

export function openItem(id: string): void {
	void goto(itemHref(id), { keepFocus: true, noScroll: true });
}
