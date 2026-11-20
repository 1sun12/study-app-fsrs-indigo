// The current study session: the plan from the backend and the cursor into it.
// Everything about an attempt itself lives in the backend; a reload refetches it.
import { goto } from '$app/navigation';
import { api, type SessionPlan } from '#lib/api';
import type { PlanItem } from '#lib/api/types/PlanItem';

class Session {
	plan = $state<SessionPlan | null>(null);
	index = $state(0);
	done = $state(0);

	start(plan: SessionPlan) {
		this.plan = plan;
		this.index = 0;
		this.done = 0;
	}

	get current(): PlanItem | null {
		return this.plan?.items[this.index] ?? null;
	}

	get remaining(): number {
		return this.plan ? Math.max(0, this.plan.items.length - this.index) : 0;
	}

	advance() {
		this.index += 1;
		this.done += 1;
	}

	/** A cram run over these cards (shuffled), then off to the study screen. */
	async cramCards(ids: string[]) {
		const plan = await api.sessionCram(ids);
		if (plan.items.length === 0) throw new Error('Nothing to cram');
		for (let i = plan.items.length - 1; i > 0; i--) {
			const j = Math.floor(Math.random() * (i + 1));
			[plan.items[i], plan.items[j]] = [plan.items[j], plan.items[i]];
		}
		this.start(plan);
		await goto('/study');
	}

	get cramming(): boolean {
		return this.current?.kind === 'cram';
	}

	clear() {
		this.plan = null;
		this.index = 0;
		this.done = 0;
	}
}

export const session = new Session();
