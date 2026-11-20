<script lang="ts">
	// Settings → Tags & filters. Built-in filters (difficulty, state) are fixed; the user's tags
	// and saved Library filters are theirs to create, rename and delete (owner ruling
	// 2026-11-14). Tag edits rewrite the cards that carry them, so they sync; created-but-unused
	// tags and saved filters live in device-local settings (autosaved by the Settings page).
	import { Lock, Plus, Trash2 } from '@lucide/svelte';
	import { onMount } from 'svelte';
	import { confirm } from '@tauri-apps/plugin-dialog';
	import { api, errorMessage, type Category, type Settings } from '#lib/api';
	import type { CardState } from '#lib/api/types/CardState';
	import type { Difficulty } from '#lib/api/types/Difficulty';
	import { cardModal } from '#lib/cardModal.svelte';
	import { toast } from '#lib/toast.svelte';

	let { settings = $bindable() }: { settings: Settings } = $props();

	const DIFFICULTIES: Difficulty[] = ['beginner', 'intermediate', 'advanced', 'expert', 'master'];
	const STATES: [CardState, string][] = [
		['active', 'Active'],
		['shelved', 'On the shelf'],
		['suspended', 'Suspended']
	];
	const cap = (s: string) => s[0].toUpperCase() + s.slice(1);

	let counts = $state<Record<string, number>>({});
	let categories = $state<Category[]>([]);
	let newTag = $state('');
	let busy = $state(false);

	const tags = $derived(
		[...new Set([...Object.keys(counts), ...settings.tags])].sort((a, b) => a.localeCompare(b))
	);

	async function load() {
		const cards = await api.cardsQuery();
		const c: Record<string, number> = {};
		for (const card of cards) for (const t of card.tags) c[t] = (c[t] ?? 0) + 1;
		counts = c;
	}
	onMount(() => {
		load().catch((e) => toast.error(errorMessage(e)));
		api
			.categoriesList()
			.then((c) => (categories = c))
			.catch(() => {});
	});

	function create() {
		const t = newTag.trim();
		if (!t) return;
		if (!tags.includes(t)) settings.tags = [...settings.tags, t];
		newTag = '';
	}

	/** Every place a tag name lives on this device: the unused list and saved filters. */
	function renameLocal(from: string, to: string | null) {
		settings.tags = settings.tags.filter((t) => t !== from && t !== to);
		if (to && !counts[to] && !counts[from]) settings.tags = [...settings.tags, to];
		settings.saved_filters = settings.saved_filters.map((f) =>
			f.tag === from ? { ...f, tag: to } : f
		);
	}

	async function rename(from: string, input: HTMLInputElement) {
		const to = input.value.trim();
		if (!to || to === from) {
			input.value = from;
			return;
		}
		if (
			tags.includes(to) &&
			!(await confirm(`Merge "${from}" into "${to}"? Cards tagged "${from}" get "${to}".`, {
				title: 'Merge tags'
			}))
		) {
			input.value = from;
			return;
		}
		busy = true;
		try {
			if (counts[from]) await api.tagRename(from, to);
			renameLocal(from, to);
			await load();
			cardModal.version++;
		} catch (e) {
			toast.error(errorMessage(e));
			input.value = from;
		} finally {
			busy = false;
		}
	}

	async function remove(tag: string) {
		const n = counts[tag] ?? 0;
		if (
			n &&
			!(await confirm(`Remove "${tag}" from ${n} card${n === 1 ? '' : 's'}? The cards stay.`, {
				title: 'Delete tag',
				kind: 'warning'
			}))
		)
			return;
		busy = true;
		try {
			if (n) await api.tagDelete(tag);
			renameLocal(tag, null);
			await load();
			cardModal.version++;
		} catch (e) {
			toast.error(errorMessage(e));
		} finally {
			busy = false;
		}
	}

	function addFilter() {
		settings.saved_filters = [
			...settings.saved_filters,
			{
				id: crypto.randomUUID(),
				name: `Filter ${settings.saved_filters.length + 1}`,
				text: null,
				category_id: null,
				state: null,
				difficulty: null,
				tag: null
			}
		];
	}
</script>

<div class="space-y-8">
	<section class="space-y-2">
		<h3 class="flex items-center gap-2 font-semibold">
			<Lock class="size-4 opacity-60" />Built-in filters
		</h3>
		<p class="hint">Always available in the Library; they cannot be renamed or removed.</p>
		<div class="flex flex-wrap gap-1.5">
			{#each DIFFICULTIES as d (d)}<span class="badge badge-ghost">{cap(d)}</span>{/each}
			{#each STATES as [, label] (label)}<span class="badge badge-ghost">{label}</span>{/each}
			<span class="badge badge-ghost">Due</span>
			<span class="badge badge-ghost">Weak step</span>
		</div>
	</section>

	<section class="space-y-2">
		<h3 class="font-semibold">Tags</h3>
		<p class="hint">
			Renaming onto an existing tag merges them. Changes apply to every card and sync.
		</p>
		<ul class="divide-y divide-base-300">
			{#each tags as tag (tag)}
				<li class="flex items-center gap-3 py-2">
					<input
						class="input min-w-0 flex-1 input-sm"
						value={tag}
						aria-label="Tag name"
						disabled={busy}
						onchange={(e) => rename(tag, e.currentTarget)}
					/>
					<span class="w-20 text-right text-xs opacity-60"
						>{counts[tag] ?? 0} card{counts[tag] === 1 ? '' : 's'}</span
					>
					<button
						type="button"
						class="btn btn-square btn-ghost text-error btn-sm"
						title="Delete tag"
						aria-label="Delete {tag}"
						disabled={busy}
						onclick={() => remove(tag)}><Trash2 class="size-4" /></button
					>
				</li>
			{:else}
				<li class="py-3 text-sm opacity-70">
					No tags yet. Create one below or add tags to a card.
				</li>
			{/each}
		</ul>
		<form
			class="flex gap-2"
			onsubmit={(e) => {
				e.preventDefault();
				create();
			}}
		>
			<input class="input flex-1 input-sm" placeholder="New tag" bind:value={newTag} />
			<button type="submit" class="btn btn-primary btn-sm" disabled={!newTag.trim()}>Create</button>
		</form>
	</section>

	<section class="space-y-2">
		<h3 class="font-semibold">Saved filters</h3>
		<p class="hint">
			Pick one from the Library's "Saved filter" menu. Empty fields match everything.
		</p>
		{#each settings.saved_filters as f, i (f.id)}
			<div class="space-y-2 rounded-box border border-base-300 p-3">
				<div class="flex items-center gap-2">
					<input
						class="input min-w-0 flex-1 font-medium input-sm"
						aria-label="Filter name"
						bind:value={settings.saved_filters[i].name}
					/>
					<button
						type="button"
						class="btn btn-square btn-ghost text-error btn-sm"
						title="Delete filter"
						aria-label="Delete {f.name}"
						onclick={() =>
							(settings.saved_filters = settings.saved_filters.filter((x) => x.id !== f.id))}
						><Trash2 class="size-4" /></button
					>
				</div>
				<div class="flex flex-wrap gap-2">
					<input
						class="input w-44 input-sm"
						placeholder="Title contains"
						aria-label="Title contains"
						value={f.text ?? ''}
						onchange={(e) =>
							(settings.saved_filters[i].text = e.currentTarget.value.trim() || null)}
					/>
					<select
						class="select w-44 select-sm"
						aria-label="Category"
						bind:value={settings.saved_filters[i].category_id}
					>
						<option value={null}>Any category</option>
						{#each categories as c (c.id)}<option value={c.id}>{c.name}</option>{/each}
					</select>
					<select
						class="select w-36 select-sm"
						aria-label="State"
						bind:value={settings.saved_filters[i].state}
					>
						<option value={null}>Any state</option>
						{#each STATES as [v, label] (v)}<option value={v}>{label}</option>{/each}
					</select>
					<select
						class="select w-36 select-sm"
						aria-label="Difficulty"
						bind:value={settings.saved_filters[i].difficulty}
					>
						<option value={null}>Any difficulty</option>
						{#each DIFFICULTIES as d (d)}<option value={d}>{cap(d)}</option>{/each}
					</select>
					<select
						class="select w-36 select-sm"
						aria-label="Tag"
						bind:value={settings.saved_filters[i].tag}
					>
						<option value={null}>Any tag</option>
						{#each tags as t (t)}<option value={t}>{t}</option>{/each}
					</select>
				</div>
			</div>
		{/each}
		<button type="button" class="btn gap-1.5 btn-sm" onclick={addFilter}
			><Plus class="size-4" />New saved filter</button
		>
	</section>
</div>
