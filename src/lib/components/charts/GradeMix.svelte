<script lang="ts">
	// Stacked weekly grade counts: Forgot / Hard / Good / Easy.
	import type { WeekMix } from '#lib/api/types/WeekMix';

	let { weeks }: { weeks: WeekMix[] } = $props();
	const w = 28;
	const gap = 6;
	const height = 110;
	const plot = height - 24;
	const max = $derived(Math.max(1, ...weeks.map((x) => x.again + x.hard + x.good + x.easy)));
	// Same colours as the grade cards on the study screen.
	const parts: { key: keyof WeekMix; fill: string; label: string }[] = [
		{ key: 'again', fill: 'var(--grade-forgot)', label: 'Forgot' },
		{ key: 'hard', fill: 'var(--grade-hard)', label: 'Hard' },
		{ key: 'good', fill: 'var(--grade-good)', label: 'Good' },
		{ key: 'easy', fill: 'var(--grade-easy)', label: 'Easy' }
	];
	function label(d: string) {
		return d.slice(5);
	}
</script>

<svg
	viewBox={`0 0 ${weeks.length * (w + gap)} ${height}`}
	class="w-full"
	style:max-height="130px"
	role="img"
	aria-label="Grades per week"
>
	{#each weeks as wk, i (wk.week_start)}
		{@const total = wk.again + wk.hard + wk.good + wk.easy}
		{#if total === 0}
			<rect
				x={i * (w + gap)}
				y={plot - 2}
				width={w}
				height={2}
				class="fill-current"
				style:opacity="0.15"
			/>
		{/if}
		{#each parts as p, j (p.key)}
			{@const value = wk[p.key] as number}
			{@const below = parts.slice(0, j).reduce((s, q) => s + (wk[q.key] as number), 0)}
			{#if value}
				<rect
					x={i * (w + gap)}
					y={plot - ((below + value) / max) * plot}
					width={w}
					height={(value / max) * plot}
					style:fill={p.fill}
					style:opacity="0.85"
				/>
			{/if}
		{/each}
		<text
			x={i * (w + gap) + w / 2}
			y={height - 2}
			text-anchor="middle"
			class="fill-current text-[8px] opacity-70">{label(wk.week_start)}</text
		>
	{/each}
</svg>
<div class="mt-1 flex gap-3 text-xs opacity-80">
	{#each parts as p (p.key)}
		<span class="inline-flex items-center gap-1"
			><svg width="10" height="10"><rect width="10" height="10" rx="2" style:fill={p.fill} /></svg
			>{p.label}</span
		>
	{/each}
</div>
