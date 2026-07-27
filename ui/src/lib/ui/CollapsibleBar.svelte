<!--
  Shared chrome for the session-local bars above a view (Filter,
  Display, and any future one): a bordered container whose collapsed
  header row carries the chevron toggle, a label, a count badge, and —
  when the bar has something active — a status pill plus action
  buttons. The expanded panel slides down and renders the caller's
  controls; the caller owns all behaviour and panel layout, this
  component owns only the shell.
-->
<script module lang="ts">
	/** One header action button (e.g. Save, Reset, Clear). */
	export interface BarAction {
		label: string;
		onclick: () => void;
		/** Accent-filled emphasis (e.g. Save); default is a neutral button. */
		primary?: boolean;
		disabled?: boolean;
	}
</script>

<script lang="ts">
	import type { Snippet } from 'svelte';
	import { slide } from 'svelte/transition';

	interface Props {
		label: string;
		/** Badge next to the label; 0 hides it. */
		count: number;
		/** Status pill text; null hides the pill and the actions. */
		status: string | null;
		/** Action buttons shown next to the status pill. */
		actions?: BarAction[];
		/** Bindable so callers can expand programmatically (e.g. when a
		    shared ?filter= link opens the page with a draft active). */
		expanded?: boolean;
		children: Snippet;
	}

	let {
		label,
		count,
		status,
		actions = [],
		expanded = $bindable(false),
		children
	}: Props = $props();
</script>

<div class="bar">
	<div class="header">
		<button
			type="button"
			class="toggle"
			aria-expanded={expanded}
			onclick={() => (expanded = !expanded)}
		>
			<span class="chevron" class:open={expanded}>▸</span>
			{label}
			{#if count > 0}<span class="count">{count}</span>{/if}
		</button>

		{#if status !== null}
			<span class="status" in:slide={{ axis: 'x' }}>{status}</span>
			{#each actions as action, index (index)}
				<button
					type="button"
					class="action"
					class:primary={action.primary}
					disabled={action.disabled}
					onclick={action.onclick}
				>
					{action.label}
				</button>
			{/each}
		{/if}
	</div>

	{#if expanded}
		<div class="panel" transition:slide>
			{@render children()}
		</div>
	{/if}
</div>

<style>
	.bar {
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background-color: var(--color-surface);
	}

	.header {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-2) var(--space-3);
	}

	.toggle {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		background: none;
		border: none;
		color: var(--color-fg);
		cursor: pointer;
		font-size: var(--text-sm);
		font-weight: 600;
		padding: 0;
	}

	.chevron {
		display: inline-block;
		transition: transform 0.15s ease;
		color: var(--color-fg-muted);
	}

	.chevron.open {
		transform: rotate(90deg);
	}

	.count {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		min-width: 1.25rem;
		height: 1.25rem;
		padding: 0 0.35rem;
		border-radius: var(--radius-full);
		background-color: var(--color-accent);
		color: var(--color-accent-fg);
		font-size: var(--text-sm);
		font-weight: 600;
	}

	.status {
		margin-left: auto;
		color: var(--color-warning-fg);
		background-color: var(--color-warning-bg);
		padding: 0.1rem var(--space-2);
		border-radius: var(--radius-full);
		font-size: var(--text-sm);
	}

	.action {
		background-color: var(--color-bg);
		color: var(--color-fg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		padding: 0.25rem var(--space-2);
		font-size: var(--text-sm);
		cursor: pointer;
	}

	.action:hover:not(:disabled) {
		border-color: var(--color-accent);
	}

	.action:disabled {
		opacity: 0.6;
		cursor: default;
	}

	.action.primary {
		background-color: var(--color-accent);
		color: var(--color-accent-fg);
		border-color: var(--color-accent);
	}

	.panel {
		padding: var(--space-3);
		border-top: 1px solid var(--color-border);
	}
</style>
