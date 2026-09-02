// The animation loop: moves cards and camera between scenes.
//
// Imperative on purpose. A few hundred cards changing `transform` every
// frame is the one thing browsers composite on the GPU without layout,
// and writing the style directly keeps the per-frame cost at exactly
// that — no reactive graph in between. Svelte owns the DOM (creating the
// card elements, rendering edges and labels once a scene has settled);
// the engine owns positions and time, and reports back through hooks.
//
// A scene runs in two phases. `transition`: every card tweens from where
// it is to the scene's layout (with a small stagger so the flight reads
// as a swarm, not a slab) while the camera moves to the scene's `enter`
// pose. `hold`: cards rest, the camera eases from `enter` to `hold`, and
// after the scene's hold time the next scene begins. Every duration is
// scaled by `speed`; reduced motion shortens the flights to a cross-fade.

import {
	easeInOutCubic,
	flythroughOpacity,
	lerpPose,
	lerpPosition,
	linear,
	poseTransform
} from './motion';
import type { Easing } from './motion';
import type { CameraPose, Position, Scene } from './types';

export interface EngineHooks {
	/** A scene's transition has begun. */
	onSceneStart: (index: number) => void;
	/** Its cards are at rest; decoration and caption may appear. */
	onSettled: (index: number) => void;
	/** The last scene's hold has elapsed while playing. */
	onFinished: () => void;
}

export interface EngineOptions {
	reducedMotion: boolean;
}

/** Where every card starts: far behind the stage, invisible. */
const OFFSTAGE: Position = { x: 0, y: 0, z: -3000, opacity: 0 };

interface CardState {
	id: string;
	element: HTMLElement;
	current: Position;
	from: Position;
	to: Position;
	delayMs: number;
}

export class TourEngine {
	private readonly cards: CardState[] = [];
	private readonly camera: {
		current: CameraPose;
		from: CameraPose;
		to: CameraPose;
		startedAt: number;
		durationMs: number;
		easing: Easing;
	};
	private sceneIndex = -1;
	private phase: 'idle' | 'transition' | 'hold' = 'idle';
	private phaseStartedAt = 0;
	private now = 0;
	private frameHandle: number | null = null;
	private isPlaying = true;
	private speedFactor = 1;

	constructor(
		private readonly scenes: readonly Scene[],
		cardElements: ReadonlyMap<string, HTMLElement>,
		private readonly world: HTMLElement,
		private readonly hooks: EngineHooks,
		private readonly options: EngineOptions
	) {
		for (const [id, element] of cardElements) {
			this.cards.push({ id, element, current: OFFSTAGE, from: OFFSTAGE, to: OFFSTAGE, delayMs: 0 });
		}
		const initial: CameraPose = { tx: 0, ty: 0, tz: -1800, rx: 0, ry: 0 };
		this.camera = {
			current: initial,
			from: initial,
			to: initial,
			startedAt: 0,
			durationMs: 1,
			easing: easeInOutCubic
		};
		this.world.style.transform = poseTransform(initial);
	}

	get playing(): boolean {
		return this.isPlaying;
	}

	get currentScene(): number {
		return this.sceneIndex;
	}

	start(): void {
		if (this.scenes.length === 0) return;
		this.now = performance.now();
		this.enterScene(0);
		this.frameHandle = requestAnimationFrame(this.frame);
	}

	destroy(): void {
		if (this.frameHandle !== null) cancelAnimationFrame(this.frameHandle);
		this.frameHandle = null;
	}

	setPlaying(playing: boolean): void {
		this.isPlaying = playing;
		// Resuming restarts the hold clock so a long pause doesn't skip ahead at once.
		if (playing && this.phase === 'hold') this.phaseStartedAt = this.now;
	}

	setSpeed(factor: number): void {
		this.speedFactor = factor;
	}

	next(): void {
		if (this.sceneIndex + 1 < this.scenes.length) this.enterScene(this.sceneIndex + 1);
	}

	previous(): void {
		if (this.sceneIndex > 0) this.enterScene(this.sceneIndex - 1);
	}

	goTo(index: number): void {
		if (index >= 0 && index < this.scenes.length) this.enterScene(index);
	}

	private transitionMs(): number {
		return (this.options.reducedMotion ? 500 : 1600) * this.speedFactor;
	}

	private moveCamera(to: CameraPose, durationMs: number, easing: Easing): void {
		this.camera.from = { ...this.camera.current };
		this.camera.to = to;
		this.camera.startedAt = this.now;
		this.camera.durationMs = Math.max(1, durationMs);
		this.camera.easing = easing;
	}

	private enterScene(index: number): void {
		const scene = this.scenes[index];
		if (scene === undefined) return;
		this.sceneIndex = index;
		this.phase = 'transition';
		this.phaseStartedAt = this.now;
		this.cards.forEach((card, position) => {
			const target = scene.layout.get(card.id);
			card.from = { ...card.current };
			card.to =
				target === undefined ? OFFSTAGE : { ...target, opacity: target.opacity * scene.dim };
			// Reduced motion: cross-fade in place rather than fly.
			if (this.options.reducedMotion) card.from = { ...card.to, opacity: 0 };
			card.delayMs = this.options.reducedMotion ? 0 : (position % 12) * 25 * this.speedFactor;
		});
		// Reduced motion also skips the tilt: enter straight at the hold pose.
		const enter = this.options.reducedMotion ? scene.camera.hold : scene.camera.enter;
		this.moveCamera(enter, this.transitionMs(), easeInOutCubic);
		this.hooks.onSceneStart(index);
	}

	private settle(): void {
		const scene = this.scenes[this.sceneIndex];
		if (scene === undefined) return;
		this.phase = 'hold';
		this.phaseStartedAt = this.now;
		this.moveCamera(
			scene.camera.hold,
			scene.holdMs * this.speedFactor,
			scene.flythrough ? linear : easeInOutCubic
		);
		this.hooks.onSettled(this.sceneIndex);
	}

	private readonly frame = (time: number): void => {
		this.now = time;
		const scene = this.scenes[this.sceneIndex];
		if (scene === undefined) return;

		if (this.phase === 'transition') {
			const duration = this.transitionMs();
			let allSettled = true;
			for (const card of this.cards) {
				const t = Math.min(
					1,
					Math.max(0, (this.now - this.phaseStartedAt - card.delayMs) / duration)
				);
				if (t < 1) allSettled = false;
				card.current = lerpPosition(card.from, card.to, easeInOutCubic(t));
				this.paint(card);
			}
			if (allSettled) this.settle();
		}

		const cameraT = Math.min(1, (this.now - this.camera.startedAt) / this.camera.durationMs);
		this.camera.current = lerpPose(this.camera.from, this.camera.to, this.camera.easing(cameraT));
		this.world.style.transform = poseTransform(this.camera.current);

		if (scene.flythrough) {
			for (const card of this.cards) {
				card.element.style.opacity = flythroughOpacity(
					card.current.z,
					this.camera.current.tz,
					card.current.opacity
				).toFixed(3);
			}
		}

		if (
			this.phase === 'hold' &&
			this.isPlaying &&
			this.now - this.phaseStartedAt > scene.holdMs * this.speedFactor
		) {
			if (this.sceneIndex + 1 < this.scenes.length) {
				this.enterScene(this.sceneIndex + 1);
			} else {
				this.isPlaying = false;
				this.hooks.onFinished();
			}
		}
		this.frameHandle = requestAnimationFrame(this.frame);
	};

	private paint(card: CardState): void {
		const { x, y, z, opacity } = card.current;
		card.element.style.transform = `translate3d(${x.toFixed(1)}px, ${y.toFixed(1)}px, ${z.toFixed(1)}px)`;
		card.element.style.opacity = opacity.toFixed(3);
	}
}
