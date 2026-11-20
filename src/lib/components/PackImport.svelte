<script lang="ts">
	// Pack import (SPEC §9.4): new cards go to the shelf; cards already in the library are
	// skipped unless ticked for update.
	import { SvelteSet } from 'svelte/reactivity';
	import { api, errorMessage, type PackPreview } from '#lib/api';
	import { toast } from '#lib/toast.svelte';

	let { path, preview, onclose }: { path: string; preview: PackPreview; onclose: () => void } =
		$props();
	const update = new SvelteSet<string>();
	let busy = $state(false);
	const existing = $derived(preview.cards.filter((c) => c.exists));
	const fresh = $derived(preview.cards.length - existing.length);

	async function run() {
		busy = true;
		try {
			const r = await api.packImport(path, [...update]);
			toast.info(
				`Added ${r.added} to the shelf` +
					(r.updated ? `, updated ${r.updated}` : '') +
					(r.skipped ? `, skipped ${r.skipped}` : '')
			);
			onclose();
		} catch (e) {
			toast.error(errorMessage(e));
		} finally {
			busy = false;
		}
	}
</script>

<dialog class="modal modal-open" aria-label="Import pack">
	<div class="modal-box">
		<h3 class="text-lg font-bold">{preview.meta.name || 'Pack'}</h3>
		{#if preview.meta.description}<p class="text-sm opacity-70">{preview.meta.description}</p>{/if}
		<p class="mt-2 text-sm">
			{fresh} new card{fresh === 1 ? '' : 's'} will go to the shelf.
		</p>
		{#if existing.length}
			<p class="mt-3 text-sm">
				Already in your library. Tick any to replace with the pack's version:
			</p>
			<ul class="mt-1 max-h-64 overflow-y-auto">
				{#each existing as c (c.pack_card_id)}
					<li>
						<label class="flex items-center gap-2 py-1 text-sm">
							<input
								type="checkbox"
								class="checkbox checkbox-xs"
								checked={update.has(c.pack_card_id)}
								onchange={() =>
									update.has(c.pack_card_id)
										? update.delete(c.pack_card_id)
										: update.add(c.pack_card_id)}
							/>
							<span class="flex-1">{c.title}</span>
							<span class="opacity-60">{c.category}</span>
						</label>
					</li>
				{/each}
			</ul>
		{/if}
		<div class="modal-action">
			<button type="button" class="btn btn-ghost" onclick={onclose} disabled={busy}>Cancel</button>
			<button
				type="button"
				class="btn btn-primary"
				onclick={run}
				disabled={busy || (fresh === 0 && update.size === 0)}>Import</button
			>
		</div>
	</div>
</dialog>
