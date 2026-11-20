<script lang="ts">
	// Read-only code with Shiki (has a real GDScript grammar). Output is generated from
	// escaped text, so it is safe to inject.
	let { code, lang = null }: { code: string; lang?: string | null } = $props();
	let html = $state('');

	$effect(() => {
		const source = code;
		const wanted = lang ?? 'text';
		let live = true;
		import('shiki').then(async ({ codeToHtml, bundledLanguages }) => {
			const use = wanted in bundledLanguages ? wanted : 'text';
			const out = await codeToHtml(source, {
				lang: use,
				themes: { light: 'github-light', dark: 'github-dark' },
				defaultColor: false
			});
			if (live) html = out;
		});
		return () => {
			live = false;
		};
	});
</script>

{#if html}
	<!-- eslint-disable-next-line svelte/no-at-html-tags -- Shiki escapes all source text -->
	{@html html}
{:else}
	<pre class="rounded bg-base-300 p-3 text-sm whitespace-pre-wrap">{code}</pre>
{/if}
