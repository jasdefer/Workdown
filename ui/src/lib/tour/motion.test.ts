import { describe, expect, it } from 'vitest';
import {
	PERSPECTIVE,
	depthForScale,
	easeInOutCubic,
	fitPose,
	flythroughOpacity,
	lerpPose,
	offsetPose,
	poseTransform,
	scaleAt
} from './motion';

describe('easing', () => {
	it('easeInOutCubic is anchored at both ends and symmetric', () => {
		expect(easeInOutCubic(0)).toBe(0);
		expect(easeInOutCubic(1)).toBe(1);
		expect(easeInOutCubic(0.5)).toBeCloseTo(0.5);
		expect(easeInOutCubic(0.25) + easeInOutCubic(0.75)).toBeCloseTo(1);
	});
});

describe('depth and scale', () => {
	it('round-trips: the depth for a scale produces that scale', () => {
		for (const scale of [0.1, 0.5, 0.9, 1]) {
			expect(scaleAt(depthForScale(scale))).toBeCloseTo(scale);
		}
	});

	it('never zooms in past 1:1', () => {
		expect(depthForScale(3)).toBe(0);
		expect(scaleAt(0)).toBe(1);
	});
});

describe('fitPose', () => {
	const viewport = { width: 1000, height: 800 };

	it('centres the bounds and pulls back until the wider dimension fits', () => {
		const pose = fitPose({ minX: -2000, maxX: 2000, minY: -100, maxY: 100 }, viewport);
		expect(pose.tx).toBeCloseTo(0);
		expect(pose.rx).toBe(0);
		expect(pose.ry).toBe(0);
		// 4000 world px into 86% of 1000 screen px.
		expect(scaleAt(pose.tz)).toBeCloseTo(0.215);
	});

	it('offsets the translation to bring an off-centre layout to the middle', () => {
		const pose = fitPose({ minX: 100, maxX: 300, minY: 50, maxY: 150 }, viewport);
		expect(pose.tx).toBe(-200);
		expect(pose.ty).toBeLessThan(-100);
		expect(pose.tz).toBe(0);
	});
});

describe('poses', () => {
	it('offsetPose adds only the given deltas', () => {
		const base = { tx: 1, ty: 2, tz: 3, rx: 4, ry: 5 };
		expect(offsetPose(base, { rx: 10, tz: -100 })).toEqual({
			tx: 1,
			ty: 2,
			tz: -97,
			rx: 14,
			ry: 5
		});
	});

	it('lerpPose interpolates every component', () => {
		const from = { tx: 0, ty: 0, tz: 0, rx: 0, ry: 0 };
		const to = { tx: 10, ty: 20, tz: 30, rx: 40, ry: 50 };
		expect(lerpPose(from, to, 0.5)).toEqual({ tx: 5, ty: 10, tz: 15, rx: 20, ry: 25 });
	});

	it('poseTransform translates before it rotates', () => {
		expect(poseTransform({ tx: 1, ty: 2, tz: 3, rx: 4, ry: 5 })).toBe(
			'translate3d(1.0px, 2.0px, 3.0px) rotateX(4.00deg) rotateY(5.00deg)'
		);
	});
});

describe('flythroughOpacity', () => {
	it('keeps the base opacity far from the camera and fades to zero before the perspective plane', () => {
		expect(flythroughOpacity(-1000, 0, 1)).toBe(1);
		expect(flythroughOpacity(0, PERSPECTIVE * 0.9, 1)).toBe(0);
		const midway = flythroughOpacity(0, PERSPECTIVE * 0.55, 1);
		expect(midway).toBeGreaterThan(0);
		expect(midway).toBeLessThan(1);
	});
});
