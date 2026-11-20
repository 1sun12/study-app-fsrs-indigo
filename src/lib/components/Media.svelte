<script lang="ts">
	import type { MediaRef } from '#lib/api/types/MediaRef';
	import { mediaBase, mediaKind, mediaUrl } from '#lib/media';

	let {
		media,
		onzoom,
		thumb = false
	}: {
		media: MediaRef;
		onzoom?: (url: string, caption: string | null) => void;
		/** Small tile for editors; no caption, no player chrome beyond controls. */
		thumb?: boolean;
	} = $props();
	let url = $state('');
	const kind = $derived(mediaKind(media.mime));

	$effect(() => {
		let live = true;
		mediaBase().then((b) => {
			if (live) url = mediaUrl(b, media);
		});
		return () => {
			live = false;
		};
	});
</script>

{#if url}
	<figure class="inline-flex max-w-full flex-col gap-1">
		{#if kind === 'image'}
			<button
				type="button"
				class="cursor-zoom-in rounded border border-base-300"
				onclick={() => onzoom?.(url, media.caption)}
				aria-label={media.caption ?? 'Zoom image'}
			>
				<img
					src={url}
					alt={media.caption ?? ''}
					class="rounded object-cover"
					class:max-h-72={!thumb}
					class:h-24={thumb}
					class:w-32={thumb}
				/>
			</button>
		{:else if kind === 'audio'}
			<audio controls src={url} class:w-32={thumb}></audio>
		{:else}
			<video
				controls
				playsinline
				src={url}
				class="rounded"
				class:max-h-72={!thumb}
				class:h-24={thumb}
			>
				<track kind="captions" />
			</video>
		{/if}
		{#if media.caption && !thumb}
			<figcaption class="text-xs opacity-70">{media.caption}</figcaption>
		{/if}
	</figure>
{/if}
