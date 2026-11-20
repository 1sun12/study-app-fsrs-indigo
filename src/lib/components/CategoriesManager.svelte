<script lang="ts">
	// Settings → Categories: create, rename, colour, code language, delete. Categories are the
	// only grouping (owner ruling: no decks, so no card can be left out of daily study).
	import { Trash2 } from '@lucide/svelte';
	import { onMount } from 'svelte';
	import { api, errorMessage, type Category } from '#lib/api';
	import { FALLBACK_COLOR, PALETTE, nextColor } from '#lib/palette';
	import { toast } from '#lib/toast.svelte';

	/** Languages people most often keep cards about, shown first in the picker. */
	const COMMON = [
		'bash',
		'c',
		'cpp',
		'csharp',
		'css',
		'gdscript',
		'glsl',
		'go',
		'hlsl',
		'html',
		'java',
		'javascript',
		'json',
		'kotlin',
		'lua',
		'markdown',
		'php',
		'python',
		'ruby',
		'rust',
		'sql',
		'swift',
		'toml',
		'typescript',
		'yaml',
		'zig'
	];

	let categories = $state<Category[]>([]);
	let counts = $state<Record<string, number>>({});
	let languages = $state<{ id: string; name: string }[]>([]);
	let newName = $state('');
	let picking = $state<string | null>(null);

	const common = $derived(languages.filter((l) => COMMON.includes(l.id)));
	const others = $derived(languages.filter((l) => !COMMON.includes(l.id)));

	async function load() {
		categories = await api.categoriesList();
		const cards = await api.cardsQuery({
			text: null,
			category_id: null,
			state: null,
			due_only: false,
			flagged_only: false,
			today: null
		});
		const c: Record<string, number> = {};
		for (const card of cards) c[card.category_id] = (c[card.category_id] ?? 0) + 1;
		counts = c;
	}

	onMount(() => {
		load().catch((e) => toast.error(errorMessage(e)));
		import('shiki/langs')
			.then((m) => (languages = m.bundledLanguagesInfo.map((l) => ({ id: l.id, name: l.name }))))
			.catch(() => {});
	});

	async function save(cat: Category, patch: Partial<Category>) {
		try {
			const saved = await api.categorySave({ ...cat, ...patch });
			categories = categories.map((c) => (c.id === saved.id ? saved : c));
		} catch (e) {
			toast.error(errorMessage(e));
			await load();
		}
	}

	async function create() {
		const name = newName.trim();
		if (!name) return;
		try {
			await api.categorySave({
				id: '',
				name,
				syntax: null,
				color: nextColor(categories.map((c) => c.color)),
				position: 0,
				modified_at: new Date().toISOString(),
				deleted_at: null
			});
			newName = '';
			await load();
		} catch (e) {
			toast.error(errorMessage(e));
		}
	}

	async function remove(cat: Category) {
		try {
			await api.categoryDelete(cat.id);
			await load();
		} catch (e) {
			toast.error(errorMessage(e));
		}
	}
</script>

<div>
	<p class="hint mb-4">
		Colours show as dots everywhere. The code language highlights typed answers and code in cards.
	</p>

	<ul class="divide-y divide-base-300">
		{#each categories as cat (cat.id)}
			<li class="flex flex-wrap items-center gap-3 py-2.5">
				<button
					type="button"
					class="size-6 shrink-0 rounded-full ring-2 ring-base-300 transition hover:scale-110"
					style:background={cat.color ?? FALLBACK_COLOR}
					aria-label="Colour of {cat.name}"
					aria-expanded={picking === cat.id}
					onclick={() => (picking = picking === cat.id ? null : cat.id)}
				></button>
				<input
					class="input min-w-0 flex-1 input-sm"
					value={cat.name}
					aria-label="Name"
					onchange={(e) => save(cat, { name: e.currentTarget.value })}
				/>
				<select
					class="select w-40 select-sm"
					value={cat.syntax ?? ''}
					aria-label="Code language"
					title="Code language for typed answers and code blocks"
					onchange={(e) => save(cat, { syntax: e.currentTarget.value || null })}
				>
					<option value="">No code</option>
					{#if cat.syntax && !languages.some((l) => l.id === cat.syntax)}
						<option value={cat.syntax}>{cat.syntax}</option>
					{/if}
					<optgroup label="Common">
						{#each common as l (l.id)}<option value={l.id}>{l.name}</option>{/each}
					</optgroup>
					<optgroup label="All languages">
						{#each others as l (l.id)}<option value={l.id}>{l.name}</option>{/each}
					</optgroup>
				</select>
				<span class="w-16 text-right text-xs opacity-60"
					>{counts[cat.id] ?? 0} card{counts[cat.id] === 1 ? '' : 's'}</span
				>
				<button
					type="button"
					class="btn btn-square btn-ghost text-error btn-sm disabled:opacity-30"
					disabled={(counts[cat.id] ?? 0) > 0}
					title={(counts[cat.id] ?? 0) > 0 ? 'Move or delete its cards first' : 'Delete category'}
					aria-label="Delete {cat.name}"
					onclick={() => remove(cat)}><Trash2 class="size-4" /></button
				>
				{#if picking === cat.id}
					<div class="flex w-full flex-wrap items-center gap-2 pl-9">
						{#each PALETTE as c (c)}
							<button
								type="button"
								class="size-6 rounded-full transition hover:scale-110"
								class:ring-2={cat.color === c}
								class:ring-base-content={cat.color === c}
								style:background={c}
								aria-label="Use {c}"
								onclick={() => save(cat, { color: c })}
							></button>
						{/each}
						<input
							type="color"
							class="size-7 cursor-pointer"
							title="Any colour"
							value={cat.color ?? FALLBACK_COLOR}
							onchange={(e) => save(cat, { color: e.currentTarget.value })}
						/>
					</div>
				{/if}
			</li>
		{:else}
			<li class="py-3 text-sm opacity-70">No categories yet. Create your first below.</li>
		{/each}
	</ul>

	<form
		class="mt-4 flex gap-2"
		onsubmit={(e) => {
			e.preventDefault();
			create();
		}}
	>
		<input class="input flex-1 input-sm" placeholder="New category" bind:value={newName} />
		<button type="submit" class="btn btn-primary btn-sm" disabled={!newName.trim()}>Create</button>
	</form>
</div>
