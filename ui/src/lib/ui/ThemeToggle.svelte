<script lang="ts">
	import { themeStore } from '$lib/stores/theme.svelte';
</script>

<button
	type="button"
	class="theme-toggle"
	aria-label={themeStore.value === 'dark' ? 'Switch to light theme' : 'Switch to dark theme'}
	onclick={() => {
		themeStore.toggle();
	}}
>
	{#if themeStore.value === 'dark'}
		<!-- Sun -->
		<svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
			<circle cx="12" cy="12" r="4" fill="currentColor" />
			<g stroke="currentColor" stroke-width="2" stroke-linecap="round">
				<line x1="12" y1="2" x2="12" y2="5" />
				<line x1="12" y1="19" x2="12" y2="22" />
				<line x1="2" y1="12" x2="5" y2="12" />
				<line x1="19" y1="12" x2="22" y2="12" />
				<line x1="4.5" y1="4.5" x2="6.5" y2="6.5" />
				<line x1="17.5" y1="17.5" x2="19.5" y2="19.5" />
				<line x1="4.5" y1="19.5" x2="6.5" y2="17.5" />
				<line x1="17.5" y1="6.5" x2="19.5" y2="4.5" />
			</g>
		</svg>
	{:else}
		<!-- Moon -->
		<svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
			<path d="M21 12.8A8 8 0 1 1 11.2 3a6.5 6.5 0 0 0 9.8 9.8z" fill="currentColor" />
		</svg>
	{/if}
</button>

<style>
	.theme-toggle {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 2rem;
		height: 2rem;
		color: var(--color-fg-muted);
		border-radius: var(--radius-md);
	}

	.theme-toggle:hover {
		color: var(--color-fg);
		background-color: var(--color-surface);
	}
</style>
