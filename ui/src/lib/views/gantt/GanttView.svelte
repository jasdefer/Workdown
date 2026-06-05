<!--
  Basic gantt view. Groups the bars into the view's sections (the `group`
  field) and feeds them to the shared <GanttChart>, then renders the
  uniform empty / count / unplaced chrome.

  Bars arrive pre-sorted by (section, start, id) — the missing-group bucket
  last — so grouping by first appearance preserves the server's section
  order. When the view sets no `group` field, every bar lands in one
  label-less section and no band headers render.

  The by-depth / by-initiative variants are separate wrappers (a later
  slice); they reuse <GanttChart> directly with one section per level /
  initiative and a shared range.
-->
<script lang="ts">
	import type { GanttData } from '$lib/api/generated/GanttData';
	import type { GanttBar } from '$lib/api/generated/GanttBar';
	import { prettifyId } from '$lib/views/prettify';
	import GanttChart, { type GanttSection } from './GanttChart.svelte';
	import UnplacedFooter from '$lib/views/UnplacedFooter.svelte';
	import EmptyHint from '$lib/views/EmptyHint.svelte';
	import RowCount from '$lib/views/RowCount.svelte';

	interface Props {
		data: GanttData;
	}

	let { data }: Props = $props();

	const sections = $derived.by<GanttSection[]>(() => {
		const order: (string | null)[] = [];
		const buckets = new Map<string | null, GanttBar[]>();
		for (const bar of data.bars) {
			let bucket = buckets.get(bar.group);
			if (bucket === undefined) {
				bucket = [];
				buckets.set(bar.group, bucket);
				order.push(bar.group);
			}
			bucket.push(bar);
		}
		return order.map((group) => ({
			// No band when the view isn't grouped. Otherwise the group value,
			// or "(no <field>)" for the missing-value bucket — matching the
			// Markdown renderer's convention.
			label: data.group_field === null ? null : (group ?? `(no ${prettifyId(data.group_field)})`),
			bars: buckets.get(group) ?? []
		}));
	});
</script>

{#if data.bars.length === 0}
	{#if data.unplaced.length === 0}
		<EmptyHint />
	{/if}
{:else}
	<GanttChart {sections} />
{/if}

<UnplacedFooter unplaced={data.unplaced} />

<RowCount count={data.bars.length} />
