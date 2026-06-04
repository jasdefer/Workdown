// Shared scaffolding for the Observable-Plot chart views (bar / line /
// workload / heatmap). Each view owns its own `Plot.plot(...)` spec and
// sizing; this module only carries the bits that have no reason to differ
// per view: the theme bridge and the async mount lifecycle.

import type * as Plot from '@observablehq/plot';

// The `style` block passed to every `Plot.plot(...)` call. Plot renders into
// an SVG we mount in a Svelte-controlled <div>, so CSS variables cascade in
// naturally — this just points Plot's text color/font at our tokens. A view
// that ever needs to override still owns its own Plot.plot call.
export const PLOT_STYLE = {
	color: 'var(--color-fg-muted)',
	fontFamily: 'var(--font-sans)',
	background: 'transparent',
	fontSize: '12px'
};

/**
 * Mount an Observable Plot chart into `host`, lazy-loading the (~30–50kb gz)
 * Plot bundle so it stays out of the main chunk — same pattern as Cytoscape
 * in the graph view. Returns a cleanup function for the caller's `$effect`;
 * the cancel guard drops a chart whose import resolves after teardown (or
 * after a width change re-ran the effect) so it never lands in a stale host.
 *
 * The caller keeps its own `$effect` and guard, so which reactive values
 * trigger a re-render is unchanged — this only owns the import/mount dance.
 */
export function mountPlot(
	host: HTMLElement,
	build: (plot: typeof Plot) => SVGSVGElement | HTMLElement,
	label: string
): () => void {
	let cancelled = false;

	const render = async (): Promise<void> => {
		const plot = await import('@observablehq/plot');
		if (cancelled) return;
		host.replaceChildren(build(plot));
	};

	render().catch((error: unknown) => {
		console.error(`Failed to render ${label}`, error);
	});

	return () => {
		cancelled = true;
		host.replaceChildren();
	};
}
