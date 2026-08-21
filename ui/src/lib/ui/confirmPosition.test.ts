import { describe, it, expect } from 'vitest';
import { positionPopover, ANCHOR_GAP, VIEWPORT_MARGIN, type AnchorRect } from './confirmPosition';

function anchor(left: number, top: number, width: number, height: number): AnchorRect {
	return { left, top, width, bottom: top + height };
}

describe('positionPopover', () => {
	const popover = { width: 200, height: 100 };
	const viewport = { width: 1000, height: 800 };

	it('places the popover below the anchor, centered on it', () => {
		const position = positionPopover(anchor(100, 100, 80, 30), popover, viewport);
		expect(position.top).toBe(130 + ANCHOR_GAP);
		// Anchor center is at 140; popover center lands there too.
		expect(position.left).toBe(40);
	});

	it('flips above the anchor when below lacks room', () => {
		const position = positionPopover(anchor(100, 700, 80, 30), popover, viewport);
		expect(position.top).toBe(700 - ANCHOR_GAP - popover.height);
	});

	it('stays below but pinned to the viewport when neither side fits', () => {
		const tightViewport = { width: 400, height: 200 };
		const tall = { width: 100, height: 150 };
		const position = positionPopover(anchor(10, 80, 20, 40), tall, tightViewport);
		expect(position.top).toBe(tightViewport.height - VIEWPORT_MARGIN - tall.height);
	});

	it('clamps at the left viewport edge', () => {
		const position = positionPopover(anchor(0, 100, 20, 30), popover, viewport);
		expect(position.left).toBe(VIEWPORT_MARGIN);
	});

	it('clamps at the right viewport edge', () => {
		const narrowViewport = { width: 500, height: 800 };
		const position = positionPopover(anchor(470, 100, 20, 30), popover, narrowViewport);
		expect(position.left).toBe(narrowViewport.width - VIEWPORT_MARGIN - popover.width);
	});

	it('pins to the left margin when the popover is wider than the viewport', () => {
		const tinyViewport = { width: 150, height: 800 };
		const position = positionPopover(anchor(40, 100, 20, 30), popover, tinyViewport);
		expect(position.left).toBe(VIEWPORT_MARGIN);
	});
});
