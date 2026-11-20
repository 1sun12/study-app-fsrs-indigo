import { readFileSync } from 'node:fs';
import { bundledLanguages } from 'shiki';
import { expect, it } from 'vitest';

// Every value in the core's known-language table must be a Shiki language id.
it('core syntax table maps to Shiki ids', () => {
	const src = readFileSync('crates/core/src/syntax.rs', 'utf8');
	const ids = [...src.matchAll(/\("[^"]+", "([^"]+)"\)/g)].map((m) => m[1]);
	expect(ids.length).toBeGreaterThan(40);
	expect(ids.filter((id) => !(id in bundledLanguages))).toEqual([]);
});
