import { expect, it } from 'vitest';
import { confirm, confirmState } from './confirm.svelte';

it('resolves with the answer and closes', async () => {
	const p = confirm('Delete?', { title: 'Delete', kind: 'warning' });
	expect(confirmState.ask?.title).toBe('Delete');
	confirmState.answer(true);
	expect(await p).toBe(true);
	expect(confirmState.ask).toBeNull();
});

it('a second ask cancels the first', async () => {
	const first = confirm('One');
	const second = confirm('Two');
	expect(await first).toBe(false);
	expect(confirmState.ask?.message).toBe('Two');
	confirmState.answer(false);
	expect(await second).toBe(false);
});
