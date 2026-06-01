<!--
  Treemap view. Hierarchical area-proportional chart: every item is a
  rectangle whose area is proportional to a numeric field, items nest
  inside their parents via the configured link field, and parents form
  labeled bordered frames around their children. Multi-level chains
  (task → epic → milestone) show as nested frames, deepest level
  carrying the leaf rectangles.

  Layout uses d3-hierarchy's `treemap` with the squarify tiling — the
  same algorithm Plot.treemap wraps internally. We drive d3-hierarchy
  directly rather than going through Plot because Plot.treemap renders
  leaves only and this view's intent is to show labeled frames at
  every depth too; layering Plot marks for that would be heavier than
  rolling the SVG ourselves.

  Color is intentionally not data-encoded: area carries the only
  numeric dimension, so a sequential color scale would be redundant.
  Leaves get `var(--color-accent)` solid, internal frames are
  transparent with a thin border. CSS variables cascade naturally into
  SVG via class-based `fill`/`stroke` rules, so no theme-flip glue is
  needed (unlike the canvas-based graph view).

  Hover reuses the rich board <Card> via TreemapItemTooltip, which
  prepends a size row. The v1 chart family deferred drill-down
  everywhere except treemap, where the rectangle-to-item mapping is
  trivially 1:1 and the Card is already on the wire.
-->
<script lang="ts">
	import { hierarchy, treemap, treemapSquarify } from 'd3-hierarchy';
	import type { TreemapData } from '$lib/api/generated/TreemapData';
	import type { TreemapNode } from '$lib/api/generated/TreemapNode';
	import type { SizeValue } from '$lib/api/generated/SizeValue';
	import type { Card as CardData } from '$lib/api/generated/Card';
	import { formatDurationSeconds, formatNumber } from '$lib/views/format';
	import { prettifyId } from '$lib/views/prettify';
	import UnplacedFooter from '$lib/views/UnplacedFooter.svelte';
	import TreemapItemTooltip from './TreemapItemTooltip.svelte';

	interface Props {
		data: TreemapData;
	}

	let { data }: Props = $props();

	const CHART_HEIGHT = 600;
	// Strip height reserved at the top of every internal frame for its
	// label; matches the `paddingTop` passed to the layout.
	const FRAME_LABEL_STRIP = 22;
	const FRAME_INNER_GAP = 2;
	// Below these thresholds a rectangle is too small to legibly carry a
	// label — we keep the rect but skip the text rather than overflow.
	const MIN_LEAF_LABEL_WIDTH = 36;
	const MIN_LEAF_LABEL_HEIGHT = 16;
	const MIN_FRAME_LABEL_WIDTH = 28;

	let container = $state<HTMLDivElement>();
	let availableWidth = $state(0);
	let hovered = $state<{ card: CardData; size: SizeValue; x: number; y: number } | null>(null);

	function sizeAsNumber(value: SizeValue): number {
		return value.value;
	}

	function formatSize(value: SizeValue): string {
		if (value.type === 'duration') return formatDurationSeconds(value.value);
		return formatNumber(value.value);
	}

	function nodeLabel(node: TreemapNode): string {
		if (node.card === null) return '';
		return node.card.title ?? prettifyId(node.card.id);
	}

	const sizeLabel = $derived(prettifyId(data.size_field));

	interface LaidNode {
		key: string;
		node: TreemapNode;
		isLeaf: boolean;
		x0: number;
		y0: number;
		width: number;
		height: number;
	}

	// d3-hierarchy.sum() invokes the accessor on every node (including
	// internal ones); we want leaf sizes to roll up via the layout's
	// own summation, so return 0 for internal nodes. This matches the
	// server's already-computed sums without us needing to transport
	// them — d3 re-derives the same totals from the leaves.
	function leafValue(node: TreemapNode): number {
		return node.children.length === 0 ? sizeAsNumber(node.size) : 0;
	}

	const laidNodes = $derived.by((): LaidNode[] => {
		if (availableWidth === 0) return [];
		const root = hierarchy<TreemapNode>(data.root, (n) => n.children)
			.sum(leafValue)
			.sort((a, b) => (b.value ?? 0) - (a.value ?? 0));
		const laid = treemap<TreemapNode>()
			.size([availableWidth, CHART_HEIGHT])
			.tile(treemapSquarify)
			.paddingTop(FRAME_LABEL_STRIP)
			.paddingInner(FRAME_INNER_GAP)(root);

		const out: LaidNode[] = [];
		let internalCounter = 0;
		laid.each((descendant) => {
			// Skip the synthetic top-level root (depth 0, no card).
			if (descendant.depth === 0) return;
			const data = descendant.data;
			const isLeaf = data.children.length === 0;
			out.push({
				key: data.card?.id ?? `__frame_${(internalCounter++).toString()}`,
				node: data,
				isLeaf,
				x0: descendant.x0,
				y0: descendant.y0,
				width: Math.max(0, descendant.x1 - descendant.x0),
				height: Math.max(0, descendant.y1 - descendant.y0)
			});
		});
		return out;
	});

	const leafCount = $derived(laidNodes.filter((laid) => laid.isLeaf).length);
	const itemCountLabel = $derived(leafCount === 1 ? '1 item' : `${leafCount.toString()} items`);

	function onMove(event: MouseEvent, laid: LaidNode): void {
		if (!laid.isLeaf || laid.node.card === null) return;
		const host = container;
		if (host === undefined) return;
		const rect = host.getBoundingClientRect();
		hovered = {
			card: laid.node.card,
			size: laid.node.size,
			x: event.clientX - rect.left,
			y: event.clientY - rect.top
		};
	}

	function onLeave(): void {
		hovered = null;
	}
</script>

{#if data.root.children.length === 0}
	<p class="empty-hint">No items to display.</p>
{:else}
	<div
		class="treemap-wrap"
		bind:this={container}
		bind:clientWidth={availableWidth}
		role="region"
		aria-label="Treemap view"
		onmouseleave={onLeave}
	>
		{#if availableWidth > 0}
			<svg
				class="treemap"
				width={availableWidth}
				height={CHART_HEIGHT}
				viewBox="0 0 {availableWidth} {CHART_HEIGHT}"
				role="presentation"
			>
				{#each laidNodes as laid (laid.key)}
					{#if laid.width > 0 && laid.height > 0}
						{@const clipId = `treemap-clip-${laid.key}`}
						<g transform="translate({laid.x0}, {laid.y0})">
							<clipPath id={clipId}>
								<rect width={laid.width} height={laid.height} />
							</clipPath>
							{#if laid.isLeaf}
								<rect
									class="leaf"
									role="img"
									aria-label={nodeLabel(laid.node)}
									width={laid.width}
									height={laid.height}
									onmousemove={(event) => {
										onMove(event, laid);
									}}
								/>
								{#if laid.width >= MIN_LEAF_LABEL_WIDTH && laid.height >= MIN_LEAF_LABEL_HEIGHT}
									<text
										class="leaf-label"
										x={laid.width / 2}
										y={laid.height / 2}
										text-anchor="middle"
										dominant-baseline="middle"
										clip-path="url(#{clipId})">{nodeLabel(laid.node)}</text
									>
								{/if}
							{:else}
								<rect class="frame" width={laid.width} height={laid.height} />
								{#if laid.width >= MIN_FRAME_LABEL_WIDTH && laid.height >= FRAME_LABEL_STRIP}
									<text class="frame-label" x={6} y={14} clip-path="url(#{clipId})"
										>{nodeLabel(laid.node)}</text
									>
								{/if}
							{/if}
						</g>
					{/if}
				{/each}
			</svg>
		{/if}
		{#if hovered}
			<div class="tooltip" style:left="{hovered.x}px" style:top="{hovered.y}px">
				<TreemapItemTooltip
					card={hovered.card}
					{sizeLabel}
					sizeFormatted={formatSize(hovered.size)}
				/>
			</div>
		{/if}
	</div>
	<p class="row-count">{itemCountLabel}</p>
{/if}

<UnplacedFooter unplaced={data.unplaced} />

<style>
	.treemap-wrap {
		position: relative;
		width: 100%;
	}

	.treemap {
		display: block;
		width: 100%;
		font-family: var(--font-sans);
	}

	.leaf {
		fill: var(--color-accent);
		stroke: var(--color-border);
		stroke-width: 1;
		cursor: default;
	}

	.frame {
		fill: transparent;
		stroke: var(--color-border);
		stroke-width: 1;
		pointer-events: none;
	}

	.frame-label {
		fill: var(--color-fg-muted);
		font-size: 11px;
		font-weight: 600;
		pointer-events: none;
	}

	.leaf-label {
		fill: var(--color-bg);
		font-size: 11px;
		pointer-events: none;
	}

	.tooltip {
		position: absolute;
		transform: translate(12px, 12px);
		max-width: 22rem;
		pointer-events: none;
		z-index: 5;
		background-color: var(--color-bg);
		border-radius: var(--radius-md);
		box-shadow: var(--shadow-sm);
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
