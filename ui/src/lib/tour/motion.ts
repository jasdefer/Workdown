// Camera and interpolation math for the tour. Pure, DOM-free.
//
// The camera is one CSS transform on the world container:
// `translate3d(tx, ty, tz) rotateX(rx) rotateY(ry)` under a fixed
// `perspective`. Moving the world toward negative z shrinks everything
// by `PERSPECTIVE / (PERSPECTIVE - tz)`, which is how a layout is fitted
// to the viewport; tilting happens about the world origin before the
// translation, so a pose reads as "tilt the layout, then frame it".

import type { Bounds, CameraPose, Position } from './types';

/** Must match the stage's CSS `perspective`. */
export const PERSPECTIVE = 1400;

export interface Viewport {
	width: number;
	height: number;
}

export type Easing = (t: number) => number;

export const linear: Easing = (t) => t;

export const easeInOutCubic: Easing = (t) =>
	t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2;

export function lerp(from: number, to: number, t: number): number {
	return from + (to - from) * t;
}

export function lerpPosition(from: Position, to: Position, t: number): Position {
	return {
		x: lerp(from.x, to.x, t),
		y: lerp(from.y, to.y, t),
		z: lerp(from.z, to.z, t),
		opacity: lerp(from.opacity, to.opacity, t)
	};
}

export function lerpPose(from: CameraPose, to: CameraPose, t: number): CameraPose {
	return {
		tx: lerp(from.tx, to.tx, t),
		ty: lerp(from.ty, to.ty, t),
		tz: lerp(from.tz, to.tz, t),
		rx: lerp(from.rx, to.rx, t),
		ry: lerp(from.ry, to.ry, t)
	};
}

/** The world container's `transform` value for a pose. */
export function poseTransform(pose: CameraPose): string {
	return (
		`translate3d(${pose.tx.toFixed(1)}px, ${pose.ty.toFixed(1)}px, ${pose.tz.toFixed(1)}px) ` +
		`rotateX(${pose.rx.toFixed(2)}deg) rotateY(${pose.ry.toFixed(2)}deg)`
	);
}

/** The on-screen scale factor a translation along z produces. */
export function scaleAt(tz: number): number {
	return PERSPECTIVE / (PERSPECTIVE - tz);
}

/** The z translation that produces a scale factor (never zooms in past 1). */
export function depthForScale(scale: number): number {
	const clamped = Math.min(1, Math.max(0.05, scale));
	return PERSPECTIVE - PERSPECTIVE / clamped;
}

/**
 * A front-on pose that frames `bounds` inside the viewport, leaving room
 * at the bottom for the caption and controls. Fractions of the viewport
 * rather than pixels so the same layout frames the same way on a laptop
 * and a projector.
 */
export function fitPose(bounds: Bounds, viewport: Viewport): CameraPose {
	const width = Math.max(1, bounds.maxX - bounds.minX);
	const height = Math.max(1, bounds.maxY - bounds.minY);
	const scale = Math.min((viewport.width * 0.86) / width, (viewport.height * 0.62) / height);
	const centreX = (bounds.minX + bounds.maxX) / 2;
	const centreY = (bounds.minY + bounds.maxY) / 2;
	return {
		tx: -centreX,
		// Nudge up: the caption and controls occupy the bottom of the stage.
		ty: -centreY - viewport.height * 0.06,
		tz: depthForScale(scale),
		rx: 0,
		ry: 0
	};
}

/** A pose offset from another: the tilted, further-away pose a scene enters through. */
export function offsetPose(pose: CameraPose, delta: Partial<CameraPose>): CameraPose {
	return {
		tx: pose.tx + (delta.tx ?? 0),
		ty: pose.ty + (delta.ty ?? 0),
		tz: pose.tz + (delta.tz ?? 0),
		rx: pose.rx + (delta.rx ?? 0),
		ry: pose.ry + (delta.ry ?? 0)
	};
}

/**
 * Opacity of a card at world depth `z` while the camera sits at `tz`:
 * cards about to pass the camera fade instead of blowing up to fill the
 * screen (and flipping once they are behind the perspective plane).
 */
export function flythroughOpacity(z: number, tz: number, base: number): number {
	const depth = z + tz;
	const fadeStart = PERSPECTIVE * 0.45;
	const fadeEnd = PERSPECTIVE * 0.65;
	if (depth <= fadeStart) return base;
	if (depth >= fadeEnd) return 0;
	return base * (1 - (depth - fadeStart) / (fadeEnd - fadeStart));
}
