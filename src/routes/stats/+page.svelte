<script lang="ts">
	// Stats (SPEC §14, owner-expanded): everything comes from one stats_overview call.
	import { onMount } from 'svelte';
	import { api, errorMessage, type StatsOverview } from '#lib/api';
	import type { NamedStat } from '#lib/api/types/NamedStat';
	import Bars from '#lib/components/charts/Bars.svelte';
	import GradeMix from '#lib/components/charts/GradeMix.svelte';
	import Heatmap from '#lib/components/charts/Heatmap.svelte';
	import { cardModal } from '#lib/cardModal.svelte';
	import { duration } from '#lib/duration';
	import { toast } from '#lib/toast.svelte';

	let stats = $state<StatsOverview | null>(null);

	onMount(async () => {
		try {
			stats = await api.statsOverview();
		} catch (e) {
			toast.error(errorMessage(e));
		}
	});

	const tables = $derived<[string, NamedStat[]][]>(
		stats
			? [
					['By category', stats.time_by_category],
					['By difficulty', stats.time_by_difficulty]
				]
			: []
	);
	const forecast = $derived(
		stats?.forecast.map((d, i) => ({
			label: i === 0 ? 'today' : d.day.slice(8),
			value: d.count
		})) ?? []
	);
	const projection = $derived(
		stats?.projection_weeks.map((d, i) => ({
			label: i === 0 ? 'this wk' : `+${i}w`,
			value: d.count
		})) ?? []
	);

	function min(x: number): string {
		return x < 1 ? `${Math.round(x * 60)} s` : `${x.toFixed(1)} min`;
	}
	function pct(x: number): string {
		return `${Math.round(x * 100)}%`;
	}
	function delta(a: number, b: number, unit: 'pct' | 'min'): string {
		const d = b - a;
		if (Math.abs(d) < 1e-9) return '±0';
		const s = unit === 'pct' ? `${Math.round(d * 100)} pts` : min(Math.abs(d));
		return (d > 0 ? '+' : '−') + s.replace(/^[+−]/, '');
	}
	function trend(t: number): string {
		if (Math.abs(t) < 0.05) return '→ steady';
		return t < 0 ? `↓ ${min(-t)} faster` : `↑ ${min(t)} slower`;
	}
</script>

<div class="mx-auto max-w-6xl space-y-6">
	<div>
		<h1 class="page-title">Stats</h1>
		<p class="page-sub">Everything here comes from your own reviews, in this app.</p>
	</div>

	{#if !stats}
		<span class="loading loading-dots"></span>
	{:else}
		<div class="stat-tiles">
			<div class="stat-tile">
				<div class="stat-num">{duration(stats.time_total_ms)}</div>
				<div class="stat-label">Time studied, all time</div>
			</div>
			<div class="stat-tile">
				<div class="stat-num">{stats.reviews_total}</div>
				<div class="stat-label">Reviews (drills and cram included)</div>
			</div>
			<div class="stat-tile">
				<div class="stat-num">{stats.streak_days}</div>
				<div class="stat-label">Day streak</div>
			</div>
			<div class="stat-tile">
				<div class="stat-num">{stats.longest_streak_days}</div>
				<div class="stat-label">Longest streak</div>
			</div>
			<div class="stat-tile">
				<div class="stat-num">{stats.matured_now}</div>
				<div class="stat-label">Mature cards · {stats.matured_30d_ago} a month ago</div>
			</div>
		</div>

		<section class="card bg-base-100 shadow-sm">
			<div class="card-body">
				<h2 class="card-title text-lg">Last year</h2>
				<Heatmap days={stats.heatmap} />
			</div>
		</section>

		<div class="grid gap-6 md:grid-cols-2">
			<section class="card bg-base-100 shadow-sm">
				<div class="card-body">
					<h2 class="card-title text-lg">Due, next 14 days</h2>
					<Bars data={forecast} labelEvery={2} />
				</div>
			</section>
			<section class="card bg-base-100 shadow-sm">
				<div class="card-body">
					<h2 class="card-title text-lg">Reviews per week, next 8 weeks</h2>
					<Bars data={projection} />
					<p class="text-xs opacity-60">
						From today's due dates; a review moves its card further out.
					</p>
				</div>
			</section>
		</div>

		<section class="card bg-base-100 shadow-sm">
			<div class="card-body">
				<h2 class="card-title text-lg">Am I improving?</h2>
				{#if stats.improvement.cards === 0}
					<p class="opacity-70">Review a card twice and this fills in.</p>
				{:else}
					<p class="text-sm opacity-70">
						Median over {stats.improvement.cards} card{stats.improvement.cards === 1 ? '' : 's'} with
						two or more reviews.
					</p>
					<div class="stats stats-vertical sm:stats-horizontal">
						<div class="stat">
							<div class="stat-title">Score, first → latest</div>
							<div class="stat-value text-2xl">
								{pct(stats.improvement.first_score)} → {pct(stats.improvement.latest_score)}
							</div>
							<div class="stat-desc">
								{delta(stats.improvement.first_score, stats.improvement.latest_score, 'pct')}
							</div>
						</div>
						<div class="stat">
							<div class="stat-title">Time per card, first → latest</div>
							<div class="stat-value text-2xl">
								{min(stats.improvement.first_minutes)} → {min(stats.improvement.latest_minutes)}
							</div>
							<div class="stat-desc">
								{delta(stats.improvement.first_minutes, stats.improvement.latest_minutes, 'min')}
							</div>
						</div>
					</div>
				{/if}
			</div>
		</section>

		<section class="card bg-base-100 shadow-sm">
			<div class="card-body">
				<h2 class="card-title text-lg">Grades per week</h2>
				<GradeMix weeks={stats.grade_mix} />
			</div>
		</section>

		<div class="grid gap-6 md:grid-cols-2">
			{#each tables as [title, rows] (title)}
				<section class="card bg-base-100 shadow-sm">
					<div class="card-body">
						<h2 class="card-title text-lg">Time per card · {title}</h2>
						{#if rows.length === 0}
							<p class="opacity-70">No measured reviews yet.</p>
						{:else}
							<table class="table table-sm">
								<thead
									><tr
										><th>Name</th><th class="text-right">Avg</th><th>Last 30 days</th><th
											class="text-right">Reviews</th
										></tr
									></thead
								>
								<tbody>
									{#each rows as r (r.name)}
										<tr
											><td>{r.name}</td><td class="text-right">{min(r.minutes)}</td><td
												class="text-xs opacity-80">{trend(r.trend)}</td
											><td class="text-right">{r.reviews}</td></tr
										>
									{/each}
								</tbody>
							</table>
						{/if}
					</div>
				</section>
			{/each}
		</div>

		<div class="grid gap-6 md:grid-cols-2">
			<section class="card bg-base-100 shadow-sm">
				<div class="card-body">
					<h2 class="card-title text-lg">Weak steps</h2>
					{#if stats.flagged.length === 0}
						<p class="opacity-70">None flagged.</p>
					{:else}
						<ul class="space-y-1 text-sm">
							{#each stats.flagged as f (f.card_id + f.step_index)}
								<li class="flex items-center gap-2">
									<button
										type="button"
										class="flex-1 link text-left"
										onclick={() =>
											cardModal.edit(f.card_id).catch((e) => toast.error(errorMessage(e)))}
										>{f.title}</button
									>
									<span class="opacity-70">step {f.step_index + 1}</span>
									<span class="badge badge-sm badge-warning">flagged ×{f.flag_count}</span>
									{#if f.flag_count >= 4}<span class="text-xs opacity-70">rewrite or split?</span
										>{/if}
								</li>
							{/each}
						</ul>
					{/if}
				</div>
			</section>
			<section class="card bg-base-100 shadow-sm">
				<div class="card-body">
					<h2 class="card-title text-lg">Categories</h2>
					<table class="table table-sm">
						<thead
							><tr
								><th>Category</th><th class="text-right">Total</th><th class="text-right">Active</th
								><th class="text-right">Shelf</th><th class="text-right">Due</th></tr
							></thead
						>
						<tbody>
							{#each stats.categories as c (c.name)}
								<tr>
									<td
										><span
											class="mr-2 inline-block size-2 rounded-full"
											style:background={c.color ?? 'oklch(70% 0.1 280)'}
										></span>{c.name}</td
									>
									<td class="text-right">{c.total}</td><td class="text-right">{c.active}</td><td
										class="text-right">{c.shelved}</td
									><td class="text-right">{c.due}</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			</section>
		</div>
	{/if}
</div>
