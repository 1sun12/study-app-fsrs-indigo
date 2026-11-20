// Settings the UI needs everywhere (theme, editor keymap). Loaded once per launch.
import { api, type Settings } from '#lib/api';
import { theme } from '#lib/theme.svelte';

class Prefs {
	settings = $state<Settings | null>(null);

	async load() {
		this.settings = await api.settingsGet();
		theme.apply(this.settings.theme);
	}

	async save(next: Settings) {
		await api.settingsSet(next);
		this.settings = next;
		theme.apply(next.theme);
	}
}

export const prefs = new Prefs();
