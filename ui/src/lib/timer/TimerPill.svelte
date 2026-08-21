<!--
  The header pill — the timer's reachable-from-anywhere face. Appears
  whenever the timer is not idle, and shows per phase (see the
  pomodoro-timer decisions): a stopwatch session is the recording dot
  and the elapsed time; a pomodoro work interval is the dot and the
  countdown, going negative in overrun; a break is the hollow ring, the
  word "Break" and its countdown. In overrun the clock text turns amber
  in both counted phases — the whole glanceability story for now.

  Expanding opens the full controls. Work: the item's title linking to
  it, the wall-clock start time, the big figure (elapsed on the
  stopwatch, remaining in pomodoro — with the measured time beneath,
  because measured time is what gets recorded), the projected write
  naming the field, and stop — which says "Stop → break" when that is
  what it does. Break: a heading saying so, the previous item named and
  linked, the countdown, and the two exits — the next interval on that
  item, or End. Nothing more: no roll-up reminder (the confirmation at
  start was the decision point) and no further item fields (the title
  link is the door to those).

  The expanded panel is the one instance of the timer controls — the
  item slot's "this item being timed" state opens it here rather than
  showing a second copy.
-->
<script lang="ts">
	import { timerStore } from '$lib/stores/timer.svelte';
	import BreakRing from '$lib/timer/BreakRing.svelte';
	import RecordingDot from '$lib/timer/RecordingDot.svelte';
	import {
		formatClock,
		formatCountdown,
		projectedNewSeconds,
		roundedWriteSeconds
	} from '$lib/timer/timerMath';
	import { formatDurationSeconds } from '$lib/views/format';
	import { prettifyId } from '$lib/views/prettify';

	let container = $state<HTMLElement>();
	let breakError = $state<string | null>(null);

	const phase = $derived(timerStore.state?.phase ?? null);
	// The running work interval and the running break — at most one is set.
	const running = $derived(phase?.phase === 'work' ? phase : null);
	const breakPhase = $derived(phase?.phase === 'break' ? phase : null);
	const elapsed = $derived(timerStore.elapsedSeconds ?? 0);
	const field = $derived(
		timerStore.state !== null && timerStore.state.effort_field.state === 'ready'
			? timerStore.state.effort_field.field
			: null
	);
	const rounded = $derived(roundedWriteSeconds(elapsed));
	// The signed countdown of whichever phase has a target; `null` on
	// the stopwatch, which counts toward nothing.
	const remaining = $derived.by(() => {
		if (running !== null && running.phase_length_seconds !== null) {
			return running.phase_length_seconds - elapsed;
		}
		if (breakPhase !== null) {
			return breakPhase.phase_length_seconds - elapsed;
		}
		return null;
	});
	const overrun = $derived(remaining !== null && remaining < 0);

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

	async function nextInterval(item: string): Promise<void> {
		breakError = null;
		const result = await timerStore.start(item, 'pomodoro');
		// The server auto-confirms the item the break followed, so a
		// confirmation request can only mean the break ended elsewhere
		// in the meantime; both leftovers read as the message.
		if (result === 'needs_confirmation') {
			breakError = 'The break already ended — start the item from its own page.';
		} else if (typeof result === 'object') {
			breakError = result.error;
		}
	}
</script>

{#if phase !== null && phase.phase !== 'idle'}
	<div class="timer" bind:this={container}>
		<button
			type="button"
			class="pill"
			aria-expanded={timerStore.panelOpen}
			onclick={() => (timerStore.panelOpen = !timerStore.panelOpen)}
		>
			{#if breakPhase !== null}
				<BreakRing />
				<span class="break-word">Break</span>
			{:else}
				<RecordingDot />
			{/if}
			<span class="clock" class:overrun>
				{remaining !== null ? formatCountdown(remaining) : formatClock(elapsed)}
			</span>
			<span class="chevron" aria-hidden="true">▾</span>
		</button>

		{#if timerStore.panelOpen && running !== null}
			<div class="panel">
				<a
					class="item"
					href={`/items/${encodeURIComponent(running.item_id)}`}
					onclick={() => (timerStore.panelOpen = false)}
				>
					{prettifyId(running.item_id)}
				</a>
				<p class="meta">Started at {startedAtLabel(running.started_at_ms)}</p>
				<p class="figure clock" class:overrun>
					{remaining !== null ? formatCountdown(remaining) : formatClock(elapsed)}
				</p>
				{#if remaining !== null}
					<p class="meta">Measured <span class="clock">{formatClock(elapsed)}</span></p>
				{/if}
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
					class="action"
					disabled={timerStore.busy}
					onclick={() => void timerStore.stop()}
				>
					{remaining !== null ? '■ Stop → break' : '■ Stop'}
				</button>
			</div>
		{:else if timerStore.panelOpen && breakPhase !== null}
			{@const followedItem = breakPhase.followed_item}
			<div class="panel">
				<p class="heading">Break</p>
				<a
					class="item"
					href={`/items/${encodeURIComponent(followedItem)}`}
					onclick={() => (timerStore.panelOpen = false)}
				>
					{prettifyId(followedItem)}
				</a>
				<p class="figure clock" class:overrun>
					{formatCountdown(breakPhase.phase_length_seconds - elapsed)}
				</p>
				<div class="break-actions">
					<button
						type="button"
						class="action"
						disabled={timerStore.busy}
						onclick={() => void nextInterval(followedItem)}
					>
						▶ Next interval
					</button>
					<button
						type="button"
						class="action"
						disabled={timerStore.busy}
						onclick={() => void timerStore.endBreak()}
					>
						End
					</button>
				</div>
				{#if breakError !== null}
					<p class="error" role="alert">{breakError}</p>
				{/if}
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

	.overrun {
		color: var(--color-warning-fg);
	}

	.break-word {
		color: var(--color-fg-muted);
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

	.heading {
		margin: 0;
		font-weight: 600;
		color: var(--color-fg-muted);
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

	.figure {
		margin: 0;
		font-size: var(--text-lg);
		font-weight: 600;
	}

	.break-actions {
		display: flex;
		gap: var(--space-2);
	}

	.action {
		align-self: flex-start;
		font-size: var(--text-sm);
		background: var(--color-card);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		color: var(--color-fg);
		padding: 0.3rem 0.7rem;
		cursor: pointer;
	}

	.action:hover {
		background: var(--color-surface);
	}

	.action:disabled {
		opacity: 0.5;
		cursor: default;
	}

	.error {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--color-error-fg);
	}
</style>
