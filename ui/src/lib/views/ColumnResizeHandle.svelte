<!--
  Drag handle for column resizing — shared between tree and table.

  Sits absolutely positioned on the right edge of its parent cell; on
  pointer drag, writes the new pixel width into the shared `widths` map
  under `columnIndex`. The parent cell must establish a positioning
  context (sticky / relative / fixed).

  Uses Pointer Events with setPointerCapture so a drag that leaves the
  handle bounds keeps tracking until release — same way native window
  resize works. Mouse and touch both flow through this single path.

  Initial drag width is measured from the parent cell's current bounding
  rect, so the first drag starts from whatever auto-size the layout had
  picked (max-content / 1fr / auto table layout) rather than snapping
  to some default. The optional `onBeforeStart` callback runs *before*
  that measurement — used by the table view to seed all sibling column
  widths so `table-layout: fixed` doesn't reflow other columns on the
  first drag.
-->
<script lang="ts">
	import type { SvelteMap } from 'svelte/reactivity';

	interface Props {
		columnIndex: number;
		widths: SvelteMap<number, number>;
		minWidth?: number;
		onBeforeStart?: (handle: HTMLElement) => void;
	}

	let { columnIndex, widths, minWidth = 64, onBeforeStart }: Props = $props();

	let dragState: { startX: number; startWidth: number } | null = null;

	function startDrag(event: PointerEvent) {
		event.preventDefault();
		event.stopPropagation();
		const handle = event.currentTarget as HTMLElement;
		onBeforeStart?.(handle);
		const cell = handle.parentElement;
		if (!cell) return;
		const rect = cell.getBoundingClientRect();
		dragState = { startX: event.clientX, startWidth: rect.width };
		handle.setPointerCapture(event.pointerId);
	}

	function onDrag(event: PointerEvent) {
		if (dragState === null) return;
		const delta = event.clientX - dragState.startX;
		const next = Math.max(minWidth, dragState.startWidth + delta);
		widths.set(columnIndex, next);
	}

	function endDrag(event: PointerEvent) {
		if (dragState === null) return;
		dragState = null;
		const handle = event.currentTarget as HTMLElement;
		if (handle.hasPointerCapture(event.pointerId)) {
			handle.releasePointerCapture(event.pointerId);
		}
	}
</script>

<div
	class="handle"
	role="separator"
	aria-orientation="vertical"
	aria-label="Resize column"
	onpointerdown={startDrag}
	onpointermove={onDrag}
	onpointerup={endDrag}
	onpointercancel={endDrag}
></div>

<style>
	.handle {
		position: absolute;
		top: 0;
		right: -2px;
		bottom: 0;
		width: 5px;
		cursor: col-resize;
		user-select: none;
		touch-action: none;
		z-index: 4;
		background-color: transparent;
		transition: background-color 0.1s;
	}

	.handle:hover,
	.handle:active {
		background-color: var(--color-accent);
	}
</style>
