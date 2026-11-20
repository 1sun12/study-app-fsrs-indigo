<script lang="ts">
	// A markdown text box that also takes images, audio and video: paste (Ctrl+V), drop, or the
	// paperclip. Pasting into the text box itself is what makes clipboard images work in
	// WebKitGTK, which only fires `paste` on editable targets.
	import { open } from '@tauri-apps/plugin-dialog';
	import { Paperclip, X } from '@lucide/svelte';
	import { api, errorMessage } from '#lib/api';
	import type { MediaRef } from '#lib/api/types/MediaRef';
	import type { MediaRole } from '#lib/api/types/MediaRole';
	import { MEDIA_EXTENSIONS } from '#lib/media';
	import { prefs } from '#lib/prefs.svelte';
	import CodeEditor from './CodeEditor.svelte';
	import { toast } from '#lib/toast.svelte';
	import Media from './Media.svelte';

	let {
		value = $bindable(''),
		media = $bindable([]),
		role,
		label,
		placeholder = '',
		autofocus = false
	}: {
		value: string;
		media: MediaRef[];
		role: MediaRole;
		label: string;
		placeholder?: string;
		autofocus?: boolean;
	} = $props();

	let busy = $state(false);
	let dragging = $state(false);
	let box: HTMLTextAreaElement | undefined = $state();
	const mine = $derived(media.filter((m) => m.role === role));
	/** Settings → Editor → Vim also covers these boxes (owner ruling 2026-11-14). */
	const vimMode = $derived(prefs.settings?.editor.vim ?? false);

	// Grow with the text, within limits, so short prompts stay compact.
	$effect(() => {
		void value;
		if (!box) return;
		box.style.height = 'auto';
		box.style.height = `${Math.min(box.scrollHeight + 2, 320)}px`;
	});
	$effect(() => {
		if (autofocus) box?.focus();
	});

	function add(info: { hash: string; ext: string; mime: string }) {
		if (media.some((m) => m.hash === info.hash && m.role === role)) return;
		media = [...media, { hash: info.hash, ext: info.ext, mime: info.mime, role, caption: null }];
	}

	async function importFiles(files: File[]) {
		busy = true;
		try {
			for (const f of files) add(await api.mediaImportBytes(new Uint8Array(await f.arrayBuffer())));
		} catch (e) {
			toast.error(errorMessage(e));
		} finally {
			busy = false;
		}
	}

	/** Files on the clipboard or in a drop: `items` carries pasted bitmaps, `files` copied files. */
	function filesOf(data: DataTransfer | null): File[] {
		const out: File[] = [];
		for (const item of data?.items ?? []) {
			if (item.kind === 'file') {
				const f = item.getAsFile();
				if (f) out.push(f);
			}
		}
		if (out.length === 0) out.push(...(data?.files ?? []));
		return out;
	}

	function onpaste(e: ClipboardEvent) {
		const data = e.clipboardData;
		const files = filesOf(data);
		if (files.length > 0) {
			e.preventDefault();
			importFiles(files);
			return;
		}
		const types = [...(data?.types ?? [])];
		// A file copied in a file manager arrives as a file:// URI list.
		const uris = types.includes('text/uri-list')
			? (data?.getData('text/uri-list') ?? '').split(/\r?\n/).filter((u) => u.startsWith('file://'))
			: [];
		if (uris.length > 0) {
			e.preventDefault();
			importPaths(uris.map((u) => decodeURIComponent(new URL(u).pathname)));
			return;
		}
		// WebKitGTK hands the page nothing for a clipboard image, so ask the backend once. The
		// normal paste still runs: text lands in the box, an image (if any) is attached.
		importClipboard();
	}

	async function importClipboard() {
		busy = true;
		try {
			const info = await api.mediaImportClipboard();
			if (info) add(info);
		} catch (e) {
			toast.error(errorMessage(e));
		} finally {
			busy = false;
		}
	}

	async function importPaths(paths: string[]) {
		busy = true;
		try {
			for (const p of paths) add(await api.mediaImportPath(p));
		} catch (e) {
			toast.error(errorMessage(e));
		} finally {
			busy = false;
		}
	}

	function ondrop(e: DragEvent) {
		dragging = false;
		const files = filesOf(e.dataTransfer);
		if (files.length === 0) return;
		e.preventDefault();
		importFiles(files);
	}

	async function pick() {
		const picked = await open({
			multiple: true,
			filters: [{ name: 'Media', extensions: MEDIA_EXTENSIONS }]
		});
		if (!picked) return;
		await importPaths(Array.isArray(picked) ? picked : [picked]);
	}

	function remove(hash: string) {
		media = media.filter((m) => !(m.hash === hash && m.role === role));
	}

	function setCaption(hash: string, caption: string) {
		media = media.map((m) =>
			m.hash === hash && m.role === role ? { ...m, caption: caption.trim() || null } : m
		);
	}
</script>

<div class="space-y-2">
	<div class="flex items-center justify-between">
		<span class="text-sm font-medium">{label}</span>
		<button
			type="button"
			class="btn gap-1 btn-ghost btn-xs"
			onclick={pick}
			disabled={busy}
			title="Attach an image, audio or video (or paste / drop it in the box)"
		>
			<Paperclip class="size-3.5" />{busy ? 'Adding… (videos are compressed first)' : 'Attach'}
		</button>
	</div>
	<div
		class="rounded-field transition-shadow"
		class:ring-2={dragging}
		class:ring-primary={dragging}
		role="group"
		ondragover={(e) => {
			e.preventDefault();
			dragging = true;
		}}
		ondragleave={() => (dragging = false)}
		{ondrop}
		onpaste={vimMode ? onpaste : undefined}
	>
		{#if vimMode}
			<!-- paste bubbles up from CodeMirror's editable area to the handler on this wrapper -->
			<CodeEditor bind:value prose {placeholder} {autofocus} />
		{:else}
			<textarea
				bind:this={box}
				bind:value
				{placeholder}
				{onpaste}
				rows="2"
				class="textarea w-full resize-none leading-relaxed"></textarea>
		{/if}
	</div>
	{#if mine.length}
		<div class="flex flex-wrap gap-3">
			{#each mine as m (m.hash)}
				<div class="group relative flex w-32 flex-col gap-1">
					<Media media={m} thumb />
					<button
						type="button"
						class="btn absolute top-1 right-1 btn-circle opacity-0 btn-xs group-focus-within:opacity-100 group-hover:opacity-100"
						onclick={() => remove(m.hash)}
						aria-label="Remove attachment"><X class="size-3" /></button
					>
					<input
						class="input input-xs"
						placeholder="Caption"
						value={m.caption ?? ''}
						oninput={(e) => setCaption(m.hash, e.currentTarget.value)}
					/>
				</div>
			{/each}
		</div>
	{/if}
</div>
