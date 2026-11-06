<!--
  The item detail docked at the side of the view: the shared
  `DockedPanel` chrome (narrow) around the shared `ItemEditor`, with
  the shared header actions on the left of its header row. The page
  behind stays fully usable, which is the point: a click on the next
  card swaps the panel's item through `?item=` while the panel stays
  mounted, and the view stays in sight.

  Closing — the chrome's close button, Escape — routes through
  `onclose` like the modal: the host drops the query param, the URL
  stays the one truth.

  The panel's body is a size container (from `DockedPanel`), so the
  editor stacks its header, fields and body at this width by its own
  layout rule — nothing here tells it how to lay out.
-->
<script lang="ts">
	import ItemEditor from '$lib/items/ItemEditor.svelte';
	import { itemHref } from '$lib/items/itemLink';
	import DockedPanel from '$lib/ui/DockedPanel.svelte';
	import ItemDetailActions from './ItemDetailActions.svelte';

	interface Props {
		itemId: string;
		onclose: () => void;
		onmutate: () => void;
	}

	let { itemId, onclose, onmutate }: Props = $props();
</script>

<DockedPanel label="Item detail" {onclose}>
	{#snippet header()}
		<ItemDetailActions {itemId} />
	{/snippet}
	<!-- A reference followed from inside swaps the panel's item. -->
	<ItemEditor {itemId} {onmutate} hrefFor={itemHref} />
</DockedPanel>
