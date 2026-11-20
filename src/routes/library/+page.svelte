<script lang="ts">
	// Library (SPEC §14): search, dropdown filters (plus tags and the user's saved filters), rows;
	// select with the checkboxes (Shift selects a range) and act on many cards at once.
	// Categories, tags and saved filters are managed in Settings.
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { confirm, save } from '@tauri-apps/plugin-dialog';
	import {
		FolderOpen,
		Pause,
		Pencil,
		Play,
		Search,
		Shapes,
		Tags,
		Trash2,
		PackageOpen,
		Zap
	} from '@lucide/svelte';
	import { SvelteSet } from 'svelte/reactivity';
	import { api, errorMessage, type CardSummary, type Category } from '#lib/api';
	import type { Difficulty } from '#lib/api/types/Difficulty';
	import { cardModal } from '#lib/cardModal.svelte';
	import { FALLBACK_COLOR } from '#lib/palette';
	import { prefs } from '#lib/prefs.svelte';
	import { session } from '#lib/study/session.svelte';
	import { toast } from '#lib/toast.svelte';

	type Status = 'all' | 'due' | 'active' | 'shelved' | 'flagged' | 'suspended';
	const DIFFICULTIES: Difficulty[] = ['beginner', 'intermediate', 'advanced', 'expert', 'master'];
	const cap = (s: string) => s[0].toUpperCase() + s.slice(1);

	let cards = $state<CardSummary[]>([]);
	let categories = $state<Category[]>([]);
	let text = $state('');
	let status = $state<Status>('all');
	let categoryId = $state<string | null>(null);
	let difficulty = $state<Difficulty | null>(null);
	let tag = $state<string | null>(null);
	let savedId = $state('');
	const selected = new SvelteSet<string>();
	let anchor = $state<number | null>(null);
	let loading = $state(true);
	let busy = $state(false);

	const visible = $derived(
		cards.filter(
			(c) => (!difficulty || c.difficulty === difficulty) && (!tag || c.tags.includes(tag))
		)
	);
	const allTags = $derived(
		[...new Set([...cards.flatMap((c) => c.tags), ...(prefs.settings?.tags ?? [])])].sort()
	);
	const filtered = $derived(!!(text || status !== 'all' || categoryId || difficulty || tag));

	/** Applies a saved filter from Settings → Tags & filters. */
	function applySaved(id: string) {
		const f = prefs.settings?.saved_filters.find((x) => x.id === id);
		if (!f) return;
		text = f.text ?? '';
		categoryId = f.category_id;
		status = f.state ?? 'all';
		difficulty = f.difficulty;
		tag = f.tag;
	}
	const allSelected = $derived(visible.length > 0 && visible.every((c) => selected.has(c.id)));
	const chosen = $derived(visible.filter((c) => selected.has(c.id)));

	async function load() {
		try {
			cards = await api.cardsQuery({
				text: text || null,
				category_id: categoryId,
				state:
					status === 'active' || status === 'shelved' || status === 'suspended' ? status : null,
				due_only: status === 'due',
				flagged_only: status === 'flagged'
			});
			for (const id of [...selected]) if (!cards.some((c) => c.id === id)) selected.delete(id);
		} catch (e) {
			toast.error(errorMessage(e));
		} finally {
			loading = false;
		}
	}
	async function loadCategories() {
		categories = await api.categoriesList().catch(() => categories);
	}
	onMount(loadCategories);
	$effect(() => {
		void [text, status, categoryId, cardModal.version];
		const t = setTimeout(load, 150);
		return () => clearTimeout(t);
	});

	/** Checkbox click; with Shift, every row between the last click and this one follows. */
	function pick(e: MouseEvent, index: number) {
		const id = visible[index].id;
		const on = !selected.has(id);
		if (e.shiftKey && anchor !== null && anchor < visible.length) {
			const [a, b] = anchor < index ? [anchor, index] : [index, anchor];
			for (const c of visible.slice(a, b + 1)) {
				if (on) selected.add(c.id);
				else selected.delete(c.id);
			}
		} else if (on) selected.add(id);
		else selected.delete(id);
		anchor = index;
	}
	function toggleAll() {
		if (allSelected) selected.clear();
		else for (const c of visible) selected.add(c.id);
		anchor = null;
	}

	/** Runs `act` on every chosen card, then reloads once. */
	async function bulk(what: string, act: (c: CardSummary) => Promise<unknown>, list = chosen) {
		if (list.length === 0) return;
		busy = true;
		try {
			for (const c of list) await act(c);
			toast.info(`${what} ${list.length} card${list.length === 1 ? '' : 's'}`);
			selected.clear();
			cardModal.version++;
		} catch (e) {
			toast.error(errorMessage(e));
		} finally {
			busy = false;
		}
	}

	async function edit(id: string, patch: Partial<{ category_id: string; difficulty: Difficulty }>) {
		const card = await api.cardGet(id);
		if (card) await api.cardSave({ ...card, ...patch });
	}

	async function remove(list: CardSummary[]) {
		const what = list.length === 1 ? `"${list[0].title}"` : `${list.length} cards`;
		const ok = await confirm(`Delete ${what}? Review history is kept.`, {
			title: 'Delete',
			kind: 'warning'
		});
		if (ok) await bulk('Deleted', (c) => api.cardDelete(c.id), list);
	}

	async function exportPack() {
		const path = await save({
			title: 'Export pack',
			defaultPath: 'cards.indigopack',
			filters: [{ name: 'Indigo pack', extensions: ['indigopack'] }]
		});
		if (!path) return;
		const name = (path.split(/[\\/]/).pop() ?? 'Pack').replace(/\.indigopack$/, '');
		try {
			const n = await api.packExport(path, [...selected], name);
			toast.info(`Exported ${n} card${n === 1 ? '' : 's'} to ${name}`);
		} catch (e) {
			toast.error(errorMessage(e));
		}
	}

	function cram(ids: string[]) {
		session.cramCards(ids).catch((e) => toast.error(errorMessage(e)));
	}

	function due(c: CardSummary): string {
		if (c.state !== 'active') return c.state;
		if (!c.due_day) return 'new';
		const today = new Date().toISOString().slice(0, 10);
		return c.due_day <= today ? 'due' : `due ${c.due_day}`;
	}
</script>

<div class="mx-auto max-w-5xl space-y-5 pb-20">
	<div class="flex flex-wrap items-center gap-3">
		<div>
			<h1 class="page-title">Library</h1>
			<p class="page-sub">
				{cards.length} card{cards.length === 1 ? '' : 's'}{categories.length
					? ` in ${categories.length} categor${categories.length === 1 ? 'y' : 'ies'}`
					: ''}
			</p>
		</div>
		<a href="/settings#categories" class="btn gap-1.5 btn-ghost btn-sm"
			><Shapes class="size-4" />Categories</a
		>
		<a href="/settings#tags" class="btn gap-1.5 btn-ghost btn-sm"
			><Tags class="size-4" />Tags & filters</a
		>
		<label class="input ml-auto w-72 input-sm">
			<Search class="size-4 opacity-50" />
			<input type="search" placeholder="Search titles" bind:value={text} />
		</label>
	</div>

	<div class="flex flex-wrap items-center gap-2">
		<select class="select w-40 select-sm" bind:value={status} aria-label="Status">
			<option value="all">All cards</option>
			<option value="due">Due</option>
			<option value="active">Active</option>
			<option value="shelved">On the shelf</option>
			<option value="flagged">Weak step</option>
			<option value="suspended">Suspended</option>
		</select>
		<select class="select w-48 select-sm" bind:value={categoryId} aria-label="Category">
			<option value={null}>All categories</option>
			{#each categories as cat (cat.id)}<option value={cat.id}>{cat.name}</option>{/each}
		</select>
		<select class="select w-44 select-sm" bind:value={difficulty} aria-label="Difficulty">
			<option value={null}>Any difficulty</option>
			{#each DIFFICULTIES as d (d)}<option value={d}>{cap(d)}</option>{/each}
		</select>
		<select class="select w-40 select-sm" bind:value={tag} aria-label="Tag">
			<option value={null}>Any tag</option>
			{#each allTags as t (t)}<option value={t}>{t}</option>{/each}
		</select>
		{#if prefs.settings?.saved_filters.length}
			<select
				class="select w-44 select-sm"
				aria-label="Saved filter"
				bind:value={savedId}
				onchange={() => {
					applySaved(savedId);
					savedId = '';
				}}
			>
				<option value="">Saved filter…</option>
				{#each prefs.settings.saved_filters as f (f.id)}<option value={f.id}>{f.name}</option
					>{/each}
			</select>
		{/if}
		{#if filtered}
			<button
				type="button"
				class="btn btn-ghost btn-sm"
				onclick={() => {
					text = '';
					status = 'all';
					categoryId = null;
					difficulty = null;
					tag = null;
				}}>Clear filters</button
			>
		{/if}
	</div>

	{#if selected.size > 0}
		<!-- Floating, so selecting never shifts the rows you are about to shift-click. -->
		<div class="pointer-events-none fixed inset-x-0 bottom-6 z-30 flex justify-center pr-6 pl-22">
			<div
				class="pointer-events-auto flex flex-wrap items-center gap-1 rounded-box border border-base-300 bg-base-100 px-3 py-2 text-sm shadow-2xl"
				role="toolbar"
				aria-label="Selected cards"
			>
				<span class="mr-2 font-medium">{selected.size} selected</span>
				<button
					type="button"
					class="btn gap-1 btn-ghost btn-xs"
					disabled={busy}
					onclick={() => cram([...selected])}><Zap class="size-3.5" />Cram</button
				>
				<button
					type="button"
					class="btn gap-1 btn-ghost btn-xs"
					disabled={busy}
					title="Take active cards out of daily study"
					onclick={() =>
						bulk(
							'Suspended',
							(c) => api.cardSetState(c.id, 'suspended'),
							chosen.filter((c) => c.state === 'active')
						)}><Pause class="size-3.5" />Suspend</button
				>
				<button
					type="button"
					class="btn gap-1 btn-ghost btn-xs"
					disabled={busy}
					onclick={() =>
						bulk(
							'Resumed',
							(c) => api.cardSetState(c.id, 'active'),
							chosen.filter((c) => c.state === 'suspended')
						)}><Play class="size-3.5" />Resume</button
				>
				<select
					class="select w-32 select-xs"
					disabled={busy}
					aria-label="Move to category"
					onchange={(e) => {
						const id = e.currentTarget.value;
						e.currentTarget.value = '';
						if (id) bulk('Moved', (c) => edit(c.id, { category_id: id }));
					}}
				>
					<option value="">Move to…</option>
					{#each categories as cat (cat.id)}<option value={cat.id}>{cat.name}</option>{/each}
				</select>
				<select
					class="select w-32 select-xs"
					disabled={busy}
					aria-label="Set difficulty"
					onchange={(e) => {
						const d = e.currentTarget.value as Difficulty;
						e.currentTarget.value = '';
						if (d) bulk('Updated', (c) => edit(c.id, { difficulty: d }));
					}}
				>
					<option value="">Difficulty…</option>
					{#each DIFFICULTIES as d (d)}<option value={d}>{cap(d)}</option>{/each}
				</select>
				<button
					type="button"
					class="btn gap-1 btn-ghost btn-xs"
					disabled={busy}
					onclick={exportPack}><PackageOpen class="size-3.5" />Export</button
				>
				<button
					type="button"
					class="btn gap-1 btn-ghost text-error btn-xs"
					disabled={busy}
					onclick={() => remove(chosen)}><Trash2 class="size-3.5" />Delete</button
				>
				<button
					type="button"
					class="btn ml-1 btn-ghost btn-xs"
					onclick={() => {
						selected.clear();
						anchor = null;
					}}>Clear</button
				>
			</div>
		</div>
	{/if}

	{#if loading}
		<span class="loading loading-dots"></span>
	{:else if visible.length === 0}
		<div class="panel flex flex-col items-center gap-3 py-12 text-center">
			<FolderOpen class="size-8 opacity-40" />
			<p class="opacity-70">
				{filtered ? 'No cards match these filters.' : 'Your library is empty.'}
			</p>
			{#if !filtered}
				<button
					type="button"
					class="btn btn-primary btn-sm"
					onclick={() => cardModal.add().catch((e) => toast.error(errorMessage(e)))}
					>Add your first card</button
				>
			{/if}
		</div>
	{:else}
		<ul class="panel overflow-hidden p-0">
			<li class="flex items-center gap-3 border-b border-(--hairline) px-4 py-2 text-xs opacity-70">
				<input
					type="checkbox"
					class="checkbox checkbox-xs"
					checked={allSelected}
					onchange={toggleAll}
					aria-label="Select all"
				/>
				<span>{visible.length} card{visible.length === 1 ? '' : 's'}</span>
				<span class="hint ml-auto">Shift+click selects a range</span>
			</li>
			{#each visible as c, i (c.id)}
				<li
					class="group flex items-center gap-3 border-b border-(--hairline) px-4 py-2.5 last:border-b-0 hover:bg-base-200/60"
					class:bg-primary-5={selected.has(c.id)}
				>
					<input
						type="checkbox"
						class="checkbox checkbox-xs transition-opacity"
						class:opacity-0={selected.size === 0}
						class:group-hover:opacity-100={selected.size === 0}
						class:focus:opacity-100={selected.size === 0}
						checked={selected.has(c.id)}
						onclick={(e) => pick(e, i)}
						aria-label="Select {c.title}"
					/>
					<span
						class="size-2.5 shrink-0 rounded-full"
						style:background={c.category_color ?? FALLBACK_COLOR}
						title={c.category_name}
					></span>
					<button
						type="button"
						class="min-w-0 flex-1 truncate text-left font-medium hover:text-primary"
						onclick={() => cardModal.edit(c.id).catch((e) => toast.error(errorMessage(e)))}
						>{c.title}</button
					>
					<span class="hidden text-xs opacity-60 sm:inline">{c.category_name}</span>
					{#if c.step_count > 1}
						<span class="badge badge-ghost badge-sm">{c.step_count} steps</span>
					{/if}
					{#if c.flagged}<span class="badge badge-sm badge-warning">weak step</span>{/if}
					<span class="w-24 text-right text-xs opacity-70">{due(c)}</span>
					<button
						type="button"
						class="btn btn-square btn-ghost btn-xs"
						title="Edit card"
						aria-label="Edit {c.title}"
						onclick={() => cardModal.edit(c.id).catch((e) => toast.error(errorMessage(e)))}
						><Pencil class="size-3.5" /></button
					>
					<div class="flex gap-1 opacity-0 transition-opacity group-hover:opacity-100">
						<button
							type="button"
							class="btn btn-square btn-ghost btn-xs"
							title="Study now"
							aria-label="Study {c.title} now"
							onclick={() => goto(`/study?card=${c.id}`)}><Play class="size-3.5" /></button
						>
						<button
							type="button"
							class="btn btn-square btn-ghost text-error btn-xs"
							title="Delete"
							aria-label="Delete {c.title}"
							onclick={() => remove([c])}><Trash2 class="size-3.5" /></button
						>
					</div>
				</li>
			{/each}
		</ul>
	{/if}
</div>

<style>
	.bg-primary-5 {
		background: color-mix(in oklab, var(--color-primary) 8%, transparent);
	}
</style>
