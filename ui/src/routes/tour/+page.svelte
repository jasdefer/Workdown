<!--
  `/tour`: the animated project overview. The page is a thin shell around
  `Tour`; when the last scene ends (or the visitor leaves), it navigates
  to the landing view — the first in views.yaml, the same one `/` opens.
-->
<script lang="ts">
	import { goto } from '$app/navigation';
	import Tour from '$lib/tour/Tour.svelte';
	import type { PageData } from './$types';

	interface Props {
		data: PageData;
	}

	let { data }: Props = $props();

	function openView(viewId: string): void {
		void goto(`/views/${encodeURIComponent(viewId)}`);
	}

	function exit(): void {
		if (data.plan.landing !== null) openView(data.plan.landing);
		else void goto('/');
	}
</script>

<Tour
	input={{ views: data.views, plan: data.plan, data: data.data, project: data.project }}
	onFinished={openView}
	onExit={exit}
/>
