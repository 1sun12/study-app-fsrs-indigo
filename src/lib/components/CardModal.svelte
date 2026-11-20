<script lang="ts">
	// Foreground dialog for adding and editing cards. Focus stays inside (Tab wraps); Esc or a
	// backdrop click closes it, asking first when there are unsaved changes.
	import { cardModal } from '#lib/cardModal.svelte';
	import { prefs } from '#lib/prefs.svelte';
	import CardEditor from './CardEditor.svelte';

	let box = $state<HTMLDivElement>();

	function onkeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			// With Vim on, Esc inside an editor means "normal mode", not "close the card".
			if (prefs.settings?.editor.vim && (e.target as HTMLElement | null)?.closest('.cm-editor'))
				return;
			e.preventDefault();
			cardModal.requestClose();
			return;
		}
		if (e.key !== 'Tab' || !box) return;
		const focusable = [
			...box.querySelectorAll<HTMLElement>(
				'button:not([disabled]), input, select, textarea, [tabindex]:not([tabindex="-1"]), [contenteditable="true"]'
			)
		].filter((el) => el.offsetParent !== null);
		if (focusable.length === 0) return;
		const first = focusable[0];
		const last = focusable[focusable.length - 1];
		if (e.shiftKey && document.activeElement === first) {
			e.preventDefault();
			last.focus();
		} else if (!e.shiftKey && document.activeElement === last) {
			e.preventDefault();
			first.focus();
		}
	}
</script>

{#if cardModal.card}
	<dialog
		class="modal modal-open"
		aria-label={cardModal.card.id ? 'Edit card' : 'New card'}
		{onkeydown}
	>
		<div bind:this={box} class="modal-box max-h-[90vh] w-11/12 max-w-2xl p-6">
			{#key cardModal.instance}
				<CardEditor card={cardModal.card} categories={cardModal.categories} />
			{/key}
		</div>
		<button
			type="button"
			class="modal-backdrop"
			aria-label="Close"
			onclick={() => cardModal.requestClose()}
		></button>
	</dialog>
{/if}
