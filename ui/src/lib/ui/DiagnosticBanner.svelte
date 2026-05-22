<!--
  Diagnostic banner — mounted above the rendered view by the route
  page. Groups by item id; primary section holds diagnostics whose
  referenced item is in the current view (plus view-config issues for
  the *current* view, which can't render and so live primary). Other
  diagnostics fall into the collapsed secondary section.

  Hidden entirely when there are no diagnostics. No click behaviour
  in slice 2 — item navigation lands with `mutations-slice`.
-->
<script lang="ts">
	import type { Diagnostic } from '$lib/api/generated/Diagnostic';
	import type { ViewData } from '$lib/api/generated/ViewData';
	import type { WorkItemId } from '$lib/api/generated/WorkItemId';
	import { idsInDiagnostic } from '$lib/diagnostics/idsInDiagnostic';
	import { idsInView } from '$lib/diagnostics/idsInView';
	import { formatDiagnostic } from '$lib/diagnostics/formatDiagnostic';

	interface Props {
		diagnostics: Diagnostic[];
		viewData?: ViewData | undefined;
		currentViewId?: string | undefined;
	}

	let { diagnostics, viewData, currentViewId }: Props = $props();

	interface ItemGroup {
		kind: 'item';
		itemId: WorkItemId;
		diagnostics: Diagnostic[];
	}
	interface SyntheticGroup {
		kind: 'synthetic';
		label: string;
		diagnostics: Diagnostic[];
	}
	type Group = ItemGroup | SyntheticGroup;

	let secondaryOpen = $state(false);

	const inViewIds = $derived(viewData ? idsInView(viewData) : new Set<WorkItemId>());

	const partitioned = $derived(partition(diagnostics, inViewIds, currentViewId));

	function isPrimary(
		diagnostic: Diagnostic,
		inView: Set<WorkItemId>,
		currentView: string | undefined
	): boolean {
		// Rule 1: any referenced item id is in the current view.
		const ids = idsInDiagnostic(diagnostic);
		for (const id of ids) {
			if (inView.has(id)) {
				return true;
			}
		}
		// Rule 2: view-config diagnostic for the current view.
		if (diagnostic.scope === 'config' && currentView !== undefined) {
			const viewId = (diagnostic as Record<string, unknown>).view_id;
			if (typeof viewId === 'string' && viewId === currentView) {
				return true;
			}
		}
		return false;
	}

	function groupDiagnostics(list: Diagnostic[]): Group[] {
		const byItem = new Map<WorkItemId, Diagnostic[]>();
		const cycles: Diagnostic[] = [];
		const duplicates: Diagnostic[] = [];
		const viewConfigs: Diagnostic[] = [];
		const other: Diagnostic[] = [];

		for (const diagnostic of list) {
			if (diagnostic.scope === 'item') {
				const itemId = diagnostic.item_id;
				const bucket = byItem.get(itemId) ?? [];
				bucket.push(diagnostic);
				byItem.set(itemId, bucket);
				continue;
			}
			if (diagnostic.scope === 'collection' && diagnostic.type === 'cycle') {
				cycles.push(diagnostic);
				continue;
			}
			if (diagnostic.scope === 'files') {
				// FilesDiagnostic only has the `duplicate_id` variant today.
				duplicates.push(diagnostic);
				continue;
			}
			if (diagnostic.scope === 'config') {
				viewConfigs.push(diagnostic);
				continue;
			}
			other.push(diagnostic);
		}

		const groups: Group[] = [];

		const itemIds = [...byItem.keys()].sort();
		for (const itemId of itemIds) {
			const bucket = byItem.get(itemId);
			if (bucket) {
				groups.push({ kind: 'item', itemId, diagnostics: sortBySeverity(bucket) });
			}
		}

		const syntheticBuckets: { label: string; bucket: Diagnostic[] }[] = [
			{ label: 'Cycles', bucket: cycles },
			{ label: 'Duplicates', bucket: duplicates },
			{ label: 'Views', bucket: viewConfigs },
			{ label: 'Other', bucket: other }
		];
		for (const { label, bucket } of syntheticBuckets) {
			if (bucket.length > 0) {
				groups.push({ kind: 'synthetic', label, diagnostics: sortBySeverity(bucket) });
			}
		}
		return groups;
	}

	function partition(
		list: Diagnostic[],
		inView: Set<WorkItemId>,
		currentView: string | undefined
	): { primary: Group[]; secondary: Group[]; secondaryCount: number } {
		const primaryRaw: Diagnostic[] = [];
		const secondaryRaw: Diagnostic[] = [];
		for (const diagnostic of list) {
			if (isPrimary(diagnostic, inView, currentView)) {
				primaryRaw.push(diagnostic);
			} else {
				secondaryRaw.push(diagnostic);
			}
		}
		return {
			primary: groupDiagnostics(primaryRaw),
			secondary: groupDiagnostics(secondaryRaw),
			secondaryCount: secondaryRaw.length
		};
	}

	function sortBySeverity(list: Diagnostic[]): Diagnostic[] {
		return [...list].sort((a, b) => {
			if (a.severity === b.severity) return 0;
			return a.severity === 'error' ? -1 : 1;
		});
	}

	function groupKey(group: Group): string {
		return group.kind === 'item' ? `item:${group.itemId}` : `synthetic:${group.label}`;
	}
</script>

{#if diagnostics.length > 0}
	<aside class="banner" aria-label="Project diagnostics">
		{#if partitioned.primary.length > 0}
			<section class="zone primary">
				<h2 class="zone-heading">This view</h2>
				{#each partitioned.primary as group (groupKey(group))}
					<div class="group">
						<div class="group-label">
							{group.kind === 'item' ? group.itemId : group.label}
						</div>
						<ul class="diagnostic-list">
							{#each group.diagnostics as diagnostic, index (index)}
								<li class="diagnostic" class:error={diagnostic.severity === 'error'}>
									<span class="icon" aria-hidden="true">
										{diagnostic.severity === 'error' ? '✕' : '⚠'}
									</span>
									<span class="message">{formatDiagnostic(diagnostic)}</span>
								</li>
							{/each}
						</ul>
					</div>
				{/each}
			</section>
		{/if}

		{#if partitioned.secondary.length > 0}
			<section class="zone secondary">
				<button
					type="button"
					class="secondary-toggle"
					aria-expanded={secondaryOpen}
					onclick={() => (secondaryOpen = !secondaryOpen)}
				>
					<span class="chevron" class:open={secondaryOpen} aria-hidden="true">▸</span>
					Other diagnostics ({partitioned.secondaryCount})
				</button>
				{#if secondaryOpen}
					<div class="secondary-body">
						{#each partitioned.secondary as group (groupKey(group))}
							<div class="group">
								<div class="group-label">
									{group.kind === 'item' ? group.itemId : group.label}
								</div>
								<ul class="diagnostic-list">
									{#each group.diagnostics as diagnostic, index (index)}
										<li class="diagnostic" class:error={diagnostic.severity === 'error'}>
											<span class="icon" aria-hidden="true">
												{diagnostic.severity === 'error' ? '✕' : '⚠'}
											</span>
											<span class="message">{formatDiagnostic(diagnostic)}</span>
										</li>
									{/each}
								</ul>
							</div>
						{/each}
					</div>
				{/if}
			</section>
		{/if}
	</aside>
{/if}

<style>
	.banner {
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		padding: var(--space-3);
		font-size: var(--text-sm);
		background-color: var(--color-surface);
	}

	.zone + .zone {
		margin-top: var(--space-3);
		padding-top: var(--space-3);
		border-top: 1px solid var(--color-border);
	}

	.zone-heading {
		margin: 0 0 var(--space-2);
		font-size: var(--text-sm);
		font-weight: 600;
		color: var(--color-fg);
	}

	.group + .group {
		margin-top: var(--space-2);
	}

	.group-label {
		font-family: var(--font-mono);
		color: var(--color-fg-muted);
		font-size: 0.85em;
		margin-bottom: 0.25rem;
	}

	.diagnostic-list {
		list-style: none;
		margin: 0;
		padding: 0 0 0 var(--space-3);
	}

	.diagnostic {
		display: flex;
		gap: var(--space-2);
		padding: 0.15rem 0;
		color: var(--color-warning-fg);
	}

	.diagnostic.error {
		color: var(--color-error-fg);
	}

	.icon {
		font-weight: 700;
		flex-shrink: 0;
	}

	.message {
		min-width: 0;
		overflow-wrap: anywhere;
	}

	.zone.secondary {
		color: var(--color-fg-muted);
	}

	.secondary-toggle {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		background: none;
		border: none;
		color: inherit;
		font-size: inherit;
		padding: 0;
		cursor: pointer;
	}

	.secondary-toggle:hover {
		color: var(--color-fg);
	}

	.chevron {
		display: inline-block;
		transition: transform 0.1s ease;
	}

	.chevron.open {
		transform: rotate(90deg);
	}

	.secondary-body {
		margin-top: var(--space-2);
	}
</style>
