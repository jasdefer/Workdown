<!--
  The type-specific block of the field editor, dispatched on the shape
  the draft's type is edited as: the bounds for `numeric` and
  `duration`, the pattern for `text`, the ordered value list for
  `values`, the cycle policy and inverse for `relation`, nothing for a
  plain `scalar`.

  One component so the panel renders one line for the type-specific
  part, whatever the type. Owns no state: the draft's shape is the
  value, every change goes up as the whole new shape, and the problem
  the panel found with it comes down to be shown by the block it is
  about.
-->
<script lang="ts">
	import type { FieldShape } from '$lib/api/generated/FieldShape';
	import type { FieldType } from '$lib/api/generated/FieldType';
	import BoundsBlock from './BoundsBlock.svelte';
	import RelationBlock from './RelationBlock.svelte';
	import TextBlock from './TextBlock.svelte';
	import ValuesBlock from './ValuesBlock.svelte';

	interface Props {
		shape: FieldShape;
		fieldType: FieldType;
		/** Why the shape cannot be saved as it stands; `null` when it can. */
		problem: string | null;
		disabled?: boolean;
		onchange: (shape: FieldShape) => void;
	}

	let { shape, fieldType, problem, disabled = false, onchange }: Props = $props();
</script>

{#if shape.kind === 'numeric' || shape.kind === 'duration'}
	<BoundsBlock {shape} {fieldType} {problem} {disabled} {onchange} />
{:else if shape.kind === 'text'}
	<TextBlock {shape} {disabled} {onchange} />
{:else if shape.kind === 'values'}
	<ValuesBlock {shape} {problem} {disabled} {onchange} />
{:else if shape.kind === 'relation'}
	<RelationBlock {shape} {disabled} {onchange} />
{/if}
