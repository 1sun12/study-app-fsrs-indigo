<script lang="ts">
	import { X } from '@lucide/svelte';
	import type { Criterion } from '#lib/api/types/Criterion';

	let { criteria = $bindable([]) }: { criteria: Criterion[] } = $props();

	function add() {
		criteria = [...criteria, { id: '', text: '', weight: 1 }];
	}
	function remove(i: number) {
		criteria = criteria.filter((_, j) => j !== i);
	}
</script>

<div class="space-y-2">
	<div class="flex items-baseline gap-2">
		<span class="text-sm font-medium">Checklist</span>
		<span class="hint"
			>reminders to tick while you check your work; you still grade the card yourself</span
		>
	</div>
	{#each criteria as c, i (i)}
		<div class="flex items-center gap-2">
			<input
				class="input flex-1 input-sm"
				placeholder="e.g. Edges are clean"
				bind:value={c.text}
				onkeydown={(e) => {
					if (e.key === 'Enter') {
						e.preventDefault();
						add();
					}
				}}
			/>
			<button
				type="button"
				class="btn btn-square btn-ghost btn-xs"
				onclick={() => remove(i)}
				aria-label="Remove check"><X class="size-3.5" /></button
			>
		</div>
	{/each}
	<button type="button" class="btn btn-ghost btn-xs" onclick={add}>+ Add check</button>
</div>
