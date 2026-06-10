<!--
  Small line-art glyph hinting a view's kind, shown as a prefix on each
  navigation link. The title/id is always the primary label — this is a
  secondary cue only (a project can have several views of the same kind),
  so the glyphs stay rough and uniform rather than precise.

  Hand-rolled inline SVG with `currentColor`, matching the `ThemeToggle`
  convention — no icon-library dependency. The three gantt variants share
  one glyph; their distinction lives in the title, not the icon.
-->
<script lang="ts">
	import type { ViewType } from '$lib/api/generated/ViewType';

	interface Props {
		kind: ViewType;
		size?: number;
	}

	let { kind, size = 16 }: Props = $props();
</script>

<svg
	viewBox="0 0 24 24"
	width={size}
	height={size}
	fill="none"
	stroke="currentColor"
	stroke-width="2"
	stroke-linecap="round"
	stroke-linejoin="round"
	aria-hidden="true"
>
	{#if kind === 'board'}
		<!-- Three columns -->
		<rect x="3" y="4" width="5" height="16" rx="1" />
		<rect x="9.5" y="4" width="5" height="11" rx="1" />
		<rect x="16" y="4" width="5" height="14" rx="1" />
	{:else if kind === 'tree'}
		<!-- Root with two children -->
		<circle cx="12" cy="5" r="2" />
		<circle cx="6" cy="19" r="2" />
		<circle cx="18" cy="19" r="2" />
		<path d="M12 7 V11 M6 17 V13 H18 V17" />
	{:else if kind === 'graph'}
		<!-- Connected nodes -->
		<circle cx="5" cy="6" r="2" />
		<circle cx="19" cy="9" r="2" />
		<circle cx="10" cy="18" r="2" />
		<path d="M6.8 7 L17.2 8 M6.3 7.7 L8.7 16.3 M11.7 16.6 L17.4 10.3" />
	{:else if kind === 'table'}
		<!-- Grid -->
		<rect x="3" y="4" width="18" height="16" rx="1" />
		<path d="M3 9.5 H21 M3 15 H21 M9 4 V20 M15 4 V20" />
	{:else if kind === 'gantt' || kind === 'gantt_by_initiative' || kind === 'gantt_by_depth'}
		<!-- Staggered horizontal bars -->
		<rect x="3" y="4" width="10" height="3.5" rx="1" />
		<rect x="7" y="10" width="11" height="3.5" rx="1" />
		<rect x="11" y="16" width="9" height="3.5" rx="1" />
	{:else if kind === 'bar_chart'}
		<!-- Vertical bars on a baseline -->
		<path d="M3 20 H21" />
		<rect x="4.5" y="11" width="3.5" height="8" rx="0.5" />
		<rect x="10.25" y="6" width="3.5" height="13" rx="0.5" />
		<rect x="16" y="14" width="3.5" height="5" rx="0.5" />
	{:else if kind === 'line_chart'}
		<!-- Axis + trend line -->
		<path d="M4 4 V20 H20" />
		<path d="M6 16 L11 11 L14 14 L19 6" />
	{:else if kind === 'workload'}
		<!-- Left-aligned load bars of differing length -->
		<path d="M4 6 H14 M4 12 H19 M4 18 H10" stroke-width="2.5" />
	{:else if kind === 'metric'}
		<!-- Gauge with a needle -->
		<path d="M5 17 A8 8 0 0 1 19 17" />
		<path d="M12 17 L15.5 11" />
	{:else if kind === 'treemap'}
		<!-- Box partitioned into uneven blocks -->
		<rect x="3" y="4" width="18" height="16" rx="1" />
		<path d="M12 4 V20 M12 12 H21 M3 13 H12" />
	{:else if kind === 'heatmap'}
		<!-- Cell grid with a couple filled cells -->
		<rect x="3" y="4" width="18" height="16" rx="1" />
		<path d="M9 4 V20 M15 4 V20 M3 9.3 H21 M3 14.6 H21" />
		<rect x="3" y="4" width="6" height="5.3" fill="currentColor" stroke="none" opacity="0.35" />
		<rect x="15" y="14.6" width="6" height="5.4" fill="currentColor" stroke="none" opacity="0.35" />
	{/if}
</svg>
