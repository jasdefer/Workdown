<!--
  The type-specific block for `string`: the regex a value must match.
  Entered as typed into a monospace input; blank means no pattern,
  which removes the key from the file. Not judged here: the pattern is
  a regex in Rust's dialect, and the browser's `RegExp` would accept
  and refuse different things, so the server's parse is the one
  verdict and its message comes back to the panel on save.

  Owns no state: the draft's shape is the value, and a change goes up
  as the whole new shape.
-->
<script lang="ts">
	import { withPattern, type TextShape } from './fieldDraft';

	interface Props {
		shape: TextShape;
		disabled?: boolean;
		onchange: (shape: TextShape) => void;
	}

	let { shape, disabled = false, onchange }: Props = $props();

	function onPatternChange(event: Event & { currentTarget: HTMLInputElement }): void {
		onchange(withPattern(shape, event.currentTarget.value));
	}
</script>

<label class="row">
	<span class="label">Pattern</span>
	<input
		type="text"
		class="pattern"
		value={shape.pattern ?? ''}
		placeholder="a regular expression, e.g. ^[a-z-]+$"
		spellcheck="false"
		{disabled}
		onchange={onPatternChange}
	/>
	<span class="hint">A value must match the whole pattern. Leave empty for none.</span>
</label>

<style>
	.row {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	.label {
		font-size: var(--text-sm);
		font-weight: 600;
		color: var(--color-fg-muted);
	}

	.pattern {
		width: 100%;
		padding: 0.25rem var(--space-2);
		background-color: var(--color-bg);
		color: var(--color-fg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		font-size: var(--text-sm);
		font-family: var(--font-mono);
	}

	.hint {
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}
</style>
