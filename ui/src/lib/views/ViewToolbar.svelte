<!--
  A view page's authoring actions: Edit (a link to the edit form) and
  Delete (confirm, then navigate home). Owns the delete-in-flight and
  delete-error state; the host mounts this inside its `{#key viewId}`
  block, so navigating to another view remounts it and a stale error
  from a failed delete never leaks onto the next view's page.
-->
<script lang="ts">
	import { goto } from '$app/navigation';
	import { api } from '$lib/api/client';

	interface Props {
		/** The view whose page hosts this toolbar. */
		viewId: string;
	}

	let { viewId }: Props = $props();

	let deleting = $state(false);
	let deleteError = $state<string | null>(null);

	async function deleteView(): Promise<void> {
		if (!confirm(`Delete view '${viewId}' from views.yaml?`)) return;
		deleting = true;
		deleteError = null;
		const result = await api.deleteView(viewId);
		deleting = false;
		if (result.error !== undefined) {
			deleteError = result.error;
			return;
		}
		// The view is gone: leave its page and refresh the navigation list.
		await goto('/', { invalidateAll: true });
	}
</script>

<div class="view-toolbar">
	<a class="toolbar-action" href={`/views/${encodeURIComponent(viewId)}/edit`}>Edit view</a>
	<button type="button" class="toolbar-action danger" disabled={deleting} onclick={deleteView}>
		{deleting ? 'Deleting…' : 'Delete view'}
	</button>
</div>

{#if deleteError !== null}
	<p class="delete-error" role="alert">{deleteError}</p>
{/if}

<style>
	.view-toolbar {
		display: flex;
		justify-content: flex-end;
		gap: var(--space-3);
	}

	.toolbar-action {
		background: none;
		border: none;
		padding: 0;
		color: var(--color-fg-muted);
		font-size: var(--text-sm);
		text-decoration: none;
		cursor: pointer;
	}

	.toolbar-action:hover {
		color: var(--color-fg);
		text-decoration: underline;
	}

	.toolbar-action.danger:hover {
		color: var(--color-error-fg);
	}

	.toolbar-action:disabled {
		opacity: 0.5;
		cursor: default;
	}

	.delete-error {
		color: var(--color-error-fg);
		background-color: var(--color-error-bg);
		padding: var(--space-2);
		border-radius: var(--radius-sm);
		font-size: var(--text-sm);
		margin: 0;
	}
</style>
