<!--
  The picture on the error page, by status. A 404 shows a stack of work
  item cards whose top card is only an outline with a question mark —
  the item that is not there. Drawn from `currentColor` and the theme
  tokens, so it follows light and dark mode; `aria-hidden` because the
  page's heading already says what happened. Other statuses have no
  picture yet and render nothing.
-->
<script lang="ts">
	interface Props {
		/** The HTTP status the page shows. (Not `status`: the linter reads
		 * that name in the template as the deprecated `window.status`.) */
		httpStatus: number;
	}

	let { httpStatus }: Props = $props();
</script>

{#if httpStatus === 404}
	<svg class="art" viewBox="0 0 240 180" width="240" height="180" aria-hidden="true">
		<!-- Two cards that are there: filled, resting at the bottom of the stack. -->
		<g class="card">
			<rect x="52" y="112" width="136" height="44" rx="8" />
			<rect class="stripe" x="52" y="112" width="5" height="44" rx="2" />
			<rect class="line" x="70" y="125" width="70" height="6" rx="3" />
			<rect class="line faint" x="70" y="139" width="44" height="6" rx="3" />
		</g>
		<g class="card">
			<rect x="52" y="64" width="136" height="44" rx="8" />
			<rect class="stripe" x="52" y="64" width="5" height="44" rx="2" />
			<rect class="line" x="70" y="77" width="84" height="6" rx="3" />
			<rect class="line faint" x="70" y="91" width="56" height="6" rx="3" />
		</g>
		<!-- The card that is not: a tilted outline, lifting away. -->
		<g class="missing" transform="rotate(-8 120 38)">
			<rect x="52" y="16" width="136" height="44" rx="8" />
			<text x="120" y="47" text-anchor="middle">?</text>
		</g>
		<!-- Motion marks, so the outline reads as drifting off. -->
		<g class="marks">
			<path d="M30 30 l-10 -6" />
			<path d="M26 44 l-12 0" />
			<path d="M210 22 l10 -8" />
		</g>
	</svg>
{/if}

<style>
	.art {
		display: block;
		max-width: 100%;
		height: auto;
		color: var(--color-fg-muted);
	}

	.card rect {
		fill: var(--color-card);
		stroke: var(--color-border);
		stroke-width: 1.5;
	}

	.card .stripe {
		fill: var(--color-accent);
		stroke: none;
	}

	.card .line {
		fill: currentColor;
		stroke: none;
		opacity: 0.6;
	}

	.card .line.faint {
		opacity: 0.3;
	}

	.missing rect {
		fill: none;
		stroke: var(--color-accent);
		stroke-width: 2;
		stroke-dasharray: 7 5;
	}

	.missing text {
		fill: var(--color-accent);
		font-family: var(--font-mono);
		font-size: 24px;
		font-weight: 600;
	}

	.marks path {
		stroke: currentColor;
		stroke-width: 2;
		stroke-linecap: round;
		opacity: 0.5;
	}
</style>
