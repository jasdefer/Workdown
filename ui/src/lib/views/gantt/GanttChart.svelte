<!--
  Bespoke horizontal-time gantt engine. Rows are items, time runs along
  the X axis. The label column is sticky-left and the date header is
  sticky-top inside one scroll container, so labels and the axis stay put
  while you scan a wide schedule.

  All the time math (range, granularity, ticks, date→x) lives in `scale.ts`
  and is unit-tested; this component is layout + interaction over it. The
  scale's single density knob (px/day per granularity) keeps even a
  multi-year chart scrollable rather than tens of thousands of px wide.

  Bars carry resolved `[start, end]` dates already (the three input recipes
  collapse server-side), so there's nothing to resolve here. Hovering a bar
  shows its full card by reusing the board <Card>; the sticky label column
  links to the item's detail panel via `?item=` (same pattern as the table's
  first column). The graph view's nodes still defer, being cytoscape-drawn.

  `sections` is the one input shape: a flat gantt passes a single
  label-less section; the by-depth / by-initiative variants pass one
  labelled section per level / initiative. Every section renders in this
  single chart, so the range is computed once across all sections' bars
  and every band lines up on one global scale.
-->
<script module lang="ts">
	import type { GanttBar } from '$lib/api/generated/GanttBar';

	/** One labelled band of bars. `label: null` renders no band header. */
	export interface GanttSection {
		label: string | null;
		bars: GanttBar[];
	}
</script>

<script lang="ts">
	import type { Card as CardData } from '$lib/api/generated/Card';
	import { itemHref } from '$lib/items/itemLink';
	import Card from '$lib/views/board/Card.svelte';
	import { formatIsoDate } from '$lib/views/format';
	import { cardLabel } from '$lib/views/prettify';
	import { barGeometry, buildAxis, computeRange, offsetForDate } from './scale';

	interface Props {
		sections: GanttSection[];
	}

	let { sections }: Props = $props();

	// Layout constants (px). Tuned by eye; the chart is read-only so these
	// are the only sizing knobs.
	const LABEL_WIDTH = 220;
	const ROW_HEIGHT = 30;
	const BAR_HEIGHT = 18;
	const BAND_HEIGHT = 30;
	const PERIOD_ROW_HEIGHT = 22;
	const TICK_ROW_HEIGHT = 22;
	const HEADER_HEIGHT = PERIOD_ROW_HEIGHT + TICK_ROW_HEIGHT;

	const allBars = $derived(sections.flatMap((section) => section.bars));

	// Container width, measured via `bind:clientWidth` below. Drives tier
	// promotion in `chooseGranularity`: a wide viewport can fit a finer
	// unit (day vs week, week vs month) than the span alone would pick.
	// The timeline area = container width minus the sticky label column.
	let availableWidth = $state(0);
	const effectiveWidth = $derived(Math.max(0, availableWidth - LABEL_WIDTH));

	const resolved = $derived(computeRange(allBars, effectiveWidth));

	const axis = $derived(resolved ? buildAxis(resolved.range, resolved.granularity) : null);

	// Today's date as a local `YYYY-MM-DD`, for the marker line.
	const todayIso = formatIsoDate(new Date());

	const todayOffset = $derived(
		resolved && axis ? offsetForDate(todayIso, resolved.range, axis.pxPerDay) : null
	);

	function barLabel(bar: GanttBar): string {
		return cardLabel(bar.card);
	}

	function barGeom(bar: GanttBar): { left: number; width: number } {
		// Guarded by the {#if resolved && axis} wrapper around all bars.
		if (!resolved || !axis) return { left: 0, width: 0 };
		return barGeometry(bar.start, bar.end, resolved.range, axis.pxPerDay);
	}

	function barDescription(bar: GanttBar): string {
		return `${barLabel(bar)} (${bar.start} to ${bar.end})`;
	}

	// Hover popover — reuse the board card, positioned at the pointer with a
	// small offset, clamped to the viewport so it never runs off-screen.
	let hovered = $state<{ card: CardData; x: number; y: number } | null>(null);

	function showCard(event: MouseEvent, card: CardData): void {
		hovered = { card, x: event.clientX, y: event.clientY };
	}
	function moveCard(event: MouseEvent): void {
		if (hovered) hovered = { ...hovered, x: event.clientX, y: event.clientY };
	}
	function hideCard(): void {
		hovered = null;
	}

	const TOOLTIP_WIDTH = 352;
	const TOOLTIP_HEIGHT = 220;
	function clampX(x: number): number {
		if (typeof window === 'undefined') return x + 14;
		return Math.min(x + 14, window.innerWidth - TOOLTIP_WIDTH);
	}
	function clampY(y: number): number {
		if (typeof window === 'undefined') return y + 14;
		return Math.min(y + 14, window.innerHeight - TOOLTIP_HEIGHT);
	}

	// On load (and whenever the data changes), scroll so today is in view;
	// fall back to the range start when today is outside the chart.
	let scrollEl = $state<HTMLDivElement>();
	$effect(() => {
		const element = scrollEl;
		if (element === undefined) return;
		const target = todayOffset ?? 0;
		element.scrollLeft = Math.max(0, target - 80);
	});
</script>

{#if resolved && axis}
	<div
		class="gantt-scroll"
		bind:this={scrollEl}
		bind:clientWidth={availableWidth}
		role="region"
		aria-label="Gantt chart"
		style="--label-w: {LABEL_WIDTH}px; --chart-w: {axis.chartWidth}px; --header-h: {HEADER_HEIGHT}px; --row-h: {ROW_HEIGHT}px; --bar-h: {BAR_HEIGHT}px; --band-h: {BAND_HEIGHT}px;"
	>
		<div class="gantt-grid">
			<!-- Sticky date header: period bands on top, ticks below. -->
			<div class="time-header">
				<div class="corner"></div>
				<div class="axis">
					<div class="period-row" style="height: {PERIOD_ROW_HEIGHT}px;">
						{#each axis.periods as period (period.label)}
							<div class="period" style="left: {period.x}px; width: {period.width}px;">
								<span>{period.label}</span>
							</div>
						{/each}
					</div>
					<div class="tick-row" style="height: {TICK_ROW_HEIGHT}px;">
						{#each axis.ticks as tick (tick.x)}
							<div class="tick" style="left: {tick.x}px;">
								<span>{tick.label}</span>
							</div>
						{/each}
					</div>
				</div>
			</div>

			<!-- Body: gridlines + today behind/over the bars, then rows. -->
			<div class="body">
				<div class="grid-overlay" aria-hidden="true">
					{#each axis.periods as period (period.label)}
						<div class="gridline" style="left: {period.x}px;"></div>
					{/each}
				</div>
				{#if todayOffset !== null}
					<div class="today-line" style="left: {todayOffset}px;" aria-hidden="true"></div>
				{/if}

				{#each sections as section (section.label ?? ' ')}
					{#if section.label !== null}
						<div class="band" style="height: {BAND_HEIGHT}px;">
							<div class="band-label">{section.label}</div>
						</div>
					{/if}
					{#each section.bars as bar (bar.card.id)}
						<div class="row" style="height: {ROW_HEIGHT}px;">
							<a class="row-label" href={itemHref(bar.card.id)} title={barLabel(bar)}
								>{barLabel(bar)}</a
							>
							<div class="track">
								<div
									class="bar"
									style:--item-color={bar.card.background}
									style="left: {barGeom(bar).left}px; width: {barGeom(bar).width}px;"
									role="img"
									aria-label={barDescription(bar)}
									title={barDescription(bar)}
									onmouseenter={(event) => {
										showCard(event, bar.card);
									}}
									onmousemove={moveCard}
									onmouseleave={hideCard}
								></div>
							</div>
						</div>
					{/each}
				{/each}
			</div>
		</div>
	</div>

	{#if hovered}
		<div class="tooltip" style="left: {clampX(hovered.x)}px; top: {clampY(hovered.y)}px;">
			<Card card={hovered.card} />
		</div>
	{/if}
{/if}

<style>
	.gantt-scroll {
		overflow: auto;
		flex: 1;
		min-height: 0;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background-color: var(--color-bg);
	}

	.gantt-grid {
		/* As wide as label + timeline, but never narrower than the
		   viewport so the header band fills the width. */
		width: max(100%, calc(var(--label-w) + var(--chart-w)));
		position: relative;
	}

	/* ── Header ───────────────────────────────────────────────── */

	.time-header {
		display: flex;
		position: sticky;
		top: 0;
		z-index: 5;
		height: var(--header-h);
		background-color: var(--color-surface);
		border-bottom: 1px solid var(--color-border);
	}

	.corner {
		flex: none;
		width: var(--label-w);
		position: sticky;
		left: 0;
		z-index: 6;
		background-color: var(--color-surface);
		border-right: 1px solid var(--color-border);
	}

	.axis {
		flex: none;
		width: var(--chart-w);
		position: relative;
	}

	.period-row,
	.tick-row {
		position: relative;
	}

	.tick-row {
		border-top: 1px solid var(--color-border);
	}

	.period,
	.tick {
		position: absolute;
		top: 0;
		bottom: 0;
		display: flex;
		align-items: center;
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
		white-space: nowrap;
		overflow: hidden;
	}

	.period {
		padding-left: var(--space-2);
		border-left: 1px solid var(--color-border);
		font-weight: 600;
	}

	.tick {
		padding-left: 4px;
		border-left: 1px solid var(--color-border-subtle, var(--color-border));
	}

	/* ── Body ─────────────────────────────────────────────────── */

	.body {
		position: relative;
	}

	.grid-overlay {
		position: absolute;
		top: 0;
		bottom: 0;
		left: var(--label-w);
		width: var(--chart-w);
		z-index: 0;
		pointer-events: none;
	}

	.gridline {
		position: absolute;
		top: 0;
		bottom: 0;
		width: 1px;
		background-color: var(--color-border);
		opacity: 0.5;
	}

	.today-line {
		position: absolute;
		top: 0;
		bottom: 0;
		width: 2px;
		margin-left: var(--label-w);
		z-index: 2;
		background-color: var(--color-accent);
		pointer-events: none;
	}

	.band {
		display: flex;
		align-items: center;
		background-color: var(--color-surface);
		border-top: 1px solid var(--color-border);
		border-bottom: 1px solid var(--color-border);
	}

	.band-label {
		position: sticky;
		left: 0;
		z-index: 3;
		width: var(--label-w);
		padding: 0 var(--space-3);
		font-size: var(--text-sm);
		font-weight: 600;
		color: var(--color-fg);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.row {
		display: flex;
		border-bottom: 1px solid var(--color-border);
	}

	.row-label {
		flex: none;
		width: var(--label-w);
		position: sticky;
		left: 0;
		z-index: 3;
		display: flex;
		align-items: center;
		padding: 0 var(--space-3);
		background-color: var(--color-bg);
		border-right: 1px solid var(--color-border);
		font-size: var(--text-sm);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		color: inherit;
		text-decoration: none;
	}

	.row-label:hover {
		text-decoration: underline;
	}

	.row-label:focus-visible {
		outline: 2px solid var(--color-accent);
		outline-offset: -2px;
	}

	.track {
		flex: none;
		width: var(--chart-w);
		position: relative;
	}

	.bar {
		position: absolute;
		top: calc((var(--row-h) - var(--bar-h)) / 2);
		height: var(--bar-h);
		z-index: 1;
		/* The bar is the item's body on this view, so it carries the
		   item's `color` field directly (absolute, same in both themes);
		   uncolored items keep the accent as the neutral default bar. */
		background-color: var(--item-color, var(--color-accent));
		border-radius: var(--radius-sm, 3px);
		cursor: default;
	}

	.bar:hover {
		filter: brightness(1.1);
	}

	.tooltip {
		position: fixed;
		max-width: 22rem;
		pointer-events: none;
		z-index: 50;
	}
</style>
