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

/** Drag source. Parameter is the work-item id to carry. */
export const draggable: Action<HTMLElement, string> = (node, id) => {
	let payload = id;

	function onDragStart(event: DragEvent): void {
		if (!event.dataTransfer) return;
		event.dataTransfer.setData(MIME, payload);
		event.dataTransfer.effectAllowed = 'move';
		node.style.opacity = '0.4';
	}
	function onDragEnd(): void {
		node.style.opacity = '';
	}

	node.draggable = true;
	node.addEventListener('dragstart', onDragStart);
	node.addEventListener('dragend', onDragEnd);

	return {
		update(next: string) {
			payload = next;
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
