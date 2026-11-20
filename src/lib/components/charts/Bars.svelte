<script lang="ts">
	// Plain vertical bars with a value on top; labels under every nth bar.
	let {
		data,
		labelEvery = 1,
		height = 90
	}: { data: { label: string; value: number }[]; labelEvery?: number; height?: number } = $props();
	const w = 28;
	const gap = 6;
	const max = $derived(Math.max(1, ...data.map((d) => d.value)));
	const plot = $derived(height - 28);
</script>

<svg
	viewBox={`0 0 ${data.length * (w + gap)} ${height}`}
	class="w-full"
	style:max-height="{height + 20}px"
	role="img"
>
	{#each data as d, i (d.label)}
		{@const h = (d.value / max) * plot}
		<rect
			x={i * (w + gap)}
			y={14 + plot - h}
			width={w}
			height={h}
			rx="3"
			class="fill-primary"
			style:opacity={d.value ? 0.85 : 0.15}
		/>
		{#if d.value}
			<text
				x={i * (w + gap) + w / 2}
				y={10 + plot - h}
				text-anchor="middle"
				class="fill-current text-[9px]">{d.value}</text
			>
		{/if}
		{#if i % labelEvery === 0}
			<text
				x={i * (w + gap) + w / 2}
				y={height - 2}
				text-anchor="middle"
				class="fill-current text-[8px] opacity-70">{d.label}</text
			>
		{/if}
	{/each}
</svg>
