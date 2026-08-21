/* Where to place the confirm popover relative to its anchor.
 *
 * Pure math so it is unit-testable without a browser: below the anchor
 * when it fits, flipped above when it does not (and above fits),
 * clamped to the viewport edges as a last resort. Horizontally the
 * popover is centered on the anchor, clamped the same way.
 */

export interface AnchorRect {
	top: number;
	bottom: number;
	left: number;
	width: number;
}

export interface Size {
	width: number;
	height: number;
}

export interface Position {
	top: number;
	left: number;
}

/** Gap between the anchor and the popover, in pixels. */
export const ANCHOR_GAP = 6;

/** Minimum distance kept from every viewport edge, in pixels. */
export const VIEWPORT_MARGIN = 8;

export function positionPopover(anchor: AnchorRect, popover: Size, viewport: Size): Position {
	const below = anchor.bottom + ANCHOR_GAP;
	const above = anchor.top - ANCHOR_GAP - popover.height;
	const fitsBelow = below + popover.height <= viewport.height - VIEWPORT_MARGIN;
	const fitsAbove = above >= VIEWPORT_MARGIN;

	// When neither side fits, stay below and let the clamp pin the
	// popover inside the viewport (it may then overlap the anchor).
	const top = clamp(
		fitsBelow || !fitsAbove ? below : above,
		VIEWPORT_MARGIN,
		viewport.height - VIEWPORT_MARGIN - popover.height
	);

	const centered = anchor.left + anchor.width / 2 - popover.width / 2;
	const left = clamp(centered, VIEWPORT_MARGIN, viewport.width - VIEWPORT_MARGIN - popover.width);

	return { top, left };
}

/** Clamps to [min, max]; when the range is inverted, min wins. */
function clamp(value: number, min: number, max: number): number {
	return Math.max(Math.min(value, max), min);
}
