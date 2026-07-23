<!--
  Graph view. Renders GraphData — one node per filtered item, one
  directed edge per outgoing link on the view's `field` — as an
  interactive Cytoscape graph. When the view sets `group_by`, the
  `groups` tree maps to Cytoscape compound (parent) nodes, so members
  nest inside a labelled box; otherwise the graph is flat. `nodes` and
  `groups` cover the same item set (see GraphData), so the hover lookup
  is built from `nodes` in both layouts.

  Layout uses dagre (via cytoscape-dagre) — the same layered-DAG
  algorithm Mermaid's `flowchart TD` runs — laid out top-down. It
  handles both flat and compound (grouped) graphs; the built-in
  breadthfirst/cose layouts ordered nodes noticeably worse.

  Read-only canvas: wheel zooms, dragging the background pans (there are
  no scrollbars), node dragging is off (`autoungrabify`). Hovering a
  node shows its full card (title, id, rendered body) by reusing the
  board <Card>. Click-to-open-item is deferred until the real item page
  lands — added across all views then.

  Cytoscape draws to <canvas>, so CSS custom properties don't cascade
  into it: colors are read from the resolved design tokens at build
  time and re-applied whenever the theme flips. Cytoscape is imported
  dynamically so it only loads on pages that actually show a graph.
-->
<script module lang="ts">
	// Cytoscape extensions register globally on the factory. Guard so the
	// per-data-change build effect doesn't re-register (and warn) on every
	// rebuild — registration only needs to happen once per page load.
	let dagreRegistered = false;
</script>

<script lang="ts">
	import type cytoscape from 'cytoscape';
	import type { GraphData } from '$lib/api/generated/GraphData';
	import type { Card as CardData } from '$lib/api/generated/Card';
	import type { TreeNode } from '$lib/api/generated/TreeNode';
	import { textColorOn } from '$lib/views/colorContrast';
	import { cardLabel } from '$lib/views/prettify';
	import { themeStore, type Theme } from '$lib/stores/theme.svelte';
	import EmptyHint from '$lib/views/EmptyHint.svelte';
	import RowCount from '$lib/views/RowCount.svelte';
	import Card from '$lib/views/board/Card.svelte';

	interface Props {
		data: GraphData;
	}

	let { data }: Props = $props();

	let container = $state<HTMLDivElement>();
	let cy = $state<cytoscape.Core>();
	let hovered = $state<{ card: CardData; x: number; y: number } | null>(null);

	const nodeCount = $derived(data.nodes.length);
	const cardById = $derived(new Map(data.nodes.map((card): [string, CardData] => [card.id, card])));

	// The item's `color` field rides along as node data (canvas can't
	// read CSS variables): the resolved fill plus its black/white label
	// color, both precomputed so the stylesheet only maps `data(...)`.
	function colorData(card: CardData, nodeData: cytoscape.NodeDataDefinition): void {
		if (card.background === null) return;
		nodeData.itemColor = card.background;
		nodeData.itemTextColor = textColorOn(card.background);
	}

	// Flat: one node per card. Grouped: walk `groups`, tagging each node
	// with its `parent` so Cytoscape nests it. Edges are identical either
	// way.
	function buildElements(graph: GraphData): cytoscape.ElementDefinition[] {
		const elements: cytoscape.ElementDefinition[] = [];
		if (graph.groups) {
			const walk = (nodes: TreeNode[], parent: string | undefined): void => {
				for (const node of nodes) {
					const nodeData: cytoscape.NodeDataDefinition = {
						id: node.card.id,
						label: cardLabel(node.card)
					};
					if (parent !== undefined) nodeData.parent = parent;
					colorData(node.card, nodeData);
					elements.push({ data: nodeData });
					if (node.children.length > 0) walk(node.children, node.card.id);
				}
			};
			walk(graph.groups.roots, undefined);
		} else {
			for (const card of graph.nodes) {
				const nodeData: cytoscape.NodeDataDefinition = { id: card.id, label: cardLabel(card) };
				colorData(card, nodeData);
				elements.push({ data: nodeData });
			}
		}
		for (const edge of graph.edges) {
			elements.push({
				data: { id: `${edge.from}->${edge.to}`, source: edge.from, target: edge.to }
			});
		}
		return elements;
	}

	// Dagre — the layered-DAG algorithm Mermaid's `flowchart TD` uses —
	// orders nodes far better than the built-ins, for both flat and
	// compound graphs. The `as unknown` hop is because @types/cytoscape-
	// dagre registers the extension but doesn't widen cytoscape's
	// LayoutOptions union to admit `name: 'dagre'`.
	function dagreLayout(): cytoscape.LayoutOptions {
		return {
			name: 'dagre',
			rankDir: 'TB',
			nodeSep: 36,
			rankSep: 48,
			padding: 24,
			fit: true
		} as unknown as cytoscape.LayoutOptions;
	}

	function token(name: string): string {
		return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
	}

	// Token-driven stylesheet. `theme` selects the compound-box tint (the
	// surface fill reads differently against light vs dark) and makes the
	// theme a real input, so the restyle effect tracks it. The array's
	// wrapper type is left inferred (its name varies across
	// @types/cytoscape versions); each entry's `style` is pinned to
	// Css.Node / Css.Edge below, so call sites stay type-safe.
	function buildStyle(theme: Theme) {
		const fg = token('--color-fg');
		const muted = token('--color-fg-muted');
		const border = token('--color-border');
		const bg = token('--color-bg');
		const surface = token('--color-surface');
		const fontSans = token('--font-sans');

		const nodeStyle: cytoscape.Css.Node = {
			'background-color': bg,
			'border-color': border,
			'border-width': 1,
			shape: 'round-rectangle',
			label: 'data(label)',
			color: fg,
			'font-family': fontSans,
			'font-size': 12,
			'text-valign': 'center',
			'text-halign': 'center',
			'text-wrap': 'wrap',
			'text-max-width': '160px',
			width: 'label',
			height: 'label',
			padding: '10px'
		};
		const parentStyle: cytoscape.Css.Node = {
			'background-color': surface,
			'background-opacity': theme === 'dark' ? 0.4 : 0.7,
			'border-color': border,
			color: muted,
			'text-valign': 'top',
			'text-halign': 'center',
			padding: '16px'
		};
		// Colored items: the node is a chart mark with its label inside
		// (the treemap-leaf situation), so it fills with the item color —
		// absolute across themes — and the label flips black/white
		// (precomputed into node data). Colored *group* boxes keep their
		// translucent surface fill and show the color as their border,
		// mirroring the treemap's frame treatment; listed after `:parent`
		// so the border override wins while the surface fill stays.
		const coloredNodeStyle: cytoscape.Css.Node = {
			'background-color': 'data(itemColor)',
			color: 'data(itemTextColor)'
		};
		const coloredParentStyle: cytoscape.Css.Node = {
			'background-color': surface,
			'border-color': 'data(itemColor)',
			color: muted
		};
		const edgeStyle: cytoscape.Css.Edge = {
			width: 1.5,
			'line-color': muted,
			'target-arrow-color': muted,
			'target-arrow-shape': 'triangle',
			'curve-style': 'bezier'
		};
		return [
			{ selector: 'node', style: nodeStyle },
			{ selector: 'node[itemColor]', style: coloredNodeStyle },
			{ selector: ':parent', style: parentStyle },
			{ selector: ':parent[itemColor]', style: coloredParentStyle },
			{ selector: 'edge', style: edgeStyle }
		];
	}

	// Build / rebuild when `data` (or the container) changes. The theme is
	// read after the dynamic import, so this effect does not re-run on
	// theme flips — the next effect handles repainting.
	$effect(() => {
		const elements = buildElements(data);
		const layout = dagreLayout();
		const host = container;
		if (host === undefined || elements.length === 0) return;

		let destroyed = false;
		let instance: cytoscape.Core | undefined;

		const build = async (): Promise<void> => {
			const factory = (await import('cytoscape')).default;
			if (!dagreRegistered) {
				const dagreExtension = (await import('cytoscape-dagre')).default;
				factory.use(dagreExtension);
				dagreRegistered = true;
			}
			if (destroyed) return;
			instance = factory({
				container: host,
				elements,
				style: buildStyle(themeStore.value),
				layout,
				autoungrabify: true,
				boxSelectionEnabled: false,
				minZoom: 0.2,
				maxZoom: 2.5
			});
			instance.on('mouseover', 'node', (event: cytoscape.EventObject) => {
				const node = event.target as cytoscape.NodeSingular;
				const card = cardById.get(node.id());
				if (card === undefined) return;
				const position = node.renderedPosition();
				hovered = { card, x: position.x, y: position.y };
			});
			instance.on('mouseout', 'node', () => {
				hovered = null;
			});
			instance.on('pan zoom', () => {
				hovered = null;
			});
			cy = instance;
		};
		build().catch((error: unknown) => {
			console.error('Failed to render graph view', error);
		});

		return () => {
			destroyed = true;
			instance?.destroy();
			cy = undefined;
			hovered = null;
		};
	});

	// Repaint on theme flip — Cytoscape can't inherit the CSS-var change.
	$effect(() => {
		cy?.style(buildStyle(themeStore.value));
	});
</script>

{#if nodeCount === 0}
	<EmptyHint />
{:else}
	<div class="graph-wrap">
		<div class="graph" bind:this={container} role="region" aria-label="Graph view"></div>
		{#if hovered}
			<div class="tooltip" style="left: {hovered.x}px; top: {hovered.y}px;">
				<Card card={hovered.card} />
			</div>
		{/if}
	</div>
{/if}

<RowCount count={nodeCount} />

<style>
	.graph-wrap {
		position: relative;
		flex: 1;
		min-height: 0;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background-color: var(--color-bg);
		overflow: hidden;
	}

	.graph {
		position: absolute;
		inset: 0;
		cursor: grab;
	}

	.graph:active {
		cursor: grabbing;
	}

	.tooltip {
		position: absolute;
		transform: translate(12px, 12px);
		max-width: 22rem;
		pointer-events: none;
		z-index: 5;
	}
</style>
