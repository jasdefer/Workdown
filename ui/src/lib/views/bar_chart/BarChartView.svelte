<!--
  Bar chart view. Bars are pre-aggregated server-side: each carries a
  group key (categorical string) and an AggregateValue (number, date,
  or duration). The y-axis adapts to that variant — linear for number
  and duration, time scale for date. Tick labels run through the same
  shared formatter as the metric tiles so a duration axis reads as
  `1d 6h` rather than raw seconds.

  Observable Plot is dynamically imported so its (~30–50kb gz) bundle
  only loads on chart pages — same pattern as Cytoscape in the graph
  view. Plot returns an SVG element which we mount into a Svelte-
  controlled `<div>`; everything inside is plain SVG, so CSS variables
  cascade in naturally. The `style` option supplies the text color and
  font; bars take `var(--color-accent)` as a fill. No token-into-JS
  bridge needed (unlike Cytoscape on canvas).

  Hover uses Plot's built-in `tip` mark — minimal value popover, the
  v1 floor agreed in remaining-read-views. Drill-down into contributing
  items is deferred (the wire doesn't carry them today).
-->
<script lang="ts">
	import type { BarChartData } from '$lib/api/generated/BarChartData';
	import type { BarChartBar } from '$lib/api/generated/BarChartBar';
	import type { AggregateValue } from '$lib/api/generated/AggregateValue';
	import type { Aggregate } from '$lib/api/generated/Aggregate';
	import { formatAggregateValue, formatIsoDate } from '$lib/views/format';
	import { mountPlot, PLOT_STYLE } from '$lib/views/plot';
	import { prettifyId } from '$lib/views/prettify';
	import UnplacedFooter from '$lib/views/UnplacedFooter.svelte';

	interface Props {
		data: BarChartData;
	}

	let { data }: Props = $props();

	let container = $state<HTMLDivElement>();
	// Plot defaults to 640px wide; bind clientWidth to fill the parent
	// instead. The effect re-runs on width change so the chart relays
	// out (rather than CSS-scaling a 640px SVG, which would shrink the
	// text along with it).
	let availableWidth = $state(0);
	const CHART_HEIGHT = 400;

	const barCount = $derived(data.bars.length);
	const itemCountLabel = $derived(barCount === 1 ? '1 bar' : `${barCount.toString()} bars`);

	function valueAsPlotNumber(value: AggregateValue): number {
		if (value.type === 'date') return new Date(value.value).getTime();
		return value.value;
	}

	function aggregateNoun(aggregate: Aggregate): string {
		switch (aggregate) {
			case 'count':
				return 'count';
			case 'sum':
				return 'sum';
			case 'avg':
				return 'average';
			case 'min':
				return 'min';
			case 'max':
				return 'max';
		}
	}

	function yAxisLabel(): string {
		if (data.aggregate === 'count') return 'items';
		const noun = aggregateNoun(data.aggregate);
		if (data.value_field !== null) {
			return `${noun} of ${prettifyId(data.value_field)}`;
		}
		return noun;
	}

	$effect(() => {
		const host = container;
		if (host === undefined || data.bars.length === 0 || availableWidth === 0) return;

		const valueType: AggregateValue['type'] | undefined = data.bars[0]?.value.type;

		const formatYTick = (n: number): string => {
			if (valueType === 'duration') {
				return formatAggregateValue({ type: 'duration', value: n });
			}
			if (valueType === 'date') {
				return formatIsoDate(new Date(n));
			}
			return formatAggregateValue({ type: 'number', value: n });
		};

		const tipFormatBar = (bar: BarChartBar): string => formatAggregateValue(bar.value);

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
						label: prettifyId(data.group_by),
						tickRotate: -45,
						type: 'band'
					},
					y: {
						label: yAxisLabel(),
						grid: true,
						tickFormat: formatYTick,
						type: valueType === 'date' ? 'time' : 'linear',
						zero: valueType !== 'date'
					},
					marks: [
						Plot.barY(data.bars, {
							x: (bar: BarChartBar): string => bar.group,
							y: (bar: BarChartBar): number => valueAsPlotNumber(bar.value),
							fill: 'var(--color-accent)',
							channels: { exact: { value: tipFormatBar, label: yAxisLabel() } },
							tip: {
								format: {
									x: true,
									y: false,
									exact: true,
									fill: false
								}
							}
						}),
						valueType === 'date' ? null : Plot.ruleY([0])
					]
				}),
			'bar chart view'
		);
	});
</script>

{#if data.bars.length === 0}
	<p class="empty-hint">No items to display.</p>
{:else}
	<div
		class="chart"
		bind:this={container}
		bind:clientWidth={availableWidth}
		role="region"
		aria-label="Bar chart view"
	></div>
	<p class="row-count">{itemCountLabel}</p>
{/if}

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
