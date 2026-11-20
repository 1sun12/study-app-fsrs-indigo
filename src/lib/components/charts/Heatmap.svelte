<script lang="ts">
	// GitHub-style calendar: one column per week, Monday at the top.
	import type { DayCount } from '#lib/api/types/DayCount';

	let { days, label = 'Reviews per day' }: { days: DayCount[]; label?: string } = $props();
	const cell = 11;
	const gap = 2;

	const cells = $derived.by(() => {
		if (!days.length) return [];
		const first = new Date(days[0].day + 'T00:00:00');
		const offset = (first.getDay() + 6) % 7;
		return days.map((d, i) => ({ ...d, col: Math.floor((i + offset) / 7), row: (i + offset) % 7 }));
	});
	const weeks = $derived(cells.length ? cells[cells.length - 1].col + 1 : 0);
	const max = $derived(Math.max(1, ...days.map((d) => d.count)));
	const opacity = [0.08, 0.3, 0.5, 0.75, 1];
	function level(c: number) {
		if (!c) return 0;
		const r = c / max;
		return r < 0.25 ? 1 : r < 0.5 ? 2 : r < 0.75 ? 3 : 4;
	}
</script>

<svg
	viewBox={`0 0 ${weeks * (cell + gap)} ${7 * (cell + gap)}`}
	class="w-full"
	style:max-height="110px"
	role="img"
	aria-label={label}
>
	{#each cells as c (c.day)}
		<rect
			x={c.col * (cell + gap)}
			y={c.row * (cell + gap)}
			width={cell}
			height={cell}
			rx="2"
			class="fill-primary"
			style:opacity={opacity[level(c.count)]}
		>
			<title>{c.day}: {c.count}</title>
		</rect>
	{/each}
</svg>
