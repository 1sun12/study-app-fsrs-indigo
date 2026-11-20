<script lang="ts">
	// Links are the only interactive thing in card markdown: confirm, then open in the system
	// browser (SPEC §13). Everything else is sanitized by renderMarkdown. Fenced code is then
	// highlighted by Shiki, which builds its output from escaped text.
	import { confirm } from '#lib/confirm.svelte';
	import { openUrl } from '@tauri-apps/plugin-opener';
	import { renderMarkdown } from '#lib/markdown';
	import { toast } from '#lib/toast.svelte';

	let { source, lang = null }: { source: string; lang?: string | null } = $props();
	const html = $derived(renderMarkdown(source));
	let root = $state<HTMLDivElement>();

	$effect(() => {
		void html;
		const blocks = [...(root?.querySelectorAll('pre > code') ?? [])];
		if (!blocks.length) return;
		let live = true;
		import('shiki').then(async ({ codeToHtml, bundledLanguages }) => {
			for (const code of blocks) {
				const fence = /language-(\S+)/.exec(code.className)?.[1] ?? lang ?? 'text';
				const out = await codeToHtml(code.textContent ?? '', {
					lang: fence in bundledLanguages ? fence : 'text',
					themes: { light: 'github-light', dark: 'github-dark' },
					defaultColor: false
				});
				const pre = code.parentElement;
				const tpl = document.createElement('template');
				tpl.innerHTML = out;
				const shiki = tpl.content.firstElementChild;
				// Update the existing <pre> in place so Svelte's {@html} nodes stay its own.
				if (!live || !pre || !shiki) continue;
				pre.className = shiki.className;
				pre.setAttribute('style', shiki.getAttribute('style') ?? '');
				pre.innerHTML = shiki.innerHTML;
			}
		});
		return () => {
			live = false;
		};
	});

	async function onclick(e: MouseEvent) {
		const a = (e.target as HTMLElement | null)?.closest('a[href]');
		if (!a) return;
		e.preventDefault();
		const href = a.getAttribute('href') ?? '';
		if (!/^https?:\/\//i.test(href)) return;
		if (
			await confirm(`Open in your browser?\n\n${href}`, { title: 'Open link', okLabel: 'Open' })
		) {
			openUrl(href).catch((err) => toast.error(String(err)));
		}
	}
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<!-- eslint-disable-next-line svelte/no-at-html-tags -- sanitized by DOMPurify in renderMarkdown -->
<div class="md" bind:this={root} {onclick}>{@html html}</div>
