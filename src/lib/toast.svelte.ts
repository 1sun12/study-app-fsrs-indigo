// One small notification queue, rendered by the root layout.
export type Toast = { id: number; kind: 'info' | 'error' | 'warning'; text: string };

let next = 1;

class Toasts {
	items = $state<Toast[]>([]);

	push(kind: Toast['kind'], text: string, ms = 5000) {
		const id = next++;
		this.items = [...this.items, { id, kind, text }];
		setTimeout(() => this.dismiss(id), ms);
	}

	info(text: string) {
		this.push('info', text);
	}

	warning(text: string) {
		this.push('warning', text, 8000);
	}

	error(text: string) {
		this.push('error', text, 8000);
	}

	dismiss(id: number) {
		this.items = this.items.filter((t) => t.id !== id);
	}
}

export const toast = new Toasts();
