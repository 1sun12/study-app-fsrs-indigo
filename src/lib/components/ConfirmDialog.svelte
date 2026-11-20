<script lang="ts">
	// The in-app confirm (confirm.svelte.ts). showModal() puts it above the card editor and makes
	// the page behind it inert; its keys stop here so page shortcuts never fire behind it.
	import { confirmState } from '#lib/confirm.svelte';

	let dialog = $state<HTMLDialogElement>();
	let ok = $state<HTMLButtonElement>();

	$effect(() => {
		if (!dialog) return;
		if (confirmState.ask && !dialog.open) {
			dialog.showModal();
			ok?.focus();
		} else if (!confirmState.ask && dialog.open) {
			dialog.close();
		}
	});
</script>

<dialog
	bind:this={dialog}
	class="modal"
	aria-labelledby="confirm-title"
	oncancel={(e) => {
		e.preventDefault(); // Esc: answer "no" ourselves so the promise always settles
		confirmState.answer(false);
	}}
	onkeydown={(e) => e.stopPropagation()}
>
	{#if confirmState.ask}
		{@const ask = confirmState.ask}
		<div class="modal-box max-w-md">
			<h3 id="confirm-title" class="font-display text-lg">{ask.title ?? 'Are you sure?'}</h3>
			<p class="mt-2 text-sm break-words whitespace-pre-line opacity-80">{ask.message}</p>
			<div class="modal-action">
				<button
					type="button"
					class="btn btn-ghost btn-sm"
					onclick={() => confirmState.answer(false)}>Cancel</button
				>
				<button
					bind:this={ok}
					type="button"
					class="btn btn-sm {ask.kind === 'warning' || ask.kind === 'error'
						? 'btn-error'
						: 'btn-primary'}"
					onclick={() => confirmState.answer(true)}>{ask.okLabel ?? 'OK'}</button
				>
			</div>
		</div>
		<button
			type="button"
			class="modal-backdrop"
			aria-label="Cancel"
			onclick={() => confirmState.answer(false)}
		></button>
	{/if}
</dialog>
