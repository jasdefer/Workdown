<!--
  Shared confirmation dialog: one question, confirm or cancel. Presents
  as a popover anchored to the control that was clicked, built on a
  native `<dialog>` opened with `showModal()` — top layer, inert page
  behind it, Escape and focus handling for free — with the centered
  default overridden by measured coordinates and the backdrop styled
  invisible. The caller mounts it conditionally and unmounts it on
  either callback; confirm closes immediately — whatever follows is the
  caller's story (workdown-items/confirm-dialog.md).
-->
<script lang="ts" module>
	// Distinguishes the label/description element ids should two dialogs
	// ever be mounted at once (only one can be *open* — showModal).
	let nextUid = 0;
</script>

<script lang="ts">
	import { positionPopover } from './confirmPosition';

	interface Props {
		/** The control the question is about — the popover attaches to it. */
		anchor: HTMLElement;
		title: string;
		/** One sentence of plain text under the title. */
		body: string;
		confirmLabel: string;
		cancelLabel?: string;
		/** Styles the confirm button as destructive (red). */
		destructive?: boolean;
		onconfirm: () => void;
		oncancel: () => void;
	}

	let {
		anchor,
		title,
		body,
		confirmLabel,
		cancelLabel = 'Cancel',
		destructive = false,
		onconfirm,
		oncancel
	}: Props = $props();

	nextUid += 1;
	const uid = `confirm-dialog-${String(nextUid)}`;

	let dialog = $state<HTMLDialogElement>();
	let cancelButton = $state<HTMLButtonElement>();
	let top = $state(0);
	let left = $state(0);

	// Whether a button already answered the question — the `close` event
	// that follows must not turn that into a second callback.
	let answered = false;

	$effect(() => {
		if (dialog === undefined || dialog.open) return;
		const dialogElement = dialog;
		dialogElement.showModal();
		reposition();
		// Focus starts on Cancel so the reflexive second Enter from the
		// keypress that opened the dialog cannot confirm it unseen.
		cancelButton?.focus();
		// Close before unmounting so the browser restores focus to the
		// trigger even when the caller drops the component while open.
		return () => {
			dialogElement.close();
		};
	});

	function reposition(): void {
		if (dialog === undefined) return;
		const position = positionPopover(
			anchor.getBoundingClientRect(),
			{ width: dialog.offsetWidth, height: dialog.offsetHeight },
			{ width: window.innerWidth, height: window.innerHeight }
		);
		top = position.top;
		left = position.left;
	}

	function answer(callback: () => void): void {
		answered = true;
		callback();
	}

	function onclose(): void {
		// Escape (or any close the buttons did not cause) is a cancel.
		if (!answered) answer(oncancel);
	}

	function onclick(event: MouseEvent): void {
		// The dialog box is fully covered by `.content`, so a click whose
		// target is the dialog element itself hit the invisible backdrop.
		if (event.target === dialog) answer(oncancel);
	}
</script>

<svelte:window onresize={reposition} />

<dialog
	bind:this={dialog}
	style:top="{top}px"
	style:left="{left}px"
	aria-labelledby="{uid}-title"
	aria-describedby="{uid}-body"
	{onclose}
	{onclick}
>
	<div class="content">
		<p class="title" id="{uid}-title">{title}</p>
		<p class="body" id="{uid}-body">{body}</p>
		<div class="actions">
			<button
				type="button"
				class="cancel"
				bind:this={cancelButton}
				onclick={() => {
					answer(oncancel);
				}}
			>
				{cancelLabel}
			</button>
			<button
				type="button"
				class="confirm"
				class:destructive
				onclick={() => {
					answer(onconfirm);
				}}
			>
				{confirmLabel}
			</button>
		</div>
	</div>
</dialog>

<style>
	dialog {
		/* Override the user agent's centered-modal styles: the popover
		   sits at measured coordinates next to its anchor. */
		position: fixed;
		margin: 0;
		width: max-content;
		max-width: min(20rem, calc(100vw - 1rem));
		padding: 0;
		background-color: var(--color-card);
		color: var(--color-fg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		box-shadow: var(--shadow-sm);
	}

	/* The page behind is inert (showModal) but not dimmed — a popover,
	   not a full-screen interruption. */
	dialog::backdrop {
		background: transparent;
	}

	.content {
		padding: var(--space-3) var(--space-4);
	}

	.title {
		margin: 0;
		font-size: var(--text-sm);
		font-weight: 600;
	}

	.body {
		margin: var(--space-1) 0 0;
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}

	.actions {
		display: flex;
		justify-content: flex-end;
		gap: var(--space-3);
		margin-top: var(--space-3);
	}

	.cancel {
		background: none;
		border: none;
		padding: 0.35rem var(--space-2);
		color: var(--color-fg-muted);
		font-size: var(--text-sm);
		cursor: pointer;
	}

	.cancel:hover {
		color: var(--color-fg);
	}

	.confirm {
		background-color: var(--color-accent);
		color: var(--color-accent-fg);
		border: 1px solid var(--color-accent);
		border-radius: var(--radius-sm);
		padding: 0.35rem var(--space-4);
		font-size: var(--text-sm);
		font-weight: 600;
		cursor: pointer;
	}

	.confirm.destructive {
		background-color: var(--color-danger);
		border-color: var(--color-danger);
		color: var(--color-danger-fg);
	}
</style>
