<!--
  Footer listing items that matched the view's filter but couldn't be
  turned into a bar (missing dates, invalid range, unresolved predecessor,
  …). Mirrors the Markdown renderer's "items dropped" block: grouped by
  reason, item names in the order they arrive (the wire sorts unplaced by
  id). Renders nothing when there's nothing dropped.

  Gantt is the first browser view with unplaced items; the chart family
  will want the same treatment, so this is written to lift to a shared
  location unchanged.
-->
<script lang="ts">
	import type { UnplacedCard } from '$lib/api/generated/UnplacedCard';
	import type { UnplacedReason } from '$lib/api/generated/UnplacedReason';
	import { prettifyId } from '$lib/views/prettify';

	interface Props {
		unplaced: UnplacedCard[];
	}

	let { unplaced }: Props = $props();

	function itemName(card: UnplacedCard['card']): string {
		return card.title ?? prettifyId(card.id);
	}

	// A short human description of why an item was dropped. The set covers
	// every UnplacedReason variant, though only a few can occur for gantt.
	function reasonLabel(reason: UnplacedReason): string {
		switch (reason.type) {
			case 'missing_value':
				return `missing ${prettifyId(reason.field)}`;
			case 'invalid_range':
				return `invalid range (${prettifyId(reason.start_field)} → ${prettifyId(reason.end_field)})`;
			case 'no_working_days':
				return `no working days (${prettifyId(reason.start_field)} → ${prettifyId(reason.end_field)})`;
			case 'non_numeric_value':
				return `non-numeric ${prettifyId(reason.field)}`;
			case 'no_anchor':
				return 'no start or predecessor to anchor to';
			case 'predecessor_unresolved':
				return `unresolved predecessor ${reason.id}`;
			case 'cycle':
				return `dependency cycle via ${prettifyId(reason.via)}`;
		}
	}

	interface ReasonGroup {
		label: string;
		names: string[];
	}

	// Group by reason description, preserving first-seen order.
	const groups = $derived.by<ReasonGroup[]>(() => {
		const order: string[] = [];
		const byLabel = new Map<string, string[]>();
		for (const card of unplaced) {
			const label = reasonLabel(card.reason);
			let names = byLabel.get(label);
			if (names === undefined) {
				names = [];
				byLabel.set(label, names);
				order.push(label);
			}
			names.push(itemName(card.card));
		}
		return order.map((label) => ({ label, names: byLabel.get(label) ?? [] }));
	});

	const headingLabel = $derived(
		unplaced.length === 1 ? '1 item dropped:' : `${unplaced.length.toString()} items dropped:`
	);
</script>

{#if unplaced.length > 0}
	<div class="unplaced" role="note">
		<p class="unplaced-heading">{headingLabel}</p>
		<ul class="unplaced-list">
			{#each groups as group (group.label)}
				<li>
					<span class="reason">{group.label}:</span>
					{group.names.join(', ')}
				</li>
			{/each}
		</ul>
	</div>
{/if}

<style>
	.unplaced {
		margin: var(--space-3) 0 0;
		padding: var(--space-3);
		border: 1px solid var(--color-border);
		border-left: 3px solid var(--color-warning, var(--color-border));
		border-radius: var(--radius-md);
		background-color: var(--color-surface);
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}

	.unplaced-heading {
		margin: 0 0 var(--space-2);
		font-weight: 600;
		color: var(--color-fg);
	}

	.unplaced-list {
		margin: 0;
		padding-left: var(--space-4);
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	.reason {
		color: var(--color-fg);
	}
</style>
