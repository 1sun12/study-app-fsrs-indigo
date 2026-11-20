<script lang="ts">
	// Navigation rail (owner ruling 2026-11-12, Hearth-style): icons only until the pointer or
	// keyboard focus reaches it, then it expands over the content (no reflow). Minimal UI mode
	// keeps it collapsed for good. Add card sits bottom-left.
	import { page } from '$app/state';
	import { BarChart3, House, Library, Plus, Settings } from '@lucide/svelte';
	import { errorMessage } from '#lib/api';
	import { cardModal } from '#lib/cardModal.svelte';
	import { prefs } from '#lib/prefs.svelte';
	import { syncStore } from '#lib/sync.svelte';
	import { toast } from '#lib/toast.svelte';

	const OPEN_DELAY_MS = 120;
	const CLOSE_DELAY_MS = 250;

	const links = [
		{ href: '/', label: 'Today', icon: House },
		{ href: '/library', label: 'Library', icon: Library },
		{ href: '/stats', label: 'Stats', icon: BarChart3 },
		{ href: '/settings', label: 'Settings', icon: Settings }
	];
	const dot: Record<string, [string, string]> = {
		syncing: ['bg-info animate-pulse', 'Syncing…'],
		synced: ['bg-success', 'Synced'],
		waiting: ['bg-base-content/30', 'Not synced yet'],
		attention: ['bg-warning', 'Sync needs attention']
	};
	const sync = $derived(dot[syncStore.state]);
	const minimal = $derived(prefs.settings?.minimal_ui ?? false);

	let hover = $state(false);
	let focused = $state(false);
	let timer: ReturnType<typeof setTimeout> | undefined;
	const open = $derived(!minimal && (hover || focused));

	function enter() {
		clearTimeout(timer);
		timer = setTimeout(() => (hover = true), OPEN_DELAY_MS);
	}
	function leave() {
		clearTimeout(timer);
		timer = setTimeout(() => (hover = false), CLOSE_DELAY_MS);
	}
	const active = (href: string) =>
		href === '/' ? page.url.pathname === '/' : page.url.pathname.startsWith(href);
</script>

<aside
	class="rail fixed inset-y-0 left-0 z-40 flex flex-col py-3 transition-[width,box-shadow] duration-150 ease-out"
	class:w-16={!open}
	class:w-56={open}
	class:shadow-xl={open}
	onmouseenter={enter}
	onmouseleave={leave}
	onfocusin={() => (focused = true)}
	onfocusout={(e) => {
		if (!e.currentTarget.contains(e.relatedTarget as Node | null)) focused = false;
	}}
	aria-label="Main navigation"
>
	<a href="/" class="mb-5 flex h-10 items-center gap-3 px-4" aria-label="Indigo, Today">
		<span
			class="grid size-8 shrink-0 place-items-center rounded-lg bg-gradient-to-br from-primary to-secondary text-sm font-bold text-primary-content"
			>I</span
		>
		{#if open}<span class="font-display text-xl">Indigo</span>{/if}
	</a>

	<nav class="flex flex-col gap-1 px-2">
		{#each links as l (l.href)}
			<a
				href={l.href}
				class="flex h-10 items-center gap-3 rounded-field px-3 text-sm transition-colors hover:bg-base-200"
				class:bg-primary-10={active(l.href)}
				class:text-primary={active(l.href)}
				class:font-medium={active(l.href)}
				title={open ? undefined : l.label}
				aria-current={active(l.href) ? 'page' : undefined}
			>
				<l.icon class="size-5 shrink-0" />
				{#if open}<span class="truncate">{l.label}</span>{/if}
			</a>
		{/each}
	</nav>

	<div class="mt-auto flex flex-col gap-2 px-2">
		{#if sync}
			<a
				href="/settings#sync"
				class="flex h-9 items-center gap-3 rounded-field px-3 text-xs opacity-80 hover:bg-base-200"
				title={syncStore.view?.status.message ?? sync[1]}
			>
				<span class="grid size-5 shrink-0 place-items-center"
					><span class="block size-2.5 rounded-full {sync[0]}"></span></span
				>
				{#if open}<span class="truncate">{sync[1]}</span>{/if}
			</a>
		{/if}
		<button
			type="button"
			class="btn h-11 justify-start gap-3 rounded-field px-3 btn-primary"
			title={open ? undefined : 'Add card'}
			aria-label="Add card"
			onclick={() => cardModal.add().catch((e) => toast.error(errorMessage(e)))}
		>
			<Plus class="size-5 shrink-0" />
			{#if open}<span>Add card</span>{/if}
		</button>
	</div>
</aside>

<style>
	.rail {
		background: var(--surface-2);
		border-right: 1px solid var(--hairline);
	}
	.bg-primary-10 {
		background: color-mix(in oklab, var(--color-primary) 12%, transparent);
	}
</style>
