// Sync status for the nav dot and Settings → Sync; the backend pushes `sync-status` events.
import { listen } from '@tauri-apps/api/event';
import { api, type SyncView } from '#lib/api';

class SyncStore {
	view = $state<SyncView | null>(null);
	#listening = false;

	async init() {
		if (!this.#listening) {
			this.#listening = true;
			await listen<SyncView>('sync-status', (e) => (this.view = e.payload));
		}
		this.view = await api.syncStatus();
	}

	/** off · syncing · synced · waiting · attention */
	get state() {
		const v = this.view;
		if (!v?.configured) return 'off';
		if (v.syncing) return 'syncing';
		if (v.status.failed || v.status.conflicts > 0) return 'attention';
		if (v.status.message) return 'waiting';
		return v.status.last_success_at ? 'synced' : 'waiting';
	}

	/** Fire and forget: sync never blocks studying (SPEC §11.3). */
	now() {
		api.syncNow().catch(() => {});
	}
}

export const syncStore = new SyncStore();
