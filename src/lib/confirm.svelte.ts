// Yes/no questions inside the app instead of the system popup; rendered by the root layout.
type Options = { title?: string; kind?: 'info' | 'warning' | 'error'; okLabel?: string };
type Ask = Options & { message: string; resolve: (ok: boolean) => void };

class Confirm {
	ask = $state<Ask | null>(null);

	answer(ok: boolean) {
		const a = this.ask;
		this.ask = null;
		a?.resolve(ok);
	}
}

export const confirmState = new Confirm();

/** Same shape as the dialog plugin's `confirm`. A second ask (a double Esc) cancels the first. */
export function confirm(message: string, options: Options = {}): Promise<boolean> {
	confirmState.answer(false);
	return new Promise((resolve) => (confirmState.ask = { ...options, message, resolve }));
}
