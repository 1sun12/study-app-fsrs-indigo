import { expect, it } from 'vitest';
import { duration } from './duration';

it('formats durations', () => {
	expect(duration(45_000)).toBe('45 s');
	expect(duration(12 * 60_000)).toBe('12 min');
	expect(duration(185 * 60_000)).toBe('3 h 5 min');
	expect(duration(120 * 60_000)).toBe('2 h');
});
