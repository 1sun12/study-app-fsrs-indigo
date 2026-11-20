<script lang="ts">
	// One step: what to do (prompt), how you answer, what good looks like (answer), and an
	// optional checklist. Rarely used fields wait behind "More" so the common path stays short.
	import { Brain, ChevronDown, GripVertical, Keyboard, Trash2 } from '@lucide/svelte';
	import type { Step } from '#lib/api/types/Step';
	import AttachField from './AttachField.svelte';
	import CodeEditor from './CodeEditor.svelte';
	import { prefs } from '#lib/prefs.svelte';
	import CriteriaEditor from './CriteriaEditor.svelte';

	let {
		step = $bindable(),
		index,
		total,
		syntax,
		autofocus = false,
		onremove,
		onhandle
	}: {
		step: Step;
		index: number;
		total: number;
		syntax: string | null;
		autofocus?: boolean;
		onremove?: () => void;
		onhandle?: () => void;
	} = $props();

	// Owner ruling 2026-11-14: no "Do it" type; every card is graded 1–4 by the user.
	const RESPONSES = [
		{
			value: 'recall',
			label: 'Recall',
			icon: Brain,
			hint: 'Recall it (or do it in your own software), then reveal and compare'
		},
		{ value: 'typed', label: 'Type it', icon: Keyboard, hint: 'Type the answer, then compare' }
	] as const;

	const typed = $derived(step.response === 'typed');
	let open = $state(true);
	let more = $state(false);
	let checklist = $state(step.criteria.length > 0);
	let solutionText = $state(step.solution_text ?? '');
	$effect(() => {
		step.solution_text = solutionText.trim() ? solutionText : null;
	});
	const summary = $derived(step.prompt_md.split('\n')[0].trim() || 'Untitled step');
</script>

<div
	class:rounded-box={total > 1}
	class:border={total > 1}
	class:border-base-300={total > 1}
	class:p-4={total > 1}
>
	{#if total > 1}
		<div class="flex items-center gap-2" class:mb-3={open}>
			<button
				type="button"
				class="cursor-grab opacity-40 hover:opacity-100"
				aria-label="Drag to reorder"
				onmousedown={onhandle}
				ontouchstart={onhandle}><GripVertical class="size-4" /></button
			>
			<button
				type="button"
				class="flex flex-1 items-center gap-2 text-left text-sm"
				onclick={() => (open = !open)}
				aria-expanded={open}
			>
				<span class="font-semibold">Step {index + 1}</span>
				{#if !open}<span class="truncate opacity-60">{summary}</span>{/if}
				<ChevronDown class="ml-auto size-4 transition-transform {open ? 'rotate-180' : ''}" />
			</button>
			<button
				type="button"
				class="btn btn-square btn-ghost text-error btn-xs"
				onclick={onremove}
				aria-label="Remove step {index + 1}"><Trash2 class="size-3.5" /></button
			>
		</div>
	{/if}

	{#if open || total === 1}
		<div class="space-y-4">
			<AttachField
				bind:value={step.prompt_md}
				bind:media={step.media}
				role="prompt"
				label="Prompt"
				placeholder="What should you do, type or recall? Paste or drop images here."
				{autofocus}
			/>

			<div class="flex flex-wrap items-center gap-3">
				<div class="segmented" role="radiogroup" aria-label="How you answer">
					{#each RESPONSES as r (r.value)}
						<button
							type="button"
							role="radio"
							aria-checked={step.response === r.value}
							title={r.hint}
							onclick={() => (step.response = r.value)}><r.icon class="size-4" />{r.label}</button
						>
					{/each}
				</div>
				<span class="hint">{RESPONSES.find((r) => r.value === step.response)?.hint}</span>
			</div>

			{#if typed}
				<div class="space-y-2">
					<span class="text-sm font-medium">Reference answer</span>
					<CodeEditor bind:value={solutionText} {syntax} placeholder="The answer you expect" />
				</div>
			{:else}
				<AttachField
					bind:value={step.solution_md}
					bind:media={step.media}
					role="solution"
					label="Answer"
					placeholder="What a good result looks like. Paste a reference screenshot here."
				/>
			{/if}

			{#if checklist}
				<CriteriaEditor bind:criteria={step.criteria} />
			{/if}

			<div class="flex flex-wrap gap-2">
				{#if !checklist}
					<button
						type="button"
						class="btn btn-ghost btn-xs"
						title="Grade with yes/no checks instead of the four buttons"
						onclick={() => {
							checklist = true;
							if (step.criteria.length === 0) step.criteria = [{ id: '', text: '', weight: 1 }];
						}}>+ Checklist</button
					>
				{/if}
				<button type="button" class="btn btn-ghost btn-xs" onclick={() => (more = !more)}
					>{more ? 'Less' : 'More'}</button
				>
			</div>

			{#if more}
				<div class="space-y-4 rounded-field bg-base-200 p-3">
					{#if typed}
						<AttachField
							bind:value={step.solution_md}
							bind:media={step.media}
							role="solution"
							label="Notes shown with the answer"
							placeholder="Why it works, pitfalls"
						/>
					{:else}
						<label class="block space-y-1">
							<span class="text-sm font-medium">Reference text</span>
							{#if prefs.settings?.editor.vim}
								<CodeEditor
									bind:value={solutionText}
									prose
									placeholder="Exact values, shortcuts, settings"
								/>
							{:else}
								<textarea
									class="textarea w-full font-mono text-sm"
									rows="2"
									bind:value={solutionText}
									placeholder="Exact values, shortcuts, settings"></textarea>
							{/if}
						</label>
					{/if}
				</div>
			{/if}
		</div>
	{/if}
</div>
