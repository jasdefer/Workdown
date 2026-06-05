<!--
  Line chart view. Points are pre-extracted server-side: each point
  carries an item id, an AxisValue x, a SizeValue y, and an optional
  group string. The wire also ships an `items` sidecar resolving each
  point's id to its title (via the view's `title:` slot, Table pattern)
  so hover tooltips can show the item by name rather than raw id.

  Two visual modes share the same code path:
    - Single-series (group_field is null) — one accent-colored line +
      points; no legend.
    - Grouped (group_field is set) — one line per group with Plot's
      categorical color scale; legend rendered above the chart.

  Plot's `dot` mark holds the hover behavior (each point is hoverable);
  the `line` mark connects them. When grouped, `z: groupKey` separates
  the lines so they don't join across series. The tip shows the item
  title plus the formatted x/y values.
-->
<script lang="ts">
	import type { LineChartData } from '$lib/api/generated/LineChartData';
	import type { LinePoint } from '$lib/api/generated/LinePoint';
	import type { AxisValue } from '$lib/api/generated/AxisValue';
	import type { WorkItemId } from '$lib/api/generated/WorkItemId';
	import { formatDurationSeconds, formatIsoDate, formatNumber } from '$lib/views/format';
	import { mountPlot, PLOT_STYLE } from '$lib/views/plot';
	import { prettifyId } from '$lib/views/prettify';
	import EmptyHint from '$lib/views/EmptyHint.svelte';
	import RowCount from '$lib/views/RowCount.svelte';
	import UnplacedFooter from '$lib/views/UnplacedFooter.svelte';

	interface Props {
		data: LineChartData;
	}

	let { data }: Props = $props();

	let container = $state<HTMLDivElement>();
	// Plot defaults to 640px wide; bind clientWidth to fill the parent
	// instead. The effect re-runs on width change so the chart relays
	// out (rather than CSS-scaling a 640px SVG, which would shrink the
	// text along with it).
	let availableWidth = $state(0);
	const CHART_HEIGHT = 400;

	const pointCount = $derived(data.points.length);

	function axisAsNumber(value: AxisValue): number {
		if (value.type === 'date') return new Date(value.value).getTime();
		return value.value;
	}

	function titleFor(id: WorkItemId): string {
		return data.items[id]?.title ?? prettifyId(id);
	}

	$effect(() => {
		const host = container;
		if (host === undefined || data.points.length === 0 || availableWidth === 0) return;

		const xType: AxisValue['type'] | undefined = data.points[0]?.x.type;
		const yType = data.points[0]?.y.type;
		const isGrouped = data.group_field !== null;
		const groupLabel = data.group_field !== null ? prettifyId(data.group_field) : '';

		const formatXTick = (n: number): string => {
			if (xType === 'duration') return formatDurationSeconds(n);
			if (xType === 'date') {
				return formatIsoDate(new Date(n));
			}
			return formatNumber(n);
		};

		const formatYTick = (n: number): string => {
			if (yType === 'duration') return formatDurationSeconds(n);
			return formatNumber(n);
		};

		const groupKey = (p: LinePoint): string => p.group ?? '(none)';
		const colorChannel: ((p: LinePoint) => string) | string = isGrouped
			? groupKey
			: 'var(--color-accent)';

		return mountPlot(
			host,
			(Plot) =>
				Plot.plot({
					width: availableWidth,
					height: CHART_HEIGHT,
					marginBottom: 90,
					marginLeft: 80,
					style: PLOT_STYLE,
					x: {
						label: prettifyId(data.x_field),
						tickFormat: formatXTick,
						tickRotate: -35,
						tickSpacing: 80,
						type: xType === 'date' ? 'time' : 'linear'
					},
					y: {
						label: prettifyId(data.y_field),
						grid: true,
						tickFormat: formatYTick,
						type: 'linear',
						zero: false
					},
					...(isGrouped ? { color: { legend: true, label: groupLabel } } : {}),
					marks: [
						Plot.line(data.points, {
							x: (p: LinePoint) => axisAsNumber(p.x),
							y: (p: LinePoint) => p.y.value,
							stroke: colorChannel,
							strokeWidth: 1.5,
							...(isGrouped ? { z: groupKey } : {})
						}),
						Plot.dot(data.points, {
							x: (p: LinePoint) => axisAsNumber(p.x),
							y: (p: LinePoint) => p.y.value,
							fill: colorChannel,
							stroke: colorChannel,
							r: 4,
							channels: { item: (p: LinePoint): string => titleFor(p.id) },
							tip: {
								format: {
									x: formatXTick,
									y: formatYTick,
									item: true,
									fill: isGrouped,
									stroke: false,
									r: false
								}
							}
						})
					]
				}),
			'line chart view'
		);
	});
</script>

{#if data.points.length === 0}
	<EmptyHint />
{:else}
	<div
		class="chart"
		bind:this={container}
		bind:clientWidth={availableWidth}
		role="region"
		aria-label="Line chart view"
	></div>
{/if}

<RowCount count={pointCount} noun="point" />

<UnplacedFooter unplaced={data.unplaced} />

<style>
	.chart {
		width: 100%;
		color: var(--color-fg-muted);
		font-family: var(--font-sans);
	}

	.chart :global(svg) {
		display: block;
		overflow: visible;
	}
</style>
