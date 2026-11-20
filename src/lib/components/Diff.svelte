<script lang="ts">
	// Typed answer vs reference, line by line. An aid only; it never grades (SPEC §4.1).
	import { diffLines } from 'diff';

	let { answer, reference }: { answer: string; reference: string } = $props();
	const parts = $derived(diffLines(answer, reference));
</script>

<pre class="rounded bg-base-300 p-3 text-sm whitespace-pre-wrap">{#each parts as part, i (i)}<span
			class:text-success={part.added}
			class:text-error={part.removed}
			class:line-through={part.removed}>{part.value}</span
		>{/each}</pre>
