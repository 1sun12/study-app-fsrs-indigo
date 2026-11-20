// Add / edit card is a foreground modal (owner rule), opened from anywhere; never a route.
import { api, type Card, type Category } from '#lib/api';
import type { Step } from '#lib/api/types/Step';

export function blankStep(response: Step['response'] = 'recall'): Step {
	return {
		id: 'tmp-' + crypto.randomUUID(),
		prompt_md: '',
		response,
		solution_md: '',
		solution_text: null,
		criteria: [],
		points: 1,
		media: []
	};
}

class CardModal {
	card = $state<Card | null>(null);
	categories = $state<Category[]>([]);
	/** Bumped after every save or delete so open views reload. */
	version = $state(0);
	/** New editor instance per open, so "Add & next" starts from a clean form. */
	instance = $state(0);
	/** Set by the editor; closing a dirty form asks first. */
	dirty = false;

	/** A blank card seeded from the remembered fields of the last card added. */
	async add() {
		const [cats, settings] = await Promise.all([api.categoriesList(), api.settingsGet()]);
		const d = settings.card_defaults;
		const remembered = d.enabled;
		const now = new Date().toISOString();
		this.categories = cats;
		this.card = {
			id: '',
			title: '',
			category_id:
				remembered && cats.some((c) => c.id === d.category_id)
					? (d.category_id ?? '')
					: (cats[0]?.id ?? ''),
			difficulty: remembered ? d.difficulty : 'beginner',
			tags: remembered ? d.tags : [],
			est_minutes: remembered ? d.est_minutes : null,
			steps: [blankStep(remembered ? d.response : 'recall')],
			source: remembered ? d.source : null,
			pack_id: null,
			pack_card_id: null,
			state: 'shelved',
			created_at: now,
			modified_at: now,
			modified_by: '',
			deleted_at: null
		};
		this.dirty = false;
		this.instance++;
	}

	async edit(id: string) {
		const [card, cats] = await Promise.all([api.cardGet(id), api.categoriesList()]);
		if (!card) throw new Error('That card no longer exists.');
		this.categories = cats;
		this.card = card;
		this.dirty = false;
		this.instance++;
	}

	close() {
		this.card = null;
		this.dirty = false;
	}

	/** Esc, the close button or the backdrop: asks before dropping unsaved work. */
	async requestClose() {
		if (this.dirty) {
			const { confirm } = await import('@tauri-apps/plugin-dialog');
			const ok = await confirm('Discard the changes to this card?', {
				title: 'Discard changes',
				kind: 'warning'
			});
			if (!ok) return;
		}
		this.close();
	}

	changed() {
		this.version++;
		this.close();
	}

	/** After "Add & next": refresh views and open a fresh form with the remembered fields. */
	async next() {
		this.version++;
		await this.add();
	}
}

export const cardModal = new CardModal();
