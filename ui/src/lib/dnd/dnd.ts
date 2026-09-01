// Reusable native HTML5 drag-and-drop actions.
//
// `draggable` marks an element as a drag source carrying a work-item id;
// `dropTarget` accepts such a drop and calls back with the id. Kept
// generic (not board-specific) so other views — tree reparenting, etc.
// — can reuse the same mechanism. If we ever outgrow native DnD (touch,
// animated reordering), only this file changes.
//
// Visual feedback is applied as inline styles rather than CSS classes:
// Svelte's scoped-style compiler strips selectors for classes that only
// appear at runtime, so a `.dragging` rule in a component wouldn't match.

import type { Action } from 'svelte/action';

const MIME = 'application/x-workdown-id';

/**
 * Drag source parameter: the work-item id to carry, and whether the
 * element is a drag source at all. `enabled: false` leaves the element
 * in place but inert — no grab cursor, no drag ghost, no drop. A `use:`
 * directive can't be applied conditionally, so the switch lives here
 * rather than at the call site.
 */
export interface DraggableOptions {
	id: string;
	/** Defaults to true. */
	enabled?: boolean;
}

/** Drag source. */
export const draggable: Action<HTMLElement, DraggableOptions> = (node, options) => {
	let payload = options;

	function onDragStart(event: DragEvent): void {
		if (!event.dataTransfer) return;
		event.dataTransfer.setData(MIME, payload.id);
		event.dataTransfer.effectAllowed = 'move';
		node.style.opacity = '0.4';
	}
	function onDragEnd(): void {
		node.style.opacity = '';
	}
	function apply(): void {
		node.draggable = payload.enabled ?? true;
	}

	apply();
	node.addEventListener('dragstart', onDragStart);
	node.addEventListener('dragend', onDragEnd);

	return {
		update(next: DraggableOptions) {
			payload = next;
			apply();
		},
		destroy() {
			node.removeEventListener('dragstart', onDragStart);
			node.removeEventListener('dragend', onDragEnd);
		}
	};
};

/** Drop zone. Parameter is the callback invoked with the dropped id. */
export const dropTarget: Action<HTMLElement, (id: string) => void> = (node, onDrop) => {
	let handler = onDrop;

	function carriesId(event: DragEvent): boolean {
		return event.dataTransfer?.types.includes(MIME) ?? false;
	}
	function onDragOver(event: DragEvent): void {
		if (!carriesId(event) || !event.dataTransfer) return;
		// preventDefault marks this element as a valid drop target.
		event.preventDefault();
		event.dataTransfer.dropEffect = 'move';
		node.style.outline = '2px dashed var(--color-fg-muted)';
		node.style.outlineOffset = '-2px';
	}
	function clearHighlight(): void {
		node.style.outline = '';
		node.style.outlineOffset = '';
	}
	function onDropEvent(event: DragEvent): void {
		clearHighlight();
		const id = event.dataTransfer?.getData(MIME);
		if (id !== undefined && id !== '') {
			event.preventDefault();
			handler(id);
		}
	}

	node.addEventListener('dragover', onDragOver);
	node.addEventListener('dragleave', clearHighlight);
	node.addEventListener('drop', onDropEvent);

	return {
		update(next: (id: string) => void) {
			handler = next;
		},
		destroy() {
			node.removeEventListener('dragover', onDragOver);
			node.removeEventListener('dragleave', clearHighlight);
			node.removeEventListener('drop', onDropEvent);
		}
	};
};
