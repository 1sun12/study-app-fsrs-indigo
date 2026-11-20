<script lang="ts">
	// "?" opens the keyboard map (SPEC §14: full keyboard path for study).
	let open = $state(false);

	const rows = [
		['?', 'This help'],
		['Esc', 'Pause the session (Study)'],
		['Enter', 'Reveal (tool / recall steps), Next on the summary'],
		['Ctrl + Enter', 'Submit a typed answer; save a card in the editor'],
		['1 · 2 · 3 · 4', 'Forgot · Hard · Good · Easy (only while the reference is shown)'],
		['Space', 'Toggle the focused criterion']
	];

	function onkeydown(e: KeyboardEvent) {
		const field = (e.target as HTMLElement | null)?.closest(
			'.cm-editor, input, textarea, [contenteditable]'
		);
		if (e.key === '?' && !field) {
			e.preventDefault();
			open = !open;
		} else if (e.key === 'Escape' && open) {
			open = false;
		}
	}
</script>

<svelte:window {onkeydown} />

{#if open}
	<dialog class="modal modal-open">
		<div class="modal-box">
			<h3 class="text-lg font-bold">Keyboard</h3>
			<table class="table mt-2 table-sm">
				<tbody>
					{#each rows as [key, what] (key)}
						<tr><td class="w-36"><kbd class="kbd kbd-sm">{key}</kbd></td><td>{what}</td></tr>
					{/each}
				</tbody>
			</table>
			<div class="modal-action">
				<button type="button" class="btn" onclick={() => (open = false)}>Close</button>
			</div>
		</div>
		<form method="dialog" class="modal-backdrop">
			<button type="button" onclick={() => (open = false)}>close</button>
		</form>
	</dialog>
{/if}
