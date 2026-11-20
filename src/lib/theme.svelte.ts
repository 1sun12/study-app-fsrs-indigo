// daisyUI theme pair (SPEC §14): "system" removes data-theme so prefers-color-scheme decides.
import type { Theme } from '#lib/api/types/Theme';

class ThemeStore {
	current = $state<Theme>('system');

	apply(theme: Theme) {
		this.current = theme;
		if (typeof document === 'undefined') return;
		if (theme === 'system') delete document.documentElement.dataset.theme;
		else document.documentElement.dataset.theme = theme === 'dark' ? 'indigo-dark' : 'indigo';
	}
}

export const theme = new ThemeStore();
