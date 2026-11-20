<script lang="ts">
	// Today (SPEC §14, 0.2 dashboard): one primary action (Study), Cram as the secondary, then
	// progress at a glance. Counts come from the plan; everything else from stats_overview.
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { confirm } from '@tauri-apps/plugin-dialog';
	import { Zap } from '@lucide/svelte';
	import { api, errorMessage, type Category, type SessionPlan, type StatsOverview } from '#lib/api';
	import Bars from '#lib/components/charts/Bars.svelte';
	import { cardModal } from '#lib/cardModal.svelte';
	import { duration } from '#lib/duration';
	import { session } from '#lib/study/session.svelte';
	import { toast } from '#lib/toast.svelte';

	const CRAM_DEFAULT = 20;

	let plan = $state<SessionPlan | null>(null);
	let stats = $state<StatsOverview | null>(null);
	let categories = $state<Category[]>([]);
	let loading = $state(true);
	let cramOpen = $state(false);
	let cramCategory = $state('');
	let cramCount = $state(CRAM_DEFAULT);

	async function load() {
		loading = true;
		try {
			plan = await api.sessionBuild(false);
			api
				.statsOverview()
				.then((s) => (stats = s))
				.catch(() => {});
			api
				.categoriesList()
				.then((c) => (categories = c))
				.catch(() => {});
		} catch (e) {
			toast.error(errorMessage(e));
		} finally {
			loading = false;
		}
	}
	onMount(load);
	$effect(() => {
		if (cardModal.version) load();
	});

	const counts = $derived.by(() => {
		if (!plan) return '';
		const c = plan.counts;
		const parts = [
			c.quick ? `${c.quick} quick` : '',
			c.projects ? `${c.projects} project${c.projects === 1 ? '' : 's'}` : '',
			c.new ? `${c.new} new` : '',
			c.drills ? `${c.drills} drill${c.drills === 1 ? '' : 's'}` : ''
		].filter(Boolean);
		if (plan.items.length) parts.push(`~${c.minutes} min`);
		return parts.join(' · ');
	});

	const hour = new Date().getHours();
	const greeting = hour < 12 ? 'Good morning' : hour < 18 ? 'Good afternoon' : 'Good evening';
	const doneToday = $derived(stats?.heatmap.at(-1)?.count ?? 0);
	const left = $derived(plan?.items.length ?? 0);
	// Paused attempts are still today's work, so the ring counts them too.
	const todo = $derived(left + (plan?.continue_items.length ?? 0));
	const progress = $derived(doneToday + todo ? doneToday / (doneToday + todo) : 0);
	const totalCards = $derived(stats?.categories.reduce((s, c) => s + c.total, 0) ?? 0);
	const catMax = $derived(Math.max(1, ...(stats?.categories.map((c) => c.total) ?? [])));
	const week = $derived(
		stats?.forecast.slice(0, 7).map((d, i) => ({
			label:
				i === 0
					? 'today'
					: new Date(d.day + 'T00:00:00').toLocaleDateString(undefined, { weekday: 'short' }),
			value: d.count
		})) ?? []
	);

	function study() {
		if (!plan) return;
		session.start(plan);
		goto('/study');
	}

	async function cram() {
		try {
			const cards = await api.cardsQuery(cramCategory ? { category_id: cramCategory } : {});
			const ids = cards.filter((c) => c.state !== 'suspended').map((c) => c.id);
			for (let i = ids.length - 1; i > 0; i--) {
				const j = Math.floor(Math.random() * (i + 1));
				[ids[i], ids[j]] = [ids[j], ids[i]];
			}
			await session.cramCards(ids.slice(0, Math.max(1, cramCount)));
		} catch (e) {
			toast.error(errorMessage(e));
		}
	}

	async function discard(id: string) {
		if (
			!(await confirm('Discard this attempt? No review is written.', { title: 'Discard attempt' }))
		)
			return;
		try {
			await api.attemptDiscard(id);
			await load();
		} catch (e) {
			toast.error(errorMessage(e));
		}
	}

	async function restart(id: string) {
		try {
			const v = await api.attemptRestart(id);
			await goto(`/study?attempt=${v.attempt.id}`);
		} catch (e) {
			toast.error(errorMessage(e));
		}
	}
</script>

<div class="mx-auto max-w-6xl space-y-6">
	<section class="panel flex flex-wrap items-center gap-6 p-6">
		<div
			class="radial-progress shrink-0 bg-base-200 text-primary"
			style:--value={Math.round(progress * 100)}
			style:--size="5.5rem"
			style:--thickness="0.5rem"
			role="progressbar"
			aria-valuenow={Math.round(progress * 100)}
			aria-label="Done today"
		>
			<span class="font-display text-lg text-base-content">{doneToday}/{doneToday + todo}</span>
		</div>
		<div class="min-w-0 flex-1">
			<h1 class="page-title">{greeting}</h1>
			{#if loading}
				<span class="loading loading-dots"></span>
			{:else if left > 0}
				<p class="opacity-70">{counts}</p>
				{#if plan && plan.more_due > 0}
					<p class="hint">{plan.more_due} more due beyond today's budget</p>
				{/if}
			{:else}
				<p class="opacity-70">Nothing due right now.</p>
			{/if}
		</div>
		<div class="flex flex-wrap items-center gap-2">
			{#if left > 0}
				<button type="button" class="btn btn-lg btn-primary" onclick={study}>Study</button>
			{:else if totalCards === 0 && !loading}
				<button
					type="button"
					class="btn btn-primary"
					onclick={() => cardModal.add().catch((e) => toast.error(errorMessage(e)))}
					>Add a card</button
				>
			{/if}
			{#if totalCards > 0}
				<button
					type="button"
					class="btn"
					class:btn-lg={left > 0}
					aria-expanded={cramOpen}
					title="Practise any cards; due dates stay as they are"
					onclick={() => (cramOpen = !cramOpen)}><Zap class="size-4" />Cram</button
				>
			{/if}
		</div>
		{#if cramOpen}
			<div class="flex w-full flex-wrap items-end gap-3 border-t border-base-300 pt-4">
				<label class="flex items-center gap-2">
					<span class="label-text">Category</span>
					<select class="select w-52 select-sm" bind:value={cramCategory}>
						<option value="">All categories</option>
						{#each categories as c (c.id)}
							<option value={c.id}>{c.name}</option>
						{/each}
					</select>
				</label>
				<label class="flex items-center gap-2">
					<span class="label-text">Cards</span>
					<input
						type="number"
						min="1"
						max="500"
						class="input w-24 input-sm"
						bind:value={cramCount}
					/>
				</label>
				<button type="button" class="btn btn-primary btn-sm" onclick={cram}>Start cram</button>
				<span class="hint"
					>Random cards, due or not. Due dates and weak-step flags stay as they are.</span
				>
			</div>
		{/if}
	</section>

	{#if stats}
		<div class="stat-tiles">
			<div class="stat-tile">
				<div class="stat-num" class:text-primary={(stats.forecast[0]?.count ?? 0) > 0}>
					{stats.forecast[0]?.count ?? 0}
				</div>
				<div class="stat-label">Due today</div>
			</div>
			<div class="stat-tile">
				<div class="stat-num">{totalCards}</div>
				<div class="stat-label">Total cards</div>
			</div>
			<div class="stat-tile">
				<div class="stat-num">{stats.streak_days}</div>
				<div class="stat-label">Day streak · longest {stats.longest_streak_days}</div>
			</div>
			<div class="stat-tile">
				<div class="stat-num">{duration(stats.time_today_ms)}</div>
				<div class="stat-label">Studied today</div>
			</div>
		</div>

		{#if stats.categories.length}
			<div class="grid gap-6 md:grid-cols-2">
				<section class="panel p-5">
					<h2 class="mb-3 text-lg">Categories</h2>
					<ul class="space-y-2">
						{#each stats.categories as c (c.name)}
							<li class="grid grid-cols-[minmax(0,10rem)_1fr_auto] items-center gap-3 text-sm">
								<span class="flex min-w-0 items-center gap-2">
									<span
										class="size-2.5 shrink-0 rounded-full bg-base-content/30"
										style:background-color={c.color}
									></span>
									<span class="truncate">{c.name}</span>
								</span>
								<span class="relative h-2 rounded-full bg-base-200">
									<span
										class="absolute inset-y-0 left-0 rounded-full bg-base-content/20"
										style:width="{(c.total / catMax) * 100}%"
									></span>
									<span
										class="absolute inset-y-0 left-0 rounded-full bg-primary"
										style:width="{(c.due / catMax) * 100}%"
										style:background-color={c.color}
									></span>
								</span>
								<span class="tabular-nums opacity-70" title="due / total">{c.due}/{c.total}</span>
							</li>
						{/each}
					</ul>
				</section>
				<section class="panel p-5">
					<h2 class="mb-3 text-lg">Next 7 days</h2>
					<Bars data={week} />
				</section>
			</div>
		{/if}
	{/if}

	{#if plan && plan.continue_items.length > 0}
		<section class="panel p-5">
			<h2 class="text-lg">Continue</h2>
			<ul class="divide-y divide-base-300">
				{#each plan.continue_items as c (c.attempt_id)}
					<li class="flex flex-wrap items-center gap-2 py-2">
						<div class="flex-1">
							<div class="font-medium">{c.title}</div>
							<div class="text-xs opacity-60">
								{c.steps_done}/{c.steps_total} steps · started {new Date(
									c.started_at
								).toLocaleDateString()}
								{#if c.stale}<span class="ml-1 badge badge-xs badge-warning"
										>paused a while ago</span
									>{/if}
							</div>
						</div>
						<a href={`/study?attempt=${c.attempt_id}`} class="btn btn-sm">Resume</a>
						{#if c.stale}
							<button
								type="button"
								class="btn btn-ghost btn-sm"
								onclick={() => restart(c.attempt_id)}>Restart</button
							>
							<button
								type="button"
								class="btn btn-ghost text-error btn-sm"
								onclick={() => discard(c.attempt_id)}>Discard</button
							>
						{/if}
					</li>
				{/each}
			</ul>
		</section>
	{/if}
</div>
