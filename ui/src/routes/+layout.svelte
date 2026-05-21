<script lang="ts">
	import '../app.css';
	import type { Snippet } from 'svelte';
	import favicon from '$lib/assets/favicon.svg';
	import ThemeToggle from '$lib/ui/ThemeToggle.svelte';

	interface Props {
		children: Snippet;
	}

	let { children }: Props = $props();
</script>

<svelte:head>
	<title>Workdown</title>
	<link rel="icon" href={favicon} />
</svelte:head>

<div class="shell">
	<header class="app-header">
		<a class="brand" href="/">Workdown</a>
		<div class="header-actions">
			<ThemeToggle />
		</div>
	</header>
	<main class="app-main">
		{@render children()}
	</main>
</div>

<style>
	.shell {
		display: flex;
		flex-direction: column;
		min-height: 100vh;
	}

	.app-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: var(--space-3) var(--space-6);
		background-color: var(--color-surface);
		border-bottom: 1px solid var(--color-border);
	}

	.brand {
		font-weight: 600;
		color: var(--color-fg);
		text-decoration: none;
	}

	.header-actions {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.app-main {
		flex: 1;
		padding: var(--space-6);
	}
</style>
