<!--
  The timer's fixed slot on the item editing surface — the one component
  the slide-over panel and the standalone item page share, so both get it
  for free. Sits in the same place on every item regardless of schema,
  which makes it the natural "place the timer would be": with no effort
  field configured, this is where the hint naming `defaults.effort_field`
  appears (only when a duration field exists to point at — a hint no
  existing field could satisfy is not actionable).

  Three states when the field is ready: no timer running — the split
  start button (stopwatch now, pomodoro wired in later, visibly
  disabled); this item being timed — it says so, and clicking opens the
  header pill's expanded panel rather than a second copy of the
  controls; another item being timed — names it and offers to stop that
  timer or open the item instead (switching is stop first, then start —
  no takeover).
-->
<script lang="ts">
	import { schemaStore } from '$lib/stores/schema.svelte';
	import { timerStore } from '$lib/stores/timer.svelte';
	import RecordingDot from '$lib/timer/RecordingDot.svelte';
	import { formatClock } from '$lib/timer/timerMath';
	import ConfirmDialog from '$lib/ui/ConfirmDialog.svelte';
	import { prettifyId } from '$lib/views/prettify';

	interface Props {
		itemId: string;
	}

	let { itemId }: Props = $props();

	let startButton = $state<HTMLButtonElement>();
	let confirming = $state(false);
	let menuOpen = $state(false);
	let error = $state<string | null>(null);

	const effort = $derived(timerStore.state?.effort_field ?? null);
	const running = $derived(timerStore.state?.running ?? null);
	const hasDurationField = $derived(
		schemaStore.fields.some((field) => field.field_type === 'duration')
	);

	$effect(() => {
		void timerStore.load();
		void schemaStore.load();
	});

	// The mode menu closes on any outside click or Escape.
	$effect(() => {
		if (!menuOpen) return undefined;
		const close = () => (menuOpen = false);
		const onKeydown = (event: KeyboardEvent) => {
			if (event.key === 'Escape') close();
		};
		document.addEventListener('click', close);
		document.addEventListener('keydown', onKeydown);
		return () => {
			document.removeEventListener('click', close);
			document.removeEventListener('keydown', onKeydown);
		};
	});

	async function start(confirmed = false): Promise<void> {
		error = null;
		const result = await timerStore.start(itemId, confirmed);
		if (result === 'needs_confirmation') {
			confirming = true;
			return;
		}
		if (typeof result === 'object') {
			error = result.error;
		}
	}
</script>

{#if effort !== null}
	<div class="timer-slot">
		{#if effort.state === 'unconfigured'}
			{#if hasDurationField}
				<p class="hint">
					No effort field configured — set <code>defaults.effort_field</code> in config.yaml and
					restart <code>workdown serve</code> to time work here.
				</p>
			{/if}
		{:else if effort.state === 'invalid'}
			<p class="hint">
				The timer is unavailable: <code>defaults.effort_field</code> — {effort.problem}. Fix
				config.yaml and restart <code>workdown serve</code>.
			</p>
		{:else if running === null}
			<div class="start-group">
				<button
					type="button"
					class="start"
					disabled={timerStore.busy}
					bind:this={startButton}
					onclick={() => void start()}
				>
					▶ Start timer
				</button>
				<button
					type="button"
					class="mode-toggle"
					aria-label="Timer mode"
					aria-expanded={menuOpen}
					onclick={(event) => {
						event.stopPropagation();
						menuOpen = !menuOpen;
					}}
				>
					▾
				</button>
				{#if menuOpen}
					<div class="mode-menu" role="menu">
						<button
							type="button"
							role="menuitem"
							class="mode-item"
							onclick={() => (menuOpen = false)}
						>
							✓ Stopwatch
						</button>
						<button type="button" role="menuitem" class="mode-item" disabled title="Coming soon">
							Pomodoro
						</button>
					</div>
				{/if}
			</div>
		{:else if running.item_id === itemId}
			<button type="button" class="this-item" onclick={() => (timerStore.panelOpen = true)}>
				<RecordingDot />
				Timing this item — {formatClock(timerStore.elapsedSeconds ?? 0)}
			</button>
		{:else}
			<div class="other-item">
				<span class="naming">
					Timing “{prettifyId(running.item_id)}”
				</span>
				<button type="button" disabled={timerStore.busy} onclick={() => void timerStore.stop()}>
					Stop that timer
				</button>
				<a href={`/items/${encodeURIComponent(running.item_id)}`}>Open it</a>
			</div>
		{/if}

		{#if error !== null}
			<p class="error" role="alert">{error}</p>
		{/if}
	</div>
{/if}

{#if confirming && startButton !== undefined && effort?.state === 'ready'}
	<ConfirmDialog
		anchor={startButton}
		title="Timing this item overrides its roll-up"
		body="Its {effort.field} rolls up from its children; recorded time becomes a hand-written value that wins over the roll-up."
		confirmLabel="Start anyway"
		onconfirm={() => {
			confirming = false;
			void start(true);
		}}
		oncancel={() => (confirming = false)}
	/>
{/if}

<style>
	.timer-slot {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	.hint {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}

	.hint code {
		font-family: var(--font-mono);
		font-size: 0.9em;
	}

	.start-group {
		position: relative;
		display: inline-flex;
		align-self: flex-start;
	}

	.start,
	.mode-toggle {
		font-size: var(--text-sm);
		background: var(--color-card);
		border: 1px solid var(--color-border);
		color: var(--color-fg);
		padding: 0.3rem 0.6rem;
		cursor: pointer;
	}

	.start {
		border-radius: var(--radius-sm) 0 0 var(--radius-sm);
	}

	.mode-toggle {
		border-left: none;
		border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
		color: var(--color-fg-muted);
	}

	.start:hover,
	.mode-toggle:hover {
		background: var(--color-surface);
	}

	.start:disabled {
		opacity: 0.5;
		cursor: default;
	}

	.mode-menu {
		position: absolute;
		top: calc(100% + 0.25rem);
		left: 0;
		z-index: 5;
		display: flex;
		flex-direction: column;
		min-width: 10rem;
		background: var(--color-card);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		box-shadow: var(--shadow-sm);
		padding: 0.25rem;
	}

	.mode-item {
		font-size: var(--text-sm);
		background: none;
		border: none;
		color: var(--color-fg);
		text-align: left;
		padding: 0.3rem 0.5rem;
		border-radius: var(--radius-sm);
		cursor: pointer;
	}

	.mode-item:hover:not(:disabled) {
		background: var(--color-surface);
	}

	.mode-item:disabled {
		color: var(--color-fg-muted);
		cursor: default;
	}

	.this-item {
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		align-self: flex-start;
		font-size: var(--text-sm);
		font-variant-numeric: tabular-nums;
		background: var(--color-card);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-full);
		color: var(--color-fg);
		padding: 0.3rem 0.7rem;
		cursor: pointer;
	}

	.other-item {
		display: flex;
		align-items: baseline;
		gap: var(--space-3);
		font-size: var(--text-sm);
	}

	.naming {
		color: var(--color-fg-muted);
	}

	.other-item button {
		background: none;
		border: none;
		padding: 0;
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
		text-decoration: underline;
		cursor: pointer;
	}

	.other-item button:hover,
	.other-item a:hover {
		color: var(--color-fg);
	}

	.other-item a {
		color: var(--color-fg-muted);
	}

	.error {
		margin: 0;
		color: var(--color-error-fg);
		font-size: var(--text-sm);
	}
</style>
