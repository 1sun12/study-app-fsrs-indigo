<script lang="ts">
	// Study (SPEC §8.2, D-16, D-17): each step PROMPT → REVEAL → next step; after the last one the
	// user grades the whole card 1–4 (owner ruling 2026-11-14: their grade alone schedules it;
	// checklist ticks are reminders kept for Stats). Esc pauses. Every transition is persisted.
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import {
		api,
		errorMessage,
		type AttemptView,
		type Card,
		type Category,
		type Grade,
		type Summary
	} from '#lib/api';
	import { Pencil } from '@lucide/svelte';
	import { cardModal } from '#lib/cardModal.svelte';
	import { confirm } from '@tauri-apps/plugin-dialog';
	import type { PlanItem } from '#lib/api/types/PlanItem';
	import type { Step } from '#lib/api/types/Step';
	import CodeEditor from '#lib/components/CodeEditor.svelte';
	import Diff from '#lib/components/Diff.svelte';
	import Highlight from '#lib/components/Highlight.svelte';
	import Lightbox from '#lib/components/Lightbox.svelte';
	import Markdown from '#lib/components/Markdown.svelte';
	import Media from '#lib/components/Media.svelte';
	import { session } from '#lib/study/session.svelte';
	import { syncStore } from '#lib/sync.svelte';
	import { toast } from '#lib/toast.svelte';

	type Phase = 'loading' | 'prompt' | 'reveal' | 'summary' | 'done';

	let phase = $state<Phase>('loading');
	let view = $state<AttemptView | null>(null);
	let drill = $state<{ card: Card; step: Step; shownAt: number } | null>(null);
	let step = $state<Step | null>(null);
	let stepIndex = $state(0);
	let typed = $state('');
	let checked = $state<string[]>([]);
	let summary = $state<Summary | null>(null);
	let singleMode = $state(false);
	let categories = $state<Category[]>([]);
	let zoom = $state<{ url: string; caption: string | null } | null>(null);
	let showDiff = $state(false);
	let busy = $state(false);
	/** Days until the card returns per grade (Forgot, Hard, Good, Easy); null while unknown. */
	let preview = $state<number[] | null>(null);

	const GRADES: { grade: Grade; label: string; key: string; cls: string }[] = [
		{ grade: 'again', label: 'Forgot', key: '1', cls: 'forgot' },
		{ grade: 'hard', label: 'Hard', key: '2', cls: 'hard' },
		{ grade: 'good', label: 'Good', key: '3', cls: 'good' },
		{ grade: 'easy', label: 'Easy', key: '4', cls: 'easy' }
	];

	const card = $derived<Card | null>(drill?.card ?? view?.card ?? null);
	const syntax = $derived(categories.find((c) => c.id === card?.category_id)?.syntax ?? null);
	const total = $derived(drill ? 1 : (view?.card.steps.length ?? 0));
	const promptMedia = $derived(step?.media.filter((m) => m.role === 'prompt') ?? []);
	const solutionMedia = $derived(step?.media.filter((m) => m.role === 'solution') ?? []);
	const typedAnswer = $derived(view?.attempt.steps[stepIndex]?.typed_answer ?? typed);
	/** The card is graded once, after its last step (or a drill's one step). */
	const lastStep = $derived(drill !== null || stepIndex === total - 1);
	const stepDone = $derived(view?.attempt.steps[stepIndex]?.status === 'graded');

	onMount(async () => {
		api
			.categoriesList()
			.then((c) => (categories = c))
			.catch(() => {});
		const attemptId = page.url.searchParams.get('attempt');
		const cardId = page.url.searchParams.get('card');
		try {
			if (attemptId) {
				singleMode = true;
				await loadAttempt(await api.attemptGet(attemptId));
			} else if (cardId) {
				singleMode = true;
				await loadAttempt(await api.attemptStart(cardId));
			} else if (session.plan) {
				await nextItem();
			} else {
				await goto('/');
			}
		} catch (e) {
			toast.error(errorMessage(e));
			await goto('/');
		}
	});

	async function nextItem() {
		const item = session.current;
		if (!item) {
			phase = 'done';
			return;
		}
		if (item.kind === 'drill') await loadDrill(item);
		else await loadAttempt(await api.attemptStart(item.card_id, item.kind === 'cram'));
	}

	async function loadAttempt(v: AttemptView) {
		drill = null;
		view = v;
		preview = null;
		const i = v.attempt.steps.findIndex((s) => s.status !== 'graded');
		// Every step already done (resumed): straight to the card's grade.
		await showStep(i === -1 ? v.attempt.steps.length - 1 : i);
	}

	async function showStep(i: number) {
		if (!view) return;
		stepIndex = i;
		const as = view.attempt.steps[i];
		step = view.card.steps.find((s) => s.id === as.step_id) ?? null;
		typed = as.typed_answer ?? '';
		checked = [];
		showDiff = false;
		if (as.status === 'pending') {
			view = await api.attemptStepShow(view.attempt.id, as.step_id);
			phase = 'prompt';
		} else {
			phase = 'reveal';
		}
		if (i === view.attempt.steps.length - 1 && !view.attempt.cram && !preview)
			api
				.gradePreview(view.card.id)
				.then((p) => (preview = p))
				.catch(() => {});
	}

	async function loadDrill(item: PlanItem) {
		const c = await api.cardGet(item.card_id);
		const s = c?.steps.find((x) => x.id === item.step_id);
		if (!c || !s) {
			session.advance();
			await nextItem();
			return;
		}
		view = null;
		drill = { card: c, step: s, shownAt: Date.now() };
		step = s;
		stepIndex = 0;
		typed = '';
		checked = [];
		showDiff = false;
		phase = 'prompt';
	}

	async function reveal() {
		if (busy || !step) return;
		busy = true;
		try {
			if (view) {
				view = await api.attemptStepReveal(
					view.attempt.id,
					step.id,
					step.response === 'typed' ? typed : null
				);
			}
			phase = 'reveal';
		} catch (e) {
			toast.error(errorMessage(e));
		} finally {
			busy = false;
		}
	}

	/** Not the last step: this one is done, on to the next. */
	async function nextStep() {
		if (busy || !step || !view) return;
		busy = true;
		try {
			view = await api.attemptStepComplete(view.attempt.id, step.id, checked);
			await showStep(stepIndex + 1);
		} catch (e) {
			toast.error(errorMessage(e));
		} finally {
			busy = false;
		}
	}

	/** The user's own 1–4 for the whole card (or for a drilled step). */
	async function grade(g: Grade) {
		if (busy || !step) return;
		busy = true;
		try {
			if (drill) {
				await api.drillRecord(drill.card.id, drill.step.id, g, Date.now() - drill.shownAt);
				drill = null;
				session.advance();
				await nextItem();
			} else if (view) {
				if (!stepDone) view = await api.attemptStepComplete(view.attempt.id, step.id, checked);
				summary = await api.attemptFinish(view.attempt.id, g);
				syncStore.now();
				phase = 'summary';
			}
		} catch (e) {
			toast.error(errorMessage(e));
		} finally {
			busy = false;
		}
	}

	/** Edit the card being studied. Changed steps restart the attempt (its steps no longer match). */
	let editing = $state<{ version: number; steps: string } | null>(null);
	function editCard() {
		if (!card) return;
		const was = { version: cardModal.version, steps: card.steps.map((s) => s.id).join() };
		// Watch for the close only once the editor is open (it loads the card first).
		cardModal
			.edit(card.id)
			.then(() => (editing = was))
			.catch((e) => toast.error(errorMessage(e)));
	}
	$effect(() => {
		if (!editing || cardModal.card) return;
		const was = editing;
		editing = null;
		if (cardModal.version !== was.version) reloadAfterEdit(was.steps);
	});
	async function reloadAfterEdit(stepsBefore: string) {
		try {
			if (drill) {
				const c = await api.cardGet(drill.card.id);
				const s = c?.steps.find((x) => x.id === drill?.step.id);
				if (c && s) {
					drill = { ...drill, card: c, step: s };
					step = s;
				}
			} else if (view) {
				const v = await api.attemptGet(view.attempt.id);
				if (v.card.steps.map((s) => s.id).join() === stepsBefore) {
					view = v;
					step = v.card.steps.find((s) => s.id === step?.id) ?? step;
				} else if (
					await confirm('This card’s steps changed, so this attempt starts over.', {
						title: 'Restart attempt'
					})
				) {
					await loadAttempt(await api.attemptRestart(view.attempt.id));
				}
			}
		} catch (e) {
			toast.error(errorMessage(e));
		}
	}

	async function next() {
		summary = null;
		view = null;
		step = null;
		preview = null;
		if (singleMode) {
			await goto('/');
			return;
		}
		session.advance();
		await nextItem();
	}

	async function pause() {
		if (view && phase !== 'summary' && phase !== 'done') {
			try {
				await api.attemptPause(view.attempt.id);
			} catch (e) {
				toast.error(errorMessage(e));
			}
		}
		await goto('/');
	}

	async function keepGoing() {
		try {
			session.start(await api.sessionBuild(true));
			await nextItem();
		} catch (e) {
			toast.error(errorMessage(e));
		}
	}

	function toggle(id: string) {
		checked = checked.includes(id) ? checked.filter((c) => c !== id) : [...checked, id];
	}

	function onkeydown(e: KeyboardEvent) {
		if (cardModal.card) return; // the card editor is open and owns the keyboard
		if (e.key === 'Escape') {
			e.preventDefault();
			pause();
			return;
		}
		const inField = (e.target as HTMLElement | null)?.closest(
			'.cm-editor, input:not([type="checkbox"]), textarea, [contenteditable]'
		);
		if (inField) return;
		if (phase === 'reveal' && lastStep) {
			const g = GRADES.find((x) => x.key === e.key);
			if (g) {
				e.preventDefault();
				grade(g.grade);
				return;
			}
		}
		if (e.key === 'Enter') {
			if (phase === 'prompt' && step && step.response !== 'typed') {
				e.preventDefault();
				reveal();
			} else if (phase === 'reveal' && !lastStep) {
				e.preventDefault();
				nextStep();
			} else if (phase === 'summary') {
				e.preventDefault();
				next();
			}
		}
	}

	const cramming = $derived(view?.attempt.cram ?? false);
	const progress = $derived(
		session.plan && !singleMode ? session.done / Math.max(1, session.plan.items.length) : null
	);

	/** "tomorrow", "in 5 days", "in 3 weeks", "in 4 months". */
	function whenBack(days: number): string {
		if (days <= 0) return 'today';
		if (days === 1) return 'tomorrow';
		if (days < 14) return `in ${days} days`;
		if (days < 60) return `in ${Math.round(days / 7)} weeks`;
		if (days < 365) return `in ${Math.round(days / 30)} months`;
		const y = Math.round((days / 365) * 10) / 10;
		return `in ${y} year${y === 1 ? '' : 's'}`;
	}
</script>

<svelte:window {onkeydown} />

<div class="mx-auto max-w-4xl space-y-4">
	{#if progress !== null}
		<progress class="progress h-1 w-full progress-primary" value={progress} max="1"></progress>
	{/if}
	<header class="flex items-center gap-3 text-sm opacity-80">
		<span class="font-display text-lg text-base-content">{card?.title ?? ''}</span>
		{#if drill}<span class="badge badge-sm badge-info">drill</span>{/if}
		{#if cramming}<span
				class="badge badge-sm badge-secondary"
				title="Practice only: due dates and weak-step flags stay as they are">cram</span
			>{/if}
		{#if step && total > 1}<span>· step {stepIndex + 1} of {total}</span>{/if}
		{#if session.plan && !singleMode}<span class="ml-auto"
				>{session.done} / {session.plan.items.length} done</span
			>{/if}
		{#if card && phase !== 'done'}
			<button
				type="button"
				class="btn btn-ghost btn-sm {session.plan && !singleMode ? '' : 'ml-auto'}"
				onclick={editCard}
				title="Edit this card"><Pencil class="size-4" />Edit</button
			>
		{/if}
		<button type="button" class="btn btn-ghost btn-sm" onclick={pause}>
			Pause <kbd class="ml-1 kbd kbd-xs">Esc</kbd>
		</button>
	</header>

	{#if phase === 'loading'}
		<span class="loading loading-dots"></span>
	{:else if phase === 'done'}
		<div class="card bg-base-100 shadow-sm">
			<div class="card-body items-center text-center">
				<h2 class="card-title text-2xl">Session done</h2>
				<p class="opacity-70">{session.done} item{session.done === 1 ? '' : 's'} reviewed.</p>
				{#if session.plan && session.plan.more_due > 0}
					<button type="button" class="btn btn-primary" onclick={keepGoing}>
						{session.plan.more_due} more due — keep going?
					</button>
				{/if}
				<a href="/" class="btn btn-ghost" onclick={() => session.clear()}>Back to Today</a>
			</div>
		</div>
	{:else if phase === 'summary' && summary}
		<div class="card bg-base-100 shadow-sm">
			<div class="card-body">
				<h2 class="card-title text-2xl">
					{GRADES.find((g) => g.grade === summary?.grade)?.label}
				</h2>
				{#if summary.cram}
					<p>Cram run: due dates and weak-step flags are unchanged.</p>
				{:else}
					<p>
						Back <strong>{whenBack(summary.interval_days)}</strong> ({summary.due_day}); review #{summary.reps}.
					</p>
				{/if}
				{#if summary.flagged_step_ids.length}
					<p class="text-sm">
						<span class="badge badge-xs badge-warning">weak step</span>
						{summary.flagged_step_ids.length === 1 ? 'One step' : 'Some steps'} will come back as a drill.
					</p>
				{/if}
				<div class="mt-4 card-actions">
					<button type="button" class="btn btn-primary" onclick={next}>
						{singleMode ? 'Back to Today' : 'Next'} <kbd class="ml-1 kbd kbd-xs">↵</kbd>
					</button>
				</div>
			</div>
		</div>
	{:else if step}
		<section class="card bg-base-100 shadow-sm">
			<div class="card-body gap-3">
				<Markdown source={step.prompt_md} lang={syntax} />
				{#if promptMedia.length}
					<div class="flex flex-wrap gap-3">
						{#each promptMedia as m (m.hash)}
							<Media media={m} onzoom={(url, caption) => (zoom = { url, caption })} />
						{/each}
					</div>
				{/if}

				{#if phase === 'prompt'}
					{#if step.response === 'typed'}
						<CodeEditor bind:value={typed} {syntax} placeholder="Your answer" onsubmit={reveal} />
						<div class="card-actions">
							<button type="button" class="btn btn-primary" onclick={reveal} disabled={busy}>
								Submit <kbd class="ml-1 kbd kbd-xs">Ctrl</kbd><kbd class="kbd kbd-xs">↵</kbd>
							</button>
						</div>
					{:else}
						<p class="hint">Recall it (or do it), then reveal the answer.</p>
						<div class="card-actions">
							<button type="button" class="btn btn-primary" onclick={reveal} disabled={busy}>
								Reveal <kbd class="ml-1 kbd kbd-xs">↵</kbd>
							</button>
						</div>
					{/if}
				{/if}
			</div>
		</section>

		{#if phase === 'reveal'}
			<section class="grid gap-4" class:md:grid-cols-2={step.response === 'typed'}>
				{#if step.response === 'typed'}
					<div class="card bg-base-100 shadow-sm">
						<div class="card-body gap-2">
							<h3 class="text-sm font-semibold uppercase opacity-60">Your answer</h3>
							{#if showDiff && step.solution_text}
								<Diff answer={typedAnswer} reference={step.solution_text} />
							{:else}
								<Highlight code={typedAnswer} lang={syntax} />
							{/if}
							{#if step.solution_text}
								<label class="label cursor-pointer justify-start gap-2 text-sm">
									<input type="checkbox" class="toggle toggle-sm" bind:checked={showDiff} /> Show diff
								</label>
							{/if}
						</div>
					</div>
				{/if}
				<div class="card bg-base-100 shadow-sm">
					<div class="card-body gap-2">
						<h3 class="text-sm font-semibold uppercase opacity-60">Reference</h3>
						{#if step.solution_md.trim()}<Markdown source={step.solution_md} lang={syntax} />{/if}
						{#if !step.solution_md.trim() && !step.solution_text && solutionMedia.length === 0}
							<p class="text-sm opacity-60">No answer saved on this card; judge from memory.</p>
						{/if}
						{#if step.solution_text && step.response === 'typed'}
							<Highlight code={step.solution_text} lang={syntax} />
						{:else if step.solution_text}
							<pre
								class="rounded bg-base-300 p-3 text-sm whitespace-pre-wrap">{step.solution_text}</pre>
						{/if}
						{#if solutionMedia.length}
							<div class="flex flex-wrap gap-3">
								{#each solutionMedia as m (m.hash)}
									<Media media={m} onzoom={(url, caption) => (zoom = { url, caption })} />
								{/each}
							</div>
						{/if}
					</div>
				</div>
			</section>

			<section class="card bg-base-100 shadow-sm">
				<div class="card-body gap-3">
					{#if step.criteria.length}
						<h3 class="text-sm font-semibold uppercase opacity-60">Checklist</h3>
						<p class="hint">A reminder for you; ticks never decide the grade.</p>
						{#each step.criteria as c (c.id)}
							<label class="label cursor-pointer justify-start gap-3">
								<input
									type="checkbox"
									class="checkbox"
									checked={checked.includes(c.id)}
									disabled={stepDone}
									onchange={() => toggle(c.id)}
								/>
								<span>{c.text}</span>
							</label>
						{/each}
					{/if}
					{#if !lastStep}
						<div class="card-actions">
							<button type="button" class="btn btn-primary" onclick={nextStep} disabled={busy}>
								Next step <kbd class="ml-1 kbd kbd-xs">↵</kbd>
							</button>
						</div>
					{:else}
						<h3 class="text-sm font-semibold uppercase opacity-60">
							{drill ? 'How did this step go?' : 'How well did you know this card?'}
						</h3>
						{#if cramming || drill}<p class="hint">
								Practice only: your grade does not move the due date.
							</p>{/if}
						<div class="grade-cards">
							{#each GRADES as g, i (g.grade)}
								<button
									type="button"
									class="grade-card {g.cls}"
									onclick={() => grade(g.grade)}
									disabled={busy}
								>
									<span class="flex items-center gap-1.5"
										><span class="grade-key">{g.key}</span>{g.label}</span
									>
									<span class="grade-when"
										>{preview && !cramming && !drill ? whenBack(preview[i]) : ''}</span
									>
								</button>
							{/each}
						</div>
					{/if}
				</div>
			</section>
		{/if}
	{/if}
</div>

<Lightbox url={zoom?.url ?? null} caption={zoom?.caption ?? null} onclose={() => (zoom = null)} />
