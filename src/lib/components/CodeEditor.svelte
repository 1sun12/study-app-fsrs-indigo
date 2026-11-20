<script lang="ts">
	// CodeMirror 6 for typed answers and reference text, and (`prose`, when Vim is on) for the card
	// editor's Prompt/Answer boxes so Vim keys work there too. Languages come from
	// @codemirror/language-data; a language it lacks borrows a close mode (GDScript uses
	// Python, OWNER-REVIEW R-03; shader languages use C++).
	import { onMount } from 'svelte';
	import { EditorView, basicSetup, minimalSetup } from 'codemirror';
	import { Compartment, EditorState } from '@codemirror/state';
	import { keymap, placeholder as cmPlaceholder } from '@codemirror/view';
	import { LanguageDescription, indentUnit, syntaxHighlighting } from '@codemirror/language';
	import { classHighlighter } from '@lezer/highlight';
	import { languages } from '@codemirror/language-data';
	import { vim } from '@replit/codemirror-vim';
	import { prefs } from '#lib/prefs.svelte';

	let {
		value = $bindable(''),
		syntax = null,
		readonly = false,
		placeholder = '',
		prose = false,
		autofocus = false,
		onsubmit
	}: {
		value?: string;
		syntax?: string | null;
		readonly?: boolean;
		placeholder?: string;
		/** Wrapped text, no gutter: a Vim-capable stand-in for a plain text box. */
		prose?: boolean;
		autofocus?: boolean;
		onsubmit?: () => void;
	} = $props();

	let host: HTMLDivElement;
	let view: EditorView | undefined = $state();
	const lang = new Compartment();
	const FALLBACK: Record<string, string> = {
		gdscript: 'python',
		gdshader: 'cpp',
		glsl: 'cpp',
		hlsl: 'cpp',
		wgsl: 'cpp',
		vb: 'vb.net',
		matlab: 'octave',
		elixir: 'ruby',
		racket: 'scheme',
		zig: 'c',
		nim: 'python',
		asm: 'gas',
		solidity: 'javascript',
		svelte: 'html',
		makefile: 'shell'
	};
	const keymapComp = new Compartment();
	const tabComp = new Compartment();

	function tabExtension(width: number) {
		return [EditorState.tabSize.of(width), indentUnit.of(' '.repeat(width))];
	}

	onMount(() => {
		view = new EditorView({
			parent: host,
			state: EditorState.create({
				doc: value,
				extensions: [
					// Vim must precede the other keymaps; Mod-Enter must beat insertBlankLine.
					keymapComp.of(prefs.settings?.editor.vim ? vim() : []),
					keymap.of([
						{
							key: 'Mod-Enter',
							run: () => {
								// Without a submit of its own, Ctrl+Enter reaches the surrounding form.
								if (!onsubmit) return false;
								onsubmit();
								return true;
							}
						}
					]),
					prose ? [minimalSetup, EditorView.lineWrapping, cmPlaceholder(placeholder)] : basicSetup,
					// tok-* classes, coloured in layout.css to match Shiki in both themes.
					syntaxHighlighting(classHighlighter),
					lang.of([]),
					tabComp.of(tabExtension(prefs.settings?.editor.tab_width ?? 4)),
					EditorState.readOnly.of(readonly),
					EditorView.updateListener.of((u) => {
						if (u.docChanged) value = u.state.doc.toString();
					})
				]
			})
		});
		if (autofocus) view.focus();
		return () => view?.destroy();
	});

	$effect(() => {
		const v = view;
		if (!v) return;
		const name = syntax ? (FALLBACK[syntax] ?? syntax) : null;
		if (!name) {
			v.dispatch({ effects: lang.reconfigure([]) });
			return;
		}
		const desc = LanguageDescription.matchLanguageName(languages, name, true);
		if (!desc) return;
		desc.load().then((support) => v.dispatch({ effects: lang.reconfigure(support) }));
	});

	$effect(() => {
		const v = view;
		const editor = prefs.settings?.editor;
		if (!v || !editor) return;
		v.dispatch({
			effects: [
				keymapComp.reconfigure(editor.vim ? vim() : []),
				tabComp.reconfigure(tabExtension(editor.tab_width))
			]
		});
	});

	$effect(() => {
		const v = view;
		const next = value;
		if (v && next !== v.state.doc.toString()) {
			v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: next } });
		}
	});
</script>

<div
	bind:this={host}
	class="cm-host border border-base-300 bg-base-100"
	class:cm-prose={prose}
	aria-label={placeholder}
></div>
