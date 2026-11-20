<script lang="ts">
	import '@fontsource-variable/noto-sans';
	import '@fontsource-variable/fraunces';
	import './layout.css';
	import favicon from '#lib/assets/favicon.svg';
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import CardModal from '#lib/components/CardModal.svelte';
	import KeyHelp from '#lib/components/KeyHelp.svelte';
	import Sidebar from '#lib/components/Sidebar.svelte';
	import { prefs } from '#lib/prefs.svelte';
	import { syncStore } from '#lib/sync.svelte';
	import { toast } from '#lib/toast.svelte';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();
	const studying = $derived(page.url.pathname.startsWith('/study'));
	onMount(() => {
		prefs.load().catch(() => {});
		syncStore.init().catch(() => {});
	});
	$effect(() => {
		document.documentElement.classList.toggle('minimal', prefs.settings?.minimal_ui ?? false);
	});
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
</svelte:head>

<div class="min-h-screen bg-base-200 text-base-content">
	{#if !studying}
		<Sidebar />
	{/if}
	<!-- Pages cap their own width (forms max-w-3xl, overviews max-w-6xl) so nothing stretches. -->
	<main class="min-h-screen px-6 py-8" class:pl-22={!studying}>
		{@render children()}
	</main>
</div>

<CardModal />
<KeyHelp />

<div class="toast toast-end z-[1000]">
	{#each toast.items as t (t.id)}
		<div
			class="alert text-sm"
			class:alert-error={t.kind === 'error'}
			class:alert-warning={t.kind === 'warning'}
			class:alert-info={t.kind === 'info'}
			role="status"
		>
			<span>{t.text}</span>
			<button type="button" class="btn btn-ghost btn-xs" onclick={() => toast.dismiss(t.id)}
				>✕</button
			>
		</div>
	{/each}
</div>
