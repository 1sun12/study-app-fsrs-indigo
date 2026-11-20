// Category colours (owner ruling 2026-11-12: users pick them). New categories take the next
// unused colour so dots are distinct from the start; any CSS colour can be set later.
export const PALETTE = [
	'#ef4444',
	'#f97316',
	'#eab308',
	'#84cc16',
	'#22c55e',
	'#14b8a6',
	'#06b6d4',
	'#3b82f6',
	'#6366f1',
	'#a855f7',
	'#ec4899',
	'#78716c'
];

export const FALLBACK_COLOR = '#8b8fd6';

export function nextColor(used: (string | null)[]): string {
	return PALETTE.find((c) => !used.includes(c)) ?? PALETTE[used.length % PALETTE.length];
}
