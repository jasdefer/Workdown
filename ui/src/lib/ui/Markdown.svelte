<!--
  Render Markdown content safely. Uses `marked` for parsing and
  `DOMPurify` for sanitisation before injecting via `{@html}`. The
  sanitiser is essential even though workdown is a local dev tool —
  body content originates from authored item files, which can in turn
  be edited by collaborators or pulled from branches.

  `compact` mode is for cards and other tight surfaces: tighter
  line-height, collapsed paragraph spacing, smaller headings. Default
  mode is for item detail panels where the body has full vertical room.
-->
<script lang="ts">
	import { marked } from 'marked';
	import DOMPurify from 'dompurify';

	interface Props {
		content: string;
		compact?: boolean;
	}

	let { content, compact = false }: Props = $props();

	const html = $derived.by(() => {
		const raw = marked.parse(content, { async: false });
		return DOMPurify.sanitize(raw);
	});
</script>

<div class="markdown" class:compact>
	<!-- eslint-disable-next-line svelte/no-at-html-tags -- DOMPurify-sanitized above -->
	{@html html}
</div>

<style>
	.markdown {
		color: var(--color-fg);
		line-height: 1.5;
	}

	.markdown :global(p) {
		margin: 0 0 var(--space-3) 0;
	}

	.markdown :global(p:last-child) {
		margin-bottom: 0;
	}

	.markdown :global(h1),
	.markdown :global(h2),
	.markdown :global(h3),
	.markdown :global(h4) {
		margin: var(--space-4) 0 var(--space-2);
		line-height: 1.25;
	}

	.markdown :global(code) {
		font-family: var(--font-mono);
		font-size: 0.9em;
		background-color: var(--color-surface);
		padding: 0.1em 0.3em;
		border-radius: var(--radius-md);
	}

	.markdown :global(pre) {
		font-family: var(--font-mono);
		background-color: var(--color-surface);
		padding: var(--space-3);
		border-radius: var(--radius-md);
		overflow-x: auto;
	}

	.markdown :global(pre code) {
		background: none;
		padding: 0;
	}

	.markdown :global(ul),
	.markdown :global(ol) {
		margin: 0 0 var(--space-3) 0;
		padding-left: var(--space-6);
	}

	.markdown :global(a) {
		color: var(--color-accent);
	}

	/* Compact mode: card-sized previews. Tighter line-height,
	   no paragraph spacing, smaller headings. */
	.markdown.compact {
		line-height: 1.35;
		font-size: var(--text-sm);
	}

	.markdown.compact :global(p),
	.markdown.compact :global(ul),
	.markdown.compact :global(ol) {
		margin: 0;
	}

	.markdown.compact :global(p + p) {
		margin-top: var(--space-1);
	}

	.markdown.compact :global(h1),
	.markdown.compact :global(h2),
	.markdown.compact :global(h3),
	.markdown.compact :global(h4) {
		margin: var(--space-1) 0;
		font-size: var(--text-base);
		font-weight: 600;
	}

	.markdown.compact :global(pre) {
		padding: var(--space-2);
		font-size: 0.85em;
	}
</style>
