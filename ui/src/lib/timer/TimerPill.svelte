<!--
  The header pill — the timer's reachable-from-anywhere face. Appears
  when a timer starts: the elapsed time, nothing else, plus the
  affordance to expand. Expanding opens the full controls: the item's
  title linking to it, the wall-clock start time, the elapsed time,
  stop, and the projected write — naming the field it writes to and
  moving once a minute as the rounding dictates; under half a minute it
  says instead that stop writes nothing. Nothing more: no roll-up
  reminder (the confirmation at start was the decision point) and no
  further item fields (the title link is the door to those).

  The expanded panel is the one instance of the timer controls — the
  item slot's "this item being timed" state opens it here rather than
  showing a second copy.
-->
<script lang="ts">
	import { timerStore } from '$lib/stores/timer.svelte';
	import RecordingDot from '$lib/timer/RecordingDot.svelte';
	import { formatClock, projectedNewSeconds, roundedWriteSeconds } from '$lib/timer/timerMath';
	import { formatDurationSeconds } from '$lib/views/format';
	import { prettifyId } from '$lib/views/prettify';

	let container = $state<HTMLElement>();

	const running = $derived(timerStore.state?.running ?? null);
	const elapsed = $derived(timerStore.elapsedSeconds ?? 0);
	const field = $derived(
		timerStore.state !== null && timerStore.state.effort_field.state === 'ready'
			? timerStore.state.effort_field.field
			: null
	);
	const rounded = $derived(roundedWriteSeconds(elapsed));

	// Click outside or Escape closes the expanded panel.
	$effect(() => {
		if (!timerStore.panelOpen) return undefined;
		const onClick = (event: MouseEvent) => {
			if (container !== undefined && !container.contains(event.target as Node)) {
				timerStore.panelOpen = false;
			}
		};
		const onKeydown = (event: KeyboardEvent) => {
			if (event.key === 'Escape') timerStore.panelOpen = false;
		};
		document.addEventListener('click', onClick);
		document.addEventListener('keydown', onKeydown);
		return () => {
			document.removeEventListener('click', onClick);
			document.removeEventListener('keydown', onKeydown);
		};
	});

	function startedAtLabel(startedAtMs: number): string {
		return new Date(startedAtMs).toLocaleTimeString([], {
			hour: '2-digit',
			minute: '2-digit'
		});
	}
</script>

{#if running !== null}
	<div class="timer" bind:this={container}>
		<button
			type="button"
			class="pill"
			aria-expanded={timerStore.panelOpen}
			onclick={() => (timerStore.panelOpen = !timerStore.panelOpen)}
		>
			<RecordingDot />
			<span class="clock">{formatClock(elapsed)}</span>
			<span class="chevron" aria-hidden="true">▾</span>
		</button>

		{#if timerStore.panelOpen}
			<div class="panel">
				<a
					class="item"
					href={`/items/${encodeURIComponent(running.item_id)}`}
					onclick={() => (timerStore.panelOpen = false)}
				>
					{prettifyId(running.item_id)}
				</a>
				<p class="meta">Started at {startedAtLabel(running.started_at_ms)}</p>
				<p class="elapsed clock">{formatClock(elapsed)}</p>
				{#if field !== null}
					{#if rounded === 0}
						<p class="projection">Stop writes nothing — under half a minute.</p>
					{:else}
						<p class="projection">
							{field}: {running.effort_before_seconds === null
								? 'none'
								: formatDurationSeconds(running.effort_before_seconds)}
							→ {formatDurationSeconds(projectedNewSeconds(running.effort_before_seconds, elapsed))} on
							stop
						</p>
					{/if}
				{/if}
				<button
					type="button"
					class="stop"
					disabled={timerStore.busy}
					onclick={() => void timerStore.stop()}
				>
					■ Stop
				</button>
			</div>
		{/if}
	</div>
{/if}

<style>
	.timer {
		position: relative;
	}

	.pill {
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		font-size: var(--text-sm);
		background: var(--color-card);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-full);
		color: var(--color-fg);
		padding: 0.25rem 0.7rem;
		cursor: pointer;
	}

	.clock {
		font-variant-numeric: tabular-nums;
	}

	.chevron {
		color: var(--color-fg-muted);
		font-size: 0.8em;
	}

	.panel {
		position: absolute;
		top: calc(100% + 0.4rem);
		right: 0;
		z-index: 20;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		min-width: 16rem;
		background: var(--color-card);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		box-shadow: var(--shadow-sm);
		padding: var(--space-3);
	}

	.item {
		font-weight: 600;
		color: var(--color-fg);
		text-decoration: none;
	}

	.item:hover {
		text-decoration: underline;
	}

	.meta,
	.projection {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}

	.elapsed {
		margin: 0;
		font-size: var(--text-lg);
		font-weight: 600;
	}

	.stop {
		align-self: flex-start;
		font-size: var(--text-sm);
		background: var(--color-card);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		color: var(--color-fg);
		padding: 0.3rem 0.7rem;
		cursor: pointer;
	}

	.stop:hover {
		background: var(--color-surface);
	}

	.stop:disabled {
		opacity: 0.5;
		cursor: default;
	}
</style>
