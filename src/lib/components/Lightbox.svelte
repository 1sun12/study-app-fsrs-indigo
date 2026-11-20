<script lang="ts">
	let {
		url,
		caption,
		onclose
	}: { url: string | null; caption: string | null; onclose: () => void } = $props();
</script>

{#if url}
	<dialog class="modal modal-open" {onclose}>
		<div class="modal-box max-w-5xl p-2">
			<img src={url} alt={caption ?? ''} class="mx-auto max-h-[85vh]" />
			{#if caption}<p class="p-2 text-center text-sm opacity-70">{caption}</p>{/if}
		</div>
		<form method="dialog" class="modal-backdrop">
			<button type="button" onclick={onclose}>close</button>
		</form>
	</dialog>
{/if}
