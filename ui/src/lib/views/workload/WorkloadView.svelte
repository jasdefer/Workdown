<!--
  Workload view. Daily effort load across a date range — one bar per
  working day with height = sum of item-effort contributions spread
  across that day. The server-side extractor already partitions each
  item's effort across its working days and sums into buckets, so the
  wire shape is just `{date, total}` per day with a top-level `unit`
  (Number or Duration) describing how to interpret `total`.

  Bars are rendered with `Plot.rectY` at `interval: 'day'`, so each
  rect spans exactly one calendar day on a time scale. Non-working
  days (weekends / holidays per the active working calendar) never
  appear in the wire data — they show up as honest gaps between bars
  rather than zero-height pseudo-bars.

  A dashed vertical rule marks today so a reader can orient between
  past and upcoming load at a glance.

  Hover scope per the chart-family plan is the value-on-pointer floor:
  date + formatted effort total. Drill-down to contributing items is
  deferred — the wire doesn't carry per-bucket item lists today.
-->
<script lang="ts">
	import type { WorkloadData } from '$lib/api/generated/WorkloadData';
	import type { WorkloadBucket } from '$lib/api/generated/WorkloadBucket';
	import { formatDurationSeconds, formatNumber, pickDurationUnit } from '$lib/views/format';
	import { mountPlot, PLOT_STYLE } from '$lib/views/plot';
	import { prettifyId } from '$lib/views/prettify';
	import EmptyHint from '$lib/views/EmptyHint.svelte';
	import RowCount from '$lib/views/RowCount.svelte';
	import UnplacedFooter from '$lib/views/UnplacedFooter.svelte';

	interface Props {
		data: WorkloadData;
	}

	let { data }: Props = $props();

	let container = $state<HTMLDivElement>();
	let availableWidth = $state(0);
	const CHART_HEIGHT = 400;

	const bucketCount = $derived(data.buckets.length);

	function formatTotal(total: number): string {
		if (data.unit === 'duration') return formatDurationSeconds(total);
		return formatNumber(total);
	}

	$effect(() => {
		const host = container;
		if (host === undefined || data.buckets.length === 0 || availableWidth === 0) return;

		// For Duration unit, scale bar heights into a human unit
		// (weeks/days/hours/minutes) so Plot's auto-ticks land on whole
		// numbers — 0, 1, 2 days rather than 0, 50000, 100000 seconds.
		// The tooltip's `load` channel still shows the full compact form
		// off the unscaled total, so readers see e.g. "2d 13h", not "2.5".
		const maxTotal = data.buckets.reduce((max, bucket) => Math.max(max, bucket.total), 0);
		const durationUnit = data.unit === 'duration' ? pickDurationUnit(maxTotal) : null;
		const scaledTotal = (bucket: WorkloadBucket): number =>
			durationUnit === null ? bucket.total : bucket.total / durationUnit.seconds;
		const yAxisLabel =
			durationUnit === null
				? `${prettifyId(data.effort_field)} per day`
				: `${prettifyId(data.effort_field)} (${durationUnit.label})`;

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
						label: prettifyId(data.start_field),
						type: 'time',
						tickRotate: -35,
						tickSpacing: 80
					},
					y: {
						label: yAxisLabel,
						grid: true,
						tickFormat: (n: number): string => formatNumber(n),
						zero: true
					},
					marks: [
						Plot.rectY(data.buckets, {
							x: (bucket: WorkloadBucket): Date => new Date(bucket.date),
							y: scaledTotal,
							interval: 'day',
							fill: 'var(--color-accent)',
							channels: {
								day: { value: (bucket: WorkloadBucket): string => bucket.date },
								load: {
									value: (bucket: WorkloadBucket): string => formatTotal(bucket.total)
								}
							},
							tip: {
								format: {
									x: false,
									x1: false,
									x2: false,
									y: false,
									day: true,
									load: true,
									fill: false
								}
							}
						}),
						Plot.ruleX([new Date()], {
							stroke: 'var(--color-fg-muted)',
							strokeDasharray: '4 2'
						}),
						Plot.ruleY([0])
					]
				}),
			'workload view'
		);
	});
</script>

{#if data.buckets.length === 0}
	<EmptyHint />
{:else}
	<div
		class="chart"
		bind:this={container}
		bind:clientWidth={availableWidth}
		role="region"
		aria-label="Workload view"
	></div>
{/if}

<RowCount count={bucketCount} noun="working day" />

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
