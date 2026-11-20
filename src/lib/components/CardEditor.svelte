<script lang="ts">
	// One form for add and edit (SPEC §9.1), shown in the card modal. The common path is
	// Title → Prompt → Answer → Ctrl+Enter; everything else is one click away. Warnings never
	// block (§9.2).
	import { goto } from '$app/navigation';
	import { confirm } from '@tauri-apps/plugin-dialog';
	import { dndzone, type DndEvent } from 'svelte-dnd-action';
	import { Pause, Play, Trash2, TriangleAlert, X } from '@lucide/svelte';
	import { api, errorMessage, type Card, type CardState, type Category } from '#lib/api';
	import type { Step } from '#lib/api/types/Step';
	import { blankStep, cardModal } from '#lib/cardModal.svelte';
	import { prefs } from '#lib/prefs.svelte';
	import { toast } from '#lib/toast.svelte';
	import CategoryPicker from './CategoryPicker.svelte';
	import StepEditor from './StepEditor.svelte';

	let { card: initial, categories: initialCategories }: { card: Card; categories: Category[] } =
		$props();

	// Props seed local editing state on purpose; the modal re-keys the editor per open.
	// svelte-ignore state_referenced_locally
	let card = $state<Card>($state.snapshot(initial) as Card);
	// svelte-ignore state_referenced_locally
	let categories = $state<Category[]>(initialCategories);
	// svelte-ignore state_referenced_locally
	let tagsText = $state(initial.tags.join(', '));
	let saving = $state(false);
	let warnings = $state<string[]>([]);
	let dragDisabled = $state(true);
	let form = $state<HTMLFormElement>();
	// svelte-ignore state_referenced_locally
	let more = $state(!!(initial.tags.length || initial.est_minutes || initial.source));

	const isNew = $derived(!card.id);

	// Tags already in use (or created in Settings → Tags & filters), offered as one-click chips.
	let knownTags = $state<string[]>([]);
	api
		.cardsQuery()
		.then((cards) => {
			knownTags = [
				...new Set([...cards.flatMap((c) => c.tags), ...(prefs.settings?.tags ?? [])])
			].sort();
		})
		.catch(() => {});
	const currentTags = $derived(
		tagsText
			.split(',')
			.map((t) => t.trim())
			.filter(Boolean)
	);
	const suggestions = $derived(knownTags.filter((t) => !currentTags.includes(t)).slice(0, 12));
	const syntax = $derived(categories.find((c) => c.id === card.category_id)?.syntax ?? null);
	if (card.steps.length === 0) card.steps = [blankStep()];

	function payload(): Card {
		const snap = $state.snapshot(card) as Card;
		return {
			...snap,
			tags: tagsText
				.split(',')
				.map((t) => t.trim())
				.filter(Boolean),
			steps: snap.steps.map((s) => ({ ...s, id: s.id.startsWith('tmp-') ? '' : s.id }))
		};
	}
	const pristine = JSON.stringify(payload());
	let dirty = $state(false);
	$effect(() => {
		dirty = JSON.stringify(payload()) !== pristine;
		cardModal.dirty = dirty;
	});

	async function save(next = false) {
		if (saving) return;
		saving = true;
		try {
			await api.cardSave(payload(), isNew);
			toast.info(isNew ? 'Added to your shelf' : 'Saved');
			if (next && isNew) await cardModal.next();
			else cardModal.changed();
		} catch (e) {
			toast.error(errorMessage(e));
		} finally {
			saving = false;
		}
	}

	async function remove() {
		const ok = await confirm('Delete this card? Its review history is kept.', {
			title: 'Delete card',
			kind: 'warning'
		});
		if (!ok) return;
		try {
			await api.cardDelete(card.id);
			cardModal.changed();
		} catch (e) {
			toast.error(errorMessage(e));
		}
	}

	async function setState(state: CardState) {
		try {
			await api.cardSetState(card.id, state);
			card.state = state;
			cardModal.version++;
			toast.info(state === 'suspended' ? 'Suspended: out of daily study' : 'Back in daily study');
		} catch (e) {
			toast.error(errorMessage(e));
		}
	}

	function addStep() {
		card.steps = [...card.steps, blankStep(card.steps.at(-1)?.response ?? 'recall')];
	}
	function removeStep(i: number) {
		card.steps = card.steps.filter((_, j) => j !== i);
	}
	function onDnd(e: CustomEvent<DndEvent<Step>>, final: boolean) {
		card.steps = e.detail.items;
		if (final) dragDisabled = true;
	}

	// Live guard rails, debounced; shown as one quiet line.
	$effect(() => {
		const p = payload();
		const t = setTimeout(() => {
			api
				.cardWarnings(p)
				.then((w) => (warnings = w))
				.catch(() => {});
		}, 600);
		return () => clearTimeout(t);
	});

	function onkeydown(e: KeyboardEvent) {
		if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
			e.preventDefault();
			save(e.shiftKey);
		}
	}
</script>

<svelte:window {onkeydown} />

<form
	bind:this={form}
	class="flex flex-col gap-5"
	onsubmit={(e) => {
		e.preventDefault();
		save();
	}}
>
	<div class="flex items-center gap-2">
		<h2 class="text-sm font-medium tracking-wide uppercase opacity-60">
			{isNew ? 'New card' : 'Edit card'}
		</h2>
		{#if !isNew}
			<span class="badge badge-ghost badge-sm">{card.state}</span>
		{/if}
		<div class="ml-auto flex items-center gap-1">
			{#if !isNew}
				{#if card.state === 'shelved'}
					<button
						type="button"
						class="btn gap-1 btn-ghost btn-sm"
						onclick={() => {
							cardModal.close();
							goto(`/study?card=${card.id}`);
						}}><Play class="size-4" />Study now</button
					>
				{:else if card.state === 'active'}
					<button
						type="button"
						class="btn gap-1 btn-ghost btn-sm"
						title="Take it out of daily study until you resume it"
						onclick={() => setState('suspended')}><Pause class="size-4" />Suspend</button
					>
				{:else}
					<button
						type="button"
						class="btn gap-1 btn-ghost btn-sm"
						onclick={() => setState('active')}><Play class="size-4" />Resume</button
					>
				{/if}
				<button
					type="button"
					class="btn btn-square btn-ghost text-error btn-sm"
					onclick={remove}
					title="Delete"
					aria-label="Delete card"><Trash2 class="size-4" /></button
				>
			{/if}
			<button
				type="button"
				class="btn btn-square btn-ghost btn-sm"
				onclick={() => cardModal.requestClose()}
				aria-label="Close"><X class="size-4" /></button
			>
		</div>
	</div>

	<!-- svelte-ignore a11y_autofocus -->
	<input
		class="font-display input w-full text-xl input-lg"
		placeholder="Title"
		bind:value={card.title}
		autofocus={isNew}
		onkeydown={(e) => {
			// Enter goes straight to the prompt: title, Enter, prompt, Ctrl+Enter.
			if (e.key === 'Enter' && !e.ctrlKey && !e.metaKey && !e.shiftKey) {
				e.preventDefault();
				form?.querySelector<HTMLElement>('textarea, .cm-content')?.focus();
			}
		}}
	/>

	<div class="flex flex-wrap items-center gap-2">
		<CategoryPicker bind:value={card.category_id} bind:categories />
		<select class="select w-40 select-sm" bind:value={card.difficulty} aria-label="Difficulty">
			<option value="beginner">Beginner</option>
			<option value="intermediate">Intermediate</option>
			<option value="advanced">Advanced</option>
			<option value="expert">Expert</option>
			<option value="master">Master</option>
		</select>
	</div>

	<div
		class="flex flex-col gap-3"
		use:dndzone={{ items: card.steps, dragDisabled, flipDurationMs: 150, dropTargetStyle: {} }}
		onconsider={(e) => onDnd(e, false)}
		onfinalize={(e) => onDnd(e, true)}
	>
		{#each card.steps as step, i (step.id)}
			<StepEditor
				bind:step={card.steps[i]}
				index={i}
				total={card.steps.length}
				{syntax}
				onremove={() => removeStep(i)}
				onhandle={() => (dragDisabled = false)}
			/>
		{/each}
	</div>

	<div class="flex flex-wrap gap-2">
		<button
			type="button"
			class="btn btn-ghost btn-sm"
			onclick={addStep}
			title="Split a bigger exercise into steps you grade separately">+ Add step</button
		>
		<button type="button" class="btn btn-ghost btn-sm" onclick={() => (more = !more)}
			>{more ? 'Fewer details' : 'Details'}</button
		>
	</div>

	{#if more}
		<div class="grid gap-3 rounded-field bg-base-200 p-3 sm:grid-cols-[1fr_8rem]">
			<label class="space-y-1">
				<span class="text-sm font-medium">Tags</span>
				<input class="input w-full input-sm" bind:value={tagsText} placeholder="comma, separated" />
				{#if suggestions.length}
					<span class="flex flex-wrap gap-1 pt-1">
						{#each suggestions as t (t)}
							<button
								type="button"
								class="badge cursor-pointer badge-ghost badge-sm hover:badge-primary"
								onclick={() => (tagsText = [...currentTags, t].join(', '))}>+ {t}</button
							>
						{/each}
					</span>
				{/if}
			</label>
			<label class="space-y-1">
				<span class="text-sm font-medium">Minutes</span>
				<input
					type="number"
					min="1"
					class="input w-full input-sm"
					bind:value={card.est_minutes}
					placeholder="auto"
					title="Expected minutes; learned from your reviews when empty"
				/>
			</label>
			<label class="space-y-1 sm:col-span-2">
				<span class="text-sm font-medium">Source</span>
				<input
					class="input w-full input-sm"
					bind:value={card.source}
					placeholder="Where this comes from (book, tutorial, URL)"
				/>
			</label>
		</div>
	{/if}

	{#if warnings.length && dirty}
		<p class="flex items-start gap-2 text-sm text-warning" title={warnings.join('\n')}>
			<TriangleAlert class="mt-0.5 size-4 shrink-0" />
			<span>{warnings[0]}{warnings.length > 1 ? ` (+${warnings.length - 1} more)` : ''}</span>
		</p>
	{/if}

	<div class="flex items-center justify-end gap-2 border-t border-base-300 pt-4">
		{#if isNew}
			<span class="hint mr-auto">Ctrl+Shift+↵ adds and starts the next card</span>
			<button type="button" class="btn btn-ghost" disabled={saving} onclick={() => save(true)}
				>Add &amp; next</button
			>
		{/if}
		<button type="submit" class="btn btn-primary" disabled={saving}
			>{isNew ? 'Add' : 'Save'} <kbd class="kbd kbd-xs">Ctrl ↵</kbd></button
		>
	</div>
</form>
