<!--
  The tour stage. Builds the scenes for the measured viewport, renders
  one element per card inside the 3D world, and hands positions and
  time to `TourEngine`. Everything that appears once a scene has settled
  — edges, labels, caption, the title and numbers overlays — is rendered
  reactively here from `settledScene`; the engine only moves cards and
  the camera.

  The stage fills `.app-main`; the header stays, so the tour is a page
  of the app rather than a takeover, and its last scene navigates to the
  landing view through `onFinished`.
-->
<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { TourEngine } from './engine';
	import { CARD_HEIGHT, edgePath } from './layouts';
	import { buildTour } from './scenes';
	import type { Tour, TourInput } from './scenes';
	import type { Scene } from './types';

	interface Props {
		input: Omit<TourInput, 'viewport' | 'today'>;
		onFinished: (viewId: string) => void;
		onExit: () => void;
	}

	let { input, onFinished, onExit }: Props = $props();

	let stage = $state<HTMLElement | null>(null);
	let world = $state<HTMLElement | null>(null);
	let tour = $state<Tour | null>(null);
	// One element per card, keyed by id; filled by `bind:this` as the
	// cards render and read once when the engine is created.
	const elements: Record<string, HTMLElement> = {};
	let engine: TourEngine | null = null;

	let sceneIndex = $state(0);
	let settled = $state(false);
	let playing = $state(true);
	let speed = $state(1);

	const scene = $derived<Scene | null>(tour?.scenes[sceneIndex] ?? null);
	const settledScene = $derived<Scene | null>(settled ? scene : null);
	const landingViewId = $derived(tour?.scenes.at(-1)?.landingViewId ?? null);
	const todayExtent = $derived.by(() => {
		if (!settledScene?.todayLine) return null;
		let maxY = 0;
		for (const position of settledScene.layout.values()) {
			if (position.opacity > 0) maxY = Math.max(maxY, position.y);
		}
		return { top: -CARD_HEIGHT * 1.6, bottom: maxY + CARD_HEIGHT };
	});

	onMount(() => {
		if (stage === null) return;
		const built = buildTour({
			...input,
			viewport: { width: stage.clientWidth, height: stage.clientHeight },
			today: new Date()
		});
		tour = built;
		// Unmounting before the tick below resolves would otherwise run the
		// cleanup while `engine` is still null, and the engine created after
		// it would animate detached nodes forever.
		let destroyed = false;
		// The card elements exist only after this state change renders.
		void tick().then(() => {
			if (destroyed || world === null || built.cards.length === 0) return;
			const cardElements = new Map<string, HTMLElement>();
			for (const card of built.cards) {
				const element = elements[card.id];
				if (element !== undefined) cardElements.set(card.id, element);
			}
			engine = new TourEngine(
				built.scenes,
				cardElements,
				world,
				{
					onSceneStart: (index) => {
						sceneIndex = index;
						settled = false;
					},
					onSettled: () => {
						settled = true;
					},
					onFinished: () => {
						playing = false;
						if (landingViewId !== null) onFinished(landingViewId);
					}
				},
				{ reducedMotion: window.matchMedia('(prefers-reduced-motion: reduce)').matches }
			);
			engine.start();
		});
		return () => {
			destroyed = true;
			engine?.destroy();
		};
	});

	function togglePlaying(): void {
		playing = !playing;
		engine?.setPlaying(playing);
	}

	function changeSpeed(event: Event): void {
		speed = Number((event.currentTarget as HTMLSelectElement).value);
		engine?.setSpeed(speed);
	}

	function onKeydown(event: KeyboardEvent): void {
		// Space on a focused control already activates it; don't also toggle.
		const onControl =
			event.target instanceof HTMLButtonElement || event.target instanceof HTMLSelectElement;
		if (event.key === 'ArrowRight') engine?.next();
		else if (event.key === 'ArrowLeft') engine?.previous();
		else if (event.key === ' ' && !onControl) {
			event.preventDefault();
			togglePlaying();
		}
	}

	const labelShift = (align: 'start' | 'center' | 'end'): string =>
		align === 'start' ? '0' : align === 'center' ? '-50%' : '-100%';
</script>

<svelte:window onkeydown={onKeydown} />

<div class="tour" bind:this={stage}>
	{#if tour !== null && tour.cards.length === 0}
		<p class="empty">Nothing to tour yet — the tour shows the work items your views contain.</p>
	{/if}

	<div class="world" bind:this={world}>
		{#if settledScene !== null}
			{#key sceneIndex}
				<svg class="edges" aria-hidden="true">
					<g transform="translate(2000 2000)">
						{#each settledScene.edges as edge, index (index)}
							<path class="edge" class:hot={edge.hot} d={edgePath(edge)} />
						{/each}
						{#if todayExtent !== null}
							<line class="today" x1="0" x2="0" y1={todayExtent.top} y2={todayExtent.bottom} />
						{/if}
					</g>
				</svg>
				{#each settledScene.labels as label, index (index)}
					<div
						class="label"
						class:accent={label.tone === 'accent'}
						style:transform={`translate(${label.x.toFixed(1)}px, ${label.y.toFixed(1)}px) translateX(${labelShift(label.align)})`}
					>
						{label.text}
					</div>
				{/each}
			{/key}
		{/if}

		{#if tour !== null}
			{#each tour.cards as card (card.id)}
				<div
					class="card"
					class:tinted={card.background !== null}
					class:compact={card.compact}
					style:--item-color={card.background}
					bind:this={elements[card.id]}
				>
					{#if !card.compact}
						<span class="title">{card.title}</span>
						<!-- The second line is the subtitle role when set, else the id:
						     the title keeps the full width either way. -->
						<span class="subtitle" class:id={card.subtitle === null}
							>{card.subtitle ?? card.id}</span
						>
					{/if}
				</div>
			{/each}
		{/if}
	</div>

	<!-- Overlays sit above the 3D stage in plain 2D. -->
	<div class="overlay" class:on={settledScene?.overlay?.kind === 'title'}>
		<div class="titlecard">
			<div class="eyebrow">Project overview</div>
			<h1>{input.project?.name ?? 'Workdown'}</h1>
			{#if input.project?.description}
				<p>{input.project.description}</p>
			{/if}
		</div>
	</div>
	<div class="overlay" class:on={settledScene?.overlay?.kind === 'metrics'}>
		{#if settledScene?.overlay?.kind === 'metrics'}
			<div class="metrics" class:many={settledScene.overlay.tiles.length > 4}>
				{#each settledScene.overlay.tiles as tile, index (index)}
					<div class="metric">
						<div class="value">{tile.value}</div>
						<div class="name">{tile.label}</div>
					</div>
				{/each}
			</div>
		{/if}
	</div>

	<div class="caption" class:hidden={settledScene?.caption === null || settledScene === null}>
		{settledScene?.caption ?? ''}
	</div>

	{#if tour !== null && tour.scenes.length > 0}
		<div class="controls" role="toolbar" aria-label="Tour controls">
			<button type="button" title="Previous scene (←)" onclick={() => engine?.previous()}>‹</button>
			<button
				type="button"
				title={playing ? 'Pause (space)' : 'Play (space)'}
				onclick={togglePlaying}
			>
				{playing ? '❚❚' : '▶'}
			</button>
			<button type="button" title="Next scene (→)" onclick={() => engine?.next()}>›</button>
			<div class="dots">
				{#each tour.scenes as entry, index (index)}
					<button
						type="button"
						class="dot"
						class:on={index === sceneIndex}
						class:past={index < sceneIndex}
						title={entry.name}
						aria-label={`Scene ${(index + 1).toString()}: ${entry.name}`}
						onclick={() => engine?.goTo(index)}
					></button>
				{/each}
			</div>
			<span class="scene-name">{scene?.name ?? ''}</span>
			<label class="speed">
				speed
				<select value={speed.toString()} onchange={changeSpeed}>
					<option value="1.5">slow</option>
					<option value="1">1×</option>
					<option value="0.5">fast</option>
				</select>
			</label>
			<button type="button" class="exit" title="Leave the tour" onclick={onExit}>✕</button>
		</div>
	{/if}
</div>

<style>
	.tour {
		position: absolute;
		inset: 0;
		overflow: hidden;
		background: var(--color-canvas);
		/* Must match `PERSPECTIVE` in motion.ts. */
		perspective: 1400px;
		perspective-origin: 50% 50%;
	}

	.empty {
		position: absolute;
		inset: 0;
		display: grid;
		place-items: center;
		color: var(--color-fg-muted);
		margin: 0;
	}

	.world {
		position: absolute;
		left: 50%;
		top: 50%;
		width: 0;
		height: 0;
		transform-style: preserve-3d;
		will-change: transform;
	}

	/* A card is centred on its world position: the engine writes only
	   `transform` and `opacity`, the margin does the centring. */
	.card {
		position: absolute;
		left: 0;
		top: 0;
		width: 150px;
		height: 60px;
		margin: -30px 0 0 -75px;
		box-sizing: border-box;
		padding: 7px 9px;
		display: flex;
		flex-direction: column;
		gap: 2px;
		background: var(--color-card);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		box-shadow: var(--shadow-sm);
		will-change: transform, opacity;
		backface-visibility: hidden;
		opacity: 0;
	}

	.card.tinted {
		--tint-base: var(--color-card);
		background: var(--tint-wash);
		border-left: 3px solid var(--item-color);
	}

	/* Scale guard: leaf items in a big project are dots. */
	.card.compact {
		width: 14px;
		height: 14px;
		margin: -7px 0 0 -7px;
		padding: 0;
		border-radius: var(--radius-full);
		border: 1px solid var(--color-border);
		background: var(--item-color, var(--color-fg-muted));
	}

	.title {
		font-size: 12px;
		font-weight: 600;
		line-height: 1.25;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.subtitle {
		font-size: 10px;
		color: var(--color-fg-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.subtitle.id {
		font-family: var(--font-mono);
	}

	.label {
		position: absolute;
		left: 0;
		top: 0;
		font-size: 11px;
		font-weight: 600;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--color-fg-muted);
		white-space: nowrap;
	}

	.label.accent {
		color: var(--color-accent);
	}

	/* A 4000×4000 canvas centred on the world origin: edges are drawn in
	   world coordinates without worrying about negative SVG space. */
	.edges {
		position: absolute;
		left: -2000px;
		top: -2000px;
		width: 4000px;
		height: 4000px;
		/* The reset caps every svg at its container's width; the world is 0px wide. */
		max-width: none;
		overflow: visible;
		pointer-events: none;
	}

	.edge {
		fill: none;
		stroke: var(--color-fg-muted);
		stroke-opacity: 0.55;
		stroke-width: 1.25;
		stroke-dasharray: 600;
		animation: draw 900ms ease-out both;
	}

	.edge.hot {
		stroke: var(--color-accent);
		stroke-opacity: 1;
		stroke-width: 2;
	}

	.today {
		stroke: var(--color-accent);
		stroke-width: 1.5;
		stroke-dasharray: 4 4;
	}

	@keyframes draw {
		from {
			stroke-dashoffset: 600;
		}
		to {
			stroke-dashoffset: 0;
		}
	}

	.overlay {
		position: absolute;
		inset: 0;
		display: grid;
		place-items: center;
		pointer-events: none;
		opacity: 0;
		transition: opacity 600ms ease;
	}

	.overlay.on {
		opacity: 1;
	}

	.titlecard {
		text-align: center;
		max-width: 60ch;
		padding: 0 var(--space-6);
	}

	.eyebrow,
	.metric .name {
		font-size: 12px;
		letter-spacing: 0.1em;
		text-transform: uppercase;
		color: var(--color-fg-muted);
	}

	.titlecard h1 {
		font-size: clamp(2rem, 6vw, 4rem);
		margin: var(--space-2) 0 var(--space-3);
		letter-spacing: -0.02em;
		text-wrap: balance;
	}

	.titlecard p {
		color: var(--color-fg-muted);
		font-size: var(--text-lg);
		margin: 0;
	}

	.metrics {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
		gap: var(--space-8) var(--space-8);
		width: min(900px, 90%);
	}

	.metrics.many .value {
		font-size: clamp(1.75rem, 3.5vw, 2.5rem);
	}

	.metric .value {
		font-size: clamp(2.25rem, 5vw, 3.5rem);
		font-weight: 700;
		letter-spacing: -0.03em;
		font-variant-numeric: tabular-nums;
		line-height: 1;
	}

	.metric .name {
		margin-top: var(--space-2);
	}

	.caption,
	.controls {
		position: absolute;
		left: 50%;
		transform: translateX(-50%);
		background: color-mix(in srgb, var(--color-card) 85%, transparent);
		backdrop-filter: blur(6px);
		border: 1px solid var(--color-border);
	}

	.caption {
		bottom: 84px;
		max-width: 48ch;
		padding: var(--space-2) var(--space-4);
		border-radius: var(--radius-md);
		font-size: 15px;
		line-height: 1.4;
		text-align: center;
		text-wrap: balance;
		transition:
			opacity 400ms ease,
			transform 400ms ease;
	}

	.caption.hidden {
		opacity: 0;
		transform: translate(-50%, 8px);
	}

	.controls {
		bottom: 24px;
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: 6px 10px;
		border-radius: var(--radius-full);
	}

	.controls button {
		all: unset;
		cursor: pointer;
		width: 32px;
		height: 32px;
		display: grid;
		place-items: center;
		border-radius: 50%;
		color: var(--color-fg);
		font-size: 14px;
	}

	.controls button:hover {
		background: var(--color-border);
	}

	.controls button:focus-visible {
		outline: 2px solid var(--color-accent);
		outline-offset: 1px;
	}

	.dots {
		display: flex;
		gap: 6px;
		padding: 0 var(--space-2);
	}

	.controls .dot {
		width: 8px;
		height: 8px;
		background: var(--color-border);
	}

	.controls .dot.past {
		background: var(--color-fg-muted);
	}

	.controls .dot.on {
		background: var(--color-accent);
	}

	.scene-name {
		font-size: 11px;
		color: var(--color-fg-muted);
		font-family: var(--font-mono);
		min-width: 9ch;
		text-align: right;
	}

	.speed {
		font-size: 11px;
		color: var(--color-fg-muted);
	}

	.speed select {
		font: inherit;
		color: inherit;
		background: transparent;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		padding: 2px 4px;
	}

	.controls .exit {
		color: var(--color-fg-muted);
	}
</style>
