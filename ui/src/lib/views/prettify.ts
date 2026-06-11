// Prettify a kebab/snake-case identifier into a human-readable label.
// "status-board" → "Status Board"; "implement-user-login" → "Implement User Login".
// Used as the title fallback when a Card has no explicit title and as
// the navigation label fallback when a ViewSummary has no title.

import type { Card } from '$lib/api/generated/Card';
import type { ItemRef } from '$lib/api/generated/ItemRef';
import type { ViewSummary } from '$lib/api/generated/ViewSummary';
import type { WorkItemId } from '$lib/api/generated/WorkItemId';

export function prettifyId(id: string): string {
	return id
		.split(/[-_]/)
		.filter((part) => part.length > 0)
		.map((part) => part.charAt(0).toUpperCase() + part.slice(1))
		.join(' ');
}

/**
 * Display label for a card: its title, falling back to the prettified
 * id. This is the documented title-fallback convention — `title` is
 * optional on work items — kept in one place so every view renders the
 * same label for the same item.
 */
export function cardLabel(card: Pick<Card, 'id' | 'title'>): string {
	return card.title ?? prettifyId(card.id);
}

/**
 * Display label for an id resolved through a view's `items` sidecar
 * map (table/line-chart link resolution): the referenced item's title,
 * falling back to the prettified id — also when the id is absent from
 * the map entirely. Note: `Cell.svelte`'s link *rendering* deliberately
 * differs for absent ids (raw id, signalling a broken link); it only
 * routes resolved ids through this helper.
 */
export function itemRefLabel(items: Partial<Record<WorkItemId, ItemRef>>, id: WorkItemId): string {
	return items[id]?.title ?? prettifyId(id);
}

/**
 * Display label for a view-navigation link: its title, falling back to
 * the prettified id. `ViewSummary.title` is always `None` today (no
 * `display_title:` source in `views.yaml` yet), so this currently always
 * prettifies the id — kept here so the nav switches to real titles for
 * free once that field lands.
 */
export function viewLabel(view: Pick<ViewSummary, 'id' | 'title'>): string {
	return view.title ?? prettifyId(view.id);
}
