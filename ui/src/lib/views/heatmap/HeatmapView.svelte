<!--
  Heatmap view. A 2D grid: x and y are pre-stringified categorical axes
  (the server has already bucketed dates and formatted other types into
  display strings), each populated cell carries a tagged AggregateValue.
  `x_labels` / `y_labels` give the full axis ordering — empty cells are
  honest gaps, never imaginary zeros.

  Color scale is Plot's built-in `blues` sequential scheme so it works
  in both light and dark themes without the computed-style glue the
  graph view needs. Color legend is rendered above the chart so the
  fill → value map is always visible.

  For Duration-typed values, `pickDurationUnit` chooses weeks/days/etc.
  off the max cell value and rescales the fill domain so legend ticks
  read as whole numbers in that unit ("0 ··· 4 days" rather than "0
  ··· 345600 seconds"). For Date-typed values, the fill domain is
  milliseconds and the legend tick formatter converts back to ISO.
  The cell tooltip always shows the fully-formatted value via a
  parallel `formatted` channel.
-->
<script lang="ts">
	import type { HeatmapData } from '$lib/api/generated/HeatmapData';
	import type { HeatmapCell } from '$lib/api/generated/HeatmapCell';
	import type { AggregateValue } from '$lib/api/generated/AggregateValue';
	import {
		formatAggregateValue,
		formatIsoDate,
		formatNumber,
		pickDurationUnit
	} from '$lib/views/format';
	import { mountPlot, PLOT_STYLE } from '$lib/views/plot';
	import { prettifyId } from '$lib/views/prettify';
	import UnplacedFooter from '$lib/views/UnplacedFooter.svelte';

	interface Props {
		data: HeatmapData;
	}

	let { data }: Props = $props();

	let container = $state<HTMLDivElement>();
	let availableWidth = $state(0);
	// Heatmaps read best with roughly square cells. Compute chart
	// dimensions from the label-count ratio (x : y) so the inner plot
	// area's aspect matches the grid's, capped at the container width
	// and a sensible max height. The chart centers horizontally when
	// it ends up narrower than the container. Tuned generous so the
	// chart still feels substantial on a 4K screen — the cell-count
	// doesn't squeeze it.
	const MAX_HEIGHT = 700;
	const chartSize = $derived.by((): { width: number; height: number } => {
		if (availableWidth === 0) return { width: 0, height: 0 };
		const xCount = Math.max(data.x_labels.length, 1);
		const yCount = Math.max(data.y_labels.length, 1);
		const aspect = xCount / yCount;
		let width = MAX_HEIGHT * aspect;
		let height = MAX_HEIGHT;
		if (width > availableWidth) {
			width = availableWidth;
			height = availableWidth / aspect;
		}
		return { width, height };
	});

	const cellCount = $derived(data.cells.length);
	const itemCountLabel = $derived(cellCount === 1 ? '1 cell' : `${cellCount.toString()} cells`);

	function colorLegendLabel(): string {
		if (data.aggregate === 'count') return 'count';
		if (data.value_field !== null) {
			return `${data.aggregate} of ${prettifyId(data.value_field)}`;
		}
		return data.aggregate;
	}

	$effect(() => {
		const host = container;
		if (host === undefined || data.cells.length === 0 || chartSize.width === 0) return;

		const valueType: AggregateValue['type'] | undefined = data.cells[0]?.value.type;
		const maxDurationSeconds =
			valueType === 'duration'
				? data.cells.reduce(
						(max, cell) => (cell.value.type === 'duration' ? Math.max(max, cell.value.value) : max),
						0
					)
				: 0;
		const durationUnit = valueType === 'duration' ? pickDurationUnit(maxDurationSeconds) : null;

		const valueAsNumber = (cell: HeatmapCell): number => {
			if (cell.value.type === 'duration') {
				return durationUnit === null ? cell.value.value : cell.value.value / durationUnit.seconds;
			}
			if (cell.value.type === 'date') {
				return new Date(cell.value.value).getTime();
			}
			return cell.value.value;
		};

		const legendTickFormat = (n: number): string => {
			if (valueType === 'date') {
				return formatIsoDate(new Date(n));
			}
			return formatNumber(n);
		};

		const legendLabel =
			durationUnit === null ? colorLegendLabel() : `${colorLegendLabel()} (${durationUnit.label})`;

		return mountPlot(
			host,
			(Plot) =>
				Plot.plot({
					width: chartSize.width,
					height: chartSize.height,
					marginBottom: 80,
					marginLeft: 100,
					style: PLOT_STYLE,
					x: {
						label: prettifyId(data.x_field),
						domain: data.x_labels,
						tickRotate: -25,
						type: 'band'
					},
					y: {
						label: prettifyId(data.y_field),
						domain: data.y_labels,
						type: 'band'
					},
					color: {
						scheme: 'blues',
						legend: true,
						label: legendLabel,
						tickFormat: legendTickFormat
					},
					marks: [
						Plot.cell(data.cells, {
							x: 'x',
							y: 'y',
							fill: valueAsNumber,
							channels: {
								formatted: {
									value: (cell: HeatmapCell): string => formatAggregateValue(cell.value)
								}
							},
							tip: {
								format: {
									x: true,
									y: true,
									fill: false,
									formatted: true
								}
							}
						})
					]
				}),
			'heatmap view'
		);
	});
</script>

{#if data.cells.length === 0}
	<p class="empty-hint">No items to display.</p>
{:else}
	<div
		class="chart"
		bind:this={container}
		bind:clientWidth={availableWidth}
		role="region"
		aria-label="Heatmap view"
	></div>
	<p class="row-count">{itemCountLabel}</p>
{/if}

<UnplacedFooter unplaced={data.unplaced} />

<style>
	.chart {
		width: 100%;
		display: flex;
		justify-content: center;
		color: var(--color-fg-muted);
		font-family: var(--font-sans);
	}

	.chart :global(svg) {
		display: block;
		overflow: visible;
	}

	.empty-hint {
		color: var(--color-fg-muted);
		font-size: var(--text-sm);
		margin: 0 0 var(--space-3);
	}

	.row-count {
		margin: var(--space-2) 0 0;
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}
</style>
