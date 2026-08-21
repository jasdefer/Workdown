<!--
  The one symbol for "this item is being timed" — the red record dot the
  header pill carries, reused wherever a view marks the running item in
  place (see the timer-recording-indicator decisions). Surfaces a dot
  can't reach (canvas, SVG) use `--color-recording` as a border/stroke
  instead. No margin of its own; the host supplies spacing.
-->
<span class="dot" title="Being timed"></span>

<style>
	.dot {
		display: inline-block;
		width: 0.5rem;
		height: 0.5rem;
		border-radius: 50%;
		background: var(--color-recording);
		flex-shrink: 0;
	}
</style>
