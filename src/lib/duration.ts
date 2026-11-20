/** "45 s", "12 min", "3 h 5 min" for a measured duration. */
export function duration(ms: number): string {
	const min = Math.round(ms / 60_000);
	if (ms < 60_000) return `${Math.round(ms / 1000)} s`;
	if (min < 60) return `${min} min`;
	return `${Math.floor(min / 60)} h${min % 60 ? ` ${min % 60} min` : ''}`;
}
