<script lang="ts">
	import { Plus } from '@lucide/svelte';
	import { api, errorMessage, type Category } from '#lib/api';
	import { FALLBACK_COLOR, nextColor } from '#lib/palette';
	import { toast } from '#lib/toast.svelte';

	let {
		value = $bindable(''),
		categories = $bindable([])
	}: { value: string; categories: Category[] } = $props();
	let creating = $state(categories.length === 0);
	let name = $state('');
	const color = $derived(categories.find((c) => c.id === value)?.color ?? FALLBACK_COLOR);

	async function create() {
		const n = name.trim();
		if (!n) return;
		try {
			const cat = await api.categorySave({
				id: '',
				name: n,
				syntax: null,
				color: nextColor(categories.map((c) => c.color)),
				position: 0,
				modified_at: new Date().toISOString(),
				deleted_at: null
			});
			categories = [...categories.filter((c) => c.id !== cat.id), cat];
			value = cat.id;
			creating = false;
			name = '';
		} catch (e) {
			toast.error(errorMessage(e));
		}
	}
</script>

{#if creating}
	<div class="join">
		<!-- svelte-ignore a11y_autofocus -->
		<input
			class="input join-item w-44 input-sm"
			placeholder="New category name"
			bind:value={name}
			autofocus={categories.length > 0}
			onkeydown={(e) => {
				if (e.key === 'Enter') {
					e.preventDefault();
					create();
				}
				if (e.key === 'Escape' && categories.length > 0) {
					e.stopPropagation();
					creating = false;
				}
			}}
		/>
		<button type="button" class="btn join-item btn-sm" onclick={create}>Create</button>
	</div>
{:else}
	<div class="flex items-center gap-1">
		<span class="mx-1 size-3 shrink-0 rounded-full" style:background={color}></span>
		<select class="select w-44 select-sm" bind:value aria-label="Category">
			{#each categories as c (c.id)}
				<option value={c.id}>{c.name}</option>
			{/each}
		</select>
		<button
			type="button"
			class="btn btn-square btn-ghost btn-sm"
			onclick={() => (creating = true)}
			title="New category"
			aria-label="New category"><Plus class="size-4" /></button
		>
	</div>
{/if}
