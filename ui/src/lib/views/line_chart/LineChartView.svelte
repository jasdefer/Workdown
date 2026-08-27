<!--
  Line chart view. The extractor ships points already partitioned into
  series and ordered: each point carries an item id, an ChartValue x and
  a SizeValue y, and each series carries the group value its points
  share (null for the synthetic no-value series). The wire also ships an
  `items` sidecar resolving each point's id to its title (via the view's
  `title:` slot, Table pattern) so hover tooltips can show the item by
  name rather than raw id.

  Two visual modes share the same code path:
    - Single-series (group_field is null) — one accent-colored line +
      points; no legend.
    - Grouped (group_field is set) — one line per series with Plot's
      categorical color scale; legend rendered above the chart. The
      no-value series is named here by `noValueLabel`, never by the
      extractor.

  Plot wants a flat array with a `z` channel rather than nested series,
  so `plotPoints` flattens them back out and `seriesLabels` is handed to
  Plot as the color domain — keeping the palette order the extractor's,
  the same order the Markdown renderer walks.

  Plot's `dot` mark holds the hover behavior (each point is hoverable);
  the `line` mark connects them. When grouped, `z: groupKey` separates
  the lines so they don't join across series. The tip shows the item
  title plus the formatted x/y values.
-->
<script lang="ts">
	import type { LineChartData } from '$lib/api/generated/LineChartData';
	import type { LinePoint } from '$lib/api/generated/LinePoint';
	import type { ChartValue } from '$lib/api/generated/ChartValue';
	import type { WorkItemId } from '$lib/api/generated/WorkItemId';
	import { formatDurationSeconds, formatIsoDate, formatNumber } from '$lib/views/format';
	import { mountPlot, PLOT_STYLE } from '$lib/views/plot';
	import { itemRefLabel, noValueLabel, prettifyId } from '$lib/views/prettify';
	import EmptyHint from '$lib/views/EmptyHint.svelte';
	import RowCount from '$lib/views/RowCount.svelte';
	import UnplacedFooter from '$lib/views/UnplacedFooter.svelte';

	/** A wire point flattened out of its series, carrying that series'
	 * display label so Plot can use it as the `z` / color channel. */
	type PlottedPoint = LinePoint & { series: string };

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

	// Core decides which points form which series and in what order.
	// Plot wants one flat array with a `z` channel, so flatten it back
	// out and hand Plot the received order as the color domain — that
	// way both front ends walk their palettes in the same sequence.
	const seriesLabels = $derived(
		data.series.map(
			(series) => series.group ?? (data.group_field !== null ? noValueLabel(data.group_field) : '')
		)
	);
	const plotPoints = $derived(
		data.series.flatMap((series, index) =>
			series.points.map((point) => ({ ...point, series: seriesLabels[index] }))
		)
	);
	const pointCount = $derived(plotPoints.length);

	function axisAsNumber(value: ChartValue): number {
		if (value.type === 'date') return new Date(value.value).getTime();
		return value.value;
	}

	function titleFor(id: WorkItemId): string {
		return itemRefLabel(data.items, id);
	}

	$effect(() => {
		const host = container;
		if (host === undefined || plotPoints.length === 0 || availableWidth === 0) return;

		const xType: ChartValue['type'] | undefined = plotPoints[0]?.x.type;
		const yType = plotPoints[0]?.y.type;
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

		const groupKey = (p: PlottedPoint): string => p.series;
		const colorChannel: ((p: PlottedPoint) => string) | string = isGrouped
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
					...(isGrouped
						? { color: { legend: true, label: groupLabel, domain: seriesLabels } }
						: {}),
					marks: [
						Plot.line(plotPoints, {
							x: (p: PlottedPoint) => axisAsNumber(p.x),
							y: (p: PlottedPoint) => p.y.value,
							stroke: colorChannel,
							strokeWidth: 1.5,
							...(isGrouped ? { z: groupKey } : {})
						}),
						Plot.dot(plotPoints, {
							x: (p: PlottedPoint) => axisAsNumber(p.x),
							y: (p: PlottedPoint) => p.y.value,
							fill: colorChannel,
							stroke: colorChannel,
							r: 4,
							channels: { item: (p: PlottedPoint): string => titleFor(p.id) },
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

{#if plotPoints.length === 0}
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
