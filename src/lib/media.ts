import { api } from '#lib/api';

let basePromise: Promise<string> | null = null;

/** Cached for the process; the server's port and token change every launch. */
export function mediaBase(): Promise<string> {
	const p = basePromise ?? (basePromise = api.mediaBaseUrl());
	return p;
}

export function mediaUrl(base: string, ref: { hash: string; ext: string }): string {
	return `${base}${ref.hash.slice(0, 2)}/${ref.hash}.${ref.ext}`;
}

export type MediaKind = 'image' | 'audio' | 'video';

export function mediaKind(mime: string): MediaKind {
	if (mime.startsWith('image/')) return 'image';
	if (mime.startsWith('audio/')) return 'audio';
	return 'video';
}

export const MEDIA_EXTENSIONS = [
	'png',
	'jpg',
	'jpeg',
	'webp',
	'gif',
	'mp3',
	'ogg',
	'wav',
	'flac',
	'mp4',
	'webm'
];
