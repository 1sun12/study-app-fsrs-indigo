<script lang="ts">
	// Settings (SPEC §14): every field has a default and a one-line explanation. Plain settings
	// autosave; scheduling constants replay every card, so they ask first (§7.6).
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { confirm, open, save } from '@tauri-apps/plugin-dialog';
	import {
		api,
		errorMessage,
		type ConflictView,
		type PackPreview,
		type SchedConfig,
		type Settings
	} from '#lib/api';
	import CategoriesManager from '#lib/components/CategoriesManager.svelte';
	import PackImport from '#lib/components/PackImport.svelte';
	import TagsManager from '#lib/components/TagsManager.svelte';
	import { prefs } from '#lib/prefs.svelte';
	import { syncStore } from '#lib/sync.svelte';
	import { toast } from '#lib/toast.svelte';

	let settings = $state<Settings | null>(null);
	let sched = $state<SchedConfig | null>(null);
	let capsText = $state('');
	let busy = $state(false);
	let pack = $state<{ path: string; preview: PackPreview } | null>(null);
	let conflicts = $state<ConflictView[]>([]);
	let savedSched = $state('');
	let saved = $state(false);
	let preview = $state<number[]>([]);

	const groups: [string, string[]][] = [
		['Study', ['Study', 'Cards']],
		['Library', ['Categories', 'Tags & filters']],
		['App', ['Appearance', 'Editor']],
		['Data', ['Sync', 'Data', 'Advanced']]
	];
	const HASHES: Record<string, string> = {
		'#sync': 'Sync',
		'#categories': 'Categories',
		'#tags': 'Tags & filters'
	};
	let section = $state('Study');
	// The rail's sync dot links here with #sync.
	$effect(() => {
		if (HASHES[page.url.hash]) section = HASHES[page.url.hash];
	});

	onMount(async () => {
		try {
			[settings, sched] = await Promise.all([api.settingsGet(), api.schedConfigGet()]);
			capsText = sched.streak_caps.join(', ');
			// f32 from the backend (0.8999…) vs the slider's 0.01 steps: compare on the slider's terms.
			sched.desired_retention = Math.round(sched.desired_retention * 100) / 100;
			savedSched = JSON.stringify(sched);
			conflicts = await api.conflictsList();
		} catch (e) {
			toast.error(errorMessage(e));
		}
	});

	// Autosave: any change to `settings` is written after a short pause.
	let saveTimer: ReturnType<typeof setTimeout> | undefined;
	let lastSaved = '';
	$effect(() => {
		if (!settings) return;
		const json = JSON.stringify(settings);
		if (!lastSaved) {
			lastSaved = json;
			return;
		}
		if (json === lastSaved) return;
		clearTimeout(saveTimer);
		saveTimer = setTimeout(async () => {
			try {
				await prefs.save(JSON.parse(json) as Settings);
				lastSaved = json;
				saved = true;
				setTimeout(() => (saved = false), 1500);
			} catch (e) {
				toast.error(errorMessage(e));
			}
		}, 400);
	});

	const schedDirty = $derived(
		!!sched && (JSON.stringify(sched) !== savedSched || capsText !== sched.streak_caps.join(', '))
	);

	// Live preview of what the retention slider means, debounced.
	let previewTimer: ReturnType<typeof setTimeout> | undefined;
	$effect(() => {
		const r = sched?.desired_retention;
		if (r === undefined) return;
		clearTimeout(previewTimer);
		previewTimer = setTimeout(() => {
			api
				.schedPreview(Number(r))
				.then((p) => (preview = p))
				.catch(() => (preview = []));
		}, 150);
	});

	async function saveSched() {
		if (!sched) return;
		const caps = capsText
			.split(',')
			.map((s) => parseInt(s.trim(), 10))
			.filter((n) => Number.isFinite(n) && n > 0);
		const ok = await confirm(
			'Saving scheduling constants recomputes every card’s due date from its review history. Continue?',
			{ title: 'Replay all cards' }
		);
		if (!ok) return;
		busy = true;
		try {
			const next = { ...($state.snapshot(sched) as SchedConfig), streak_caps: caps };
			await api.schedConfigSet(next);
			sched = next;
			capsText = caps.join(', ');
			savedSched = JSON.stringify(next);
			toast.info('Scheduling saved and all cards replayed');
		} catch (e) {
			toast.error(errorMessage(e));
		} finally {
			busy = false;
		}
	}

	const plural = (n: number, w: string) => `${n} ${w}${n === 1 ? '' : 's'}`;

	async function exportBackup() {
		const path = await save({
			title: 'Export backup',
			defaultPath: `indigo-backup-${new Date().toISOString().slice(0, 10)}.zip`,
			filters: [{ name: 'Indigo backup', extensions: ['zip'] }]
		});
		if (!path) return;
		busy = true;
		try {
			const r = await api.backupExport(path);
			toast.info(`Backed up ${plural(r.cards, 'card')}, ${plural(r.reviews, 'review')}`);
			if (r.missing_media)
				toast.warning(`${plural(r.missing_media, 'media file')} missing on disk`);
		} catch (e) {
			toast.error(errorMessage(e));
		} finally {
			busy = false;
		}
	}

	async function importBackup() {
		const path = await open({
			title: 'Import backup',
			filters: [{ name: 'Indigo backup', extensions: ['zip'] }]
		});
		if (!path) return;
		const ok = await confirm(
			'Merge this backup into your library? Nothing here is deleted; where both have a card, the newer edit wins and the other is kept under Conflicts.',
			{ title: 'Import backup' }
		);
		if (!ok) return;
		busy = true;
		try {
			const r = await api.backupImport(path);
			toast.info(`Imported ${plural(r.cards, 'card')} and ${plural(r.reviews, 'review')}`);
			for (const s of r.skipped.slice(0, 3)) toast.warning(`Skipped: ${s}`);
			if (r.skipped.length > 3) toast.warning(`…and ${r.skipped.length - 3} more skipped`);
			[settings, sched] = await Promise.all([api.settingsGet(), api.schedConfigGet()]);
		} catch (e) {
			toast.error(errorMessage(e));
		} finally {
			busy = false;
		}
	}

	async function importPack() {
		const path = await open({
			title: 'Import pack',
			filters: [{ name: 'Indigo pack', extensions: ['indigopack'] }]
		});
		if (!path) return;
		try {
			pack = { path, preview: await api.packInspect(path) };
		} catch (e) {
			toast.error(errorMessage(e));
		}
	}

	// Conflicts arrive with syncs; refresh the list whenever one finishes.
	$effect(() => {
		if (syncStore.view && !syncStore.view.syncing)
			api
				.conflictsList()
				.then((c) => (conflicts = c))
				.catch(() => {});
	});

	async function resolve(id: string, restore: boolean) {
		try {
			await (restore ? api.conflictRestore(id) : api.conflictDismiss(id));
			conflicts = conflicts.filter((c) => c.id !== id);
			await syncStore.init();
			if (restore) syncStore.now();
		} catch (e) {
			toast.error(errorMessage(e));
		}
	}

	async function cleanup() {
		const ok = await confirm('Delete media files no card refers to? This cannot be undone.', {
			title: 'Clean up media',
			kind: 'warning'
		});
		if (!ok) return;
		try {
			const r = await api.mediaCleanup();
			toast.info(
				`Removed ${r.removed_files} file${r.removed_files === 1 ? '' : 's'} (${(r.freed_bytes / 1048576).toFixed(1)} MB)`
			);
		} catch (e) {
			toast.error(errorMessage(e));
		}
	}
</script>

<div class="mx-auto max-w-4xl">
	<div class="page-head flex items-end gap-3">
		<div>
			<h1 class="page-title">Settings</h1>
			<p class="page-sub">Changes save as you make them.</p>
		</div>
		<span class="pb-0.5 text-sm text-success transition-opacity" class:opacity-0={!saved}
			>Saved</span
		>
	</div>

	{#if !settings || !sched}
		<span class="loading loading-dots"></span>
	{:else}
		<div class="flex flex-col gap-6 sm:flex-row">
			<nav class="sm:w-40 sm:shrink-0" aria-label="Settings sections">
				<ul class="menu w-full gap-0.5 p-0">
					{#each groups as [group, names] (group)}
						<li class="menu-title px-3 pt-3 pb-1 text-xs">{group}</li>
						{#each names as name (name)}
							<li>
								<button
									type="button"
									class:bg-primary-10={section === name}
									class:text-primary={section === name}
									class:font-medium={section === name}
									onclick={() => (section = name)}>{name}</button
								>
							</li>
						{/each}
					{/each}
				</ul>
			</nav>

			<section class="panel max-w-3xl min-w-0 flex-1 space-y-4 self-start p-5">
				<h2 class="text-lg">{section}</h2>
				{#if section === 'Study'}
					<label class="flex flex-col items-start gap-1">
						<span class="label-text">Minutes per day</span>
						<input
							type="number"
							min="5"
							max="600"
							class="input w-32"
							bind:value={settings.daily_minutes}
						/>
						<span class="hint">Soft budget for the session builder (default 30).</span>
					</label>
					<label class="flex flex-col items-start gap-1">
						<span class="label-text">New cards per day</span>
						<input
							type="number"
							min="0"
							max="500"
							class="input w-32"
							bind:value={settings.new_per_day}
						/>
						<span class="hint">Shelf cards introduced each day (default 20).</span>
					</label>
					<div class="divider my-1"></div>
					<label class="flex flex-col items-start gap-1">
						<span class="label-text"
							>Desired retention: {Math.round(sched.desired_retention * 100)}%</span
						>
						<input
							type="range"
							min="0.40"
							max="0.99"
							step="0.01"
							class="range w-72 range-sm"
							bind:value={sched.desired_retention}
						/>
						<span class="hint"
							>How likely you are to still know a card when it comes back. Higher means more
							frequent reviews (default 90%).</span
						>
					</label>
					{#if preview.length}
						<p class="text-sm">
							At {Math.round(sched.desired_retention * 100)}%, a card you keep knowing returns in
							{preview.slice(0, 5).join(', ')} days…
						</p>
					{/if}
					{#if sched.desired_retention < 0.7}
						<p class="text-sm text-warning">
							Below 70% most cards soon reach the maximum interval and you will forget many of them.
						</p>
					{:else if sched.desired_retention > 0.95}
						<p class="text-sm text-warning">
							Above 95% cards come back very often; expect many more reviews each day.
						</p>
					{/if}
					<label class="flex flex-col items-start gap-1">
						<span class="label-text">Maximum interval, days</span>
						<input
							type="number"
							min="7"
							max="3650"
							class="input w-32"
							bind:value={sched.max_interval_days}
						/>
						<span class="hint">No card waits longer than this (default 365).</span>
					</label>
				{:else if section === 'Cards'}
					<label class="label cursor-pointer justify-start gap-3">
						<input type="checkbox" class="toggle" bind:checked={settings.card_defaults.enabled} />
						<span>Remember fields for the next card</span>
					</label>
					<p class="hint">
						A new card starts with the category, difficulty, answer type, tags and source of the
						last card you added. Off: every new card starts blank.
					</p>
					<div class="divider my-1"></div>
					<h3 class="font-semibold">Media size limits</h3>
					<p class="hint">
						Largest file you can attach, after video and audio are compressed. Sync caps every file
						at 95 MB.
					</p>
					<div class="flex flex-wrap gap-4">
						{#each [['image_mb', 'Images'], ['audio_mb', 'Audio'], ['video_mb', 'Video']] as [key, label] (key)}
							<label class="flex flex-col items-start gap-1">
								<span class="label-text">{label}, MB</span>
								<input
									type="number"
									min="1"
									max="95"
									class="input w-24"
									bind:value={settings.media_limits[key as 'image_mb' | 'audio_mb' | 'video_mb']}
								/>
							</label>
						{/each}
					</div>
				{:else if section === 'Categories'}
					<CategoriesManager />
				{:else if section === 'Tags & filters'}
					<TagsManager bind:settings />
				{:else if section === 'Appearance'}
					<div class="segmented flex w-fit" role="radiogroup" aria-label="Theme">
						{#each [['system', 'Follow the OS'], ['light', 'Light'], ['dark', 'Dark']] as [value, label] (value)}
							<button
								type="button"
								role="radio"
								aria-checked={settings.theme === value}
								onclick={() => settings && (settings.theme = value as Settings['theme'])}
								>{label}</button
							>
						{/each}
					</div>
					<label class="label cursor-pointer justify-start gap-3">
						<input type="checkbox" class="toggle" bind:checked={settings.minimal_ui} />
						<span>Minimal UI</span>
					</label>
					<p class="hint">
						Keeps the side rail collapsed and hides helper text like this line. Tooltips stay.
					</p>
				{:else if section === 'Editor'}
					<label class="label cursor-pointer justify-start gap-3">
						<input type="checkbox" class="toggle" bind:checked={settings.editor.vim} />
						<span>Vim keys in every editor (prompts, answers, code)</span>
					</label>
					<label class="flex flex-col items-start gap-1">
						<span class="label-text">Tab width</span>
						<input
							type="number"
							min="1"
							max="8"
							class="input w-24"
							bind:value={settings.editor.tab_width}
						/>
						<span class="hint">Spaces per indent (default 4).</span>
					</label>
				{:else if section === 'Sync'}
					<p class="hint">
						Keeps two computers in step through a private GitHub repository. Create an empty private
						repo, sign git in to GitHub once (<code>gh auth login</code>, then
						<code>gh auth setup-git</code>), and paste the repo's URL here. Indigo never sees your
						password or token. See SYNC.md.
					</p>
					<div class="flex flex-wrap items-end gap-3">
						<label class="flex w-full max-w-md min-w-0 flex-col items-start gap-1">
							<span class="label-text">Private repository URL</span>
							<input
								class="input w-full"
								placeholder="https://github.com/you/indigo-vault.git"
								spellcheck="false"
								bind:value={settings.sync.repo_url}
							/>
						</label>
						<button
							type="button"
							class="btn btn-sm"
							disabled={!settings.sync.repo_url.trim() || syncStore.view?.syncing}
							onclick={async () => {
								await prefs.save($state.snapshot(settings) as Settings);
								await syncStore.init();
								syncStore.now();
							}}>Sync now</button
						>
					</div>
					{#if syncStore.view?.configured}
						<p class="text-sm">
							{#if syncStore.view.syncing}
								Syncing…
							{:else if syncStore.view.status.last_success_at}
								Last synced {new Date(syncStore.view.status.last_success_at).toLocaleString()}.
							{:else}
								Not synced yet.
							{/if}
							{#if syncStore.view.status.message}
								<span
									class="block whitespace-pre-line"
									class:text-warning={syncStore.view.status.failed}
									>{syncStore.view.status.message}</span
								>
							{/if}
						</p>
					{/if}
					{#if conflicts.length}
						<div>
							<h3 class="font-semibold">Conflicts</h3>
							<p class="text-sm opacity-70">
								A card was edited on two devices; the newer edit won. Restore brings the other
								version back; Dismiss forgets it.
							</p>
							<ul class="mt-1 divide-y divide-base-300">
								{#each conflicts as c (c.id)}
									<li class="flex items-center gap-2 py-1 text-sm">
										<span class="flex-1">{c.title}</span>
										<span class="opacity-60">{new Date(c.detected_at).toLocaleDateString()}</span>
										<button type="button" class="btn btn-xs" onclick={() => resolve(c.id, true)}
											>Restore</button
										>
										<button
											type="button"
											class="btn btn-ghost btn-xs"
											onclick={() => resolve(c.id, false)}>Dismiss</button
										>
									</li>
								{/each}
							</ul>
						</div>
					{/if}
				{:else if section === 'Data'}
					<div class="flex flex-wrap gap-2">
						<button type="button" class="btn btn-sm" onclick={exportBackup} disabled={busy}
							>Export backup</button
						>
						<button type="button" class="btn btn-sm" onclick={importBackup} disabled={busy}
							>Import backup</button
						>
						<button type="button" class="btn btn-sm" onclick={importPack} disabled={busy}
							>Import pack</button
						>
					</div>
					<p class="hint">
						A backup holds every card, review, media file and setting. Importing merges; it never
						deletes. Packs share cards without history; export one from the Library.
					</p>
					<div>
						<button type="button" class="btn btn-sm" onclick={cleanup}>Clean up media</button>
						<span class="hint ml-2">removes files no card refers to; never runs on its own</span>
					</div>
				{:else}
					<p class="hint">
						SPEC §7 defaults. Changing them replays every card and syncs to your other devices.
					</p>
					<div class="grid gap-3 sm:grid-cols-2">
						<label class="flex flex-col items-start gap-1"
							><span class="label-text"
								>Day rollover hour <span class="opacity-60">(default 4)</span></span
							><input
								type="number"
								min="0"
								max="23"
								class="input input-sm"
								bind:value={sched.rollover_hour}
							/></label
						>
						<label class="flex flex-col items-start gap-1"
							><span class="label-text"
								>Minimum interval, days <span class="opacity-60">(default 1)</span></span
							><input
								type="number"
								min="1"
								class="input input-sm"
								bind:value={sched.min_interval_days}
							/></label
						>
						<label class="flex flex-col items-start gap-1"
							><span class="label-text"
								>Early-growth caps, days <span class="opacity-60"
									>(default 7, 21, 60; empty = off)</span
								></span
							><input class="input input-sm" bind:value={capsText} /></label
						>
						{#each ['Forgot', 'Hard', 'Good', 'Easy'] as name, i (name)}
							<label class="flex flex-col items-start gap-1"
								><span class="label-text"
									>Stats score for {name}
									<span class="opacity-60">(default {[0, 0.7, 0.85, 1][i]})</span></span
								><input
									type="number"
									min="0"
									max="1"
									step="0.01"
									class="input input-sm"
									bind:value={sched.grade4_scores[i]}
								/></label
							>
						{/each}
						<label class="flex flex-col items-start gap-1"
							><span class="label-text"
								>Flag window <span class="opacity-60">· recent scores kept per step (3)</span></span
							><input
								type="number"
								min="1"
								max="10"
								class="input input-sm"
								bind:value={sched.flag_window}
							/></label
						>
						<label class="flex flex-col items-start gap-1"
							><span class="label-text"
								>Flag when this many are below <span class="opacity-60">(2)</span></span
							><input
								type="number"
								min="1"
								max="10"
								class="input input-sm"
								bind:value={sched.flag_low_count}
							/></label
						>
						<label class="flex flex-col items-start gap-1"
							><span class="label-text">…this score <span class="opacity-60">(0.60)</span></span
							><input
								type="number"
								min="0"
								max="1"
								step="0.01"
								class="input input-sm"
								bind:value={sched.flag_low_score}
							/></label
						>
						<label class="flex flex-col items-start gap-1"
							><span class="label-text"
								>Unflag when the last N <span class="opacity-60">(2)</span></span
							><input
								type="number"
								min="1"
								max="10"
								class="input input-sm"
								bind:value={sched.unflag_high_count}
							/></label
						>
						<label class="flex flex-col items-start gap-1"
							><span class="label-text">…are at least <span class="opacity-60">(0.80)</span></span
							><input
								type="number"
								min="0"
								max="1"
								step="0.01"
								class="input input-sm"
								bind:value={sched.unflag_high_score}
							/></label
						>
					</div>
				{/if}

				{#if schedDirty && (section === 'Study' || section === 'Advanced')}
					<div class="flex items-center gap-3 border-t border-base-300 pt-4">
						<button type="button" class="btn btn-primary btn-sm" disabled={busy} onclick={saveSched}
							>Apply and replay</button
						>
						<span class="hint">Scheduling changes recompute every card’s due date.</span>
					</div>
				{/if}
			</section>
		</div>
	{/if}
</div>

{#if pack}
	<PackImport path={pack.path} preview={pack.preview} onclose={() => (pack = null)} />
{/if}
