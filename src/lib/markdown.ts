// Card text is untrusted (SPEC §13): marked renders, DOMPurify sanitizes, raw HTML is escaped,
// images never come from markdown (media is attached as MediaRefs), and only http(s) links
// survive; the Markdown component confirms before opening one in the system browser.
import DOMPurify from 'dompurify';
import { Marked, type Tokens } from 'marked';

const escapeHtml = (s: string) => s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);

const marked = new Marked({
	gfm: true,
	renderer: {
		html({ text }: Tokens.HTML | Tokens.Tag) {
			return escapeHtml(text);
		},
		link({ href, tokens }: Tokens.Link) {
			const inner = this.parser.parseInline(tokens);
			if (!/^https?:\/\//i.test(href)) return inner;
			return `<a href="${escapeHtml(href)}" class="link">${inner}</a> <span class="md-url">(${escapeHtml(href)})</span>`;
		},
		image({ href, text }: Tokens.Image) {
			return `<span class="md-url">[image: ${escapeHtml(text || href)}]</span>`;
		}
	}
});

const purifyConfig = {
	FORBID_TAGS: [
		'img',
		'video',
		'audio',
		'iframe',
		'object',
		'embed',
		'form',
		'input',
		'button',
		'style',
		'script',
		'svg',
		'math',
		'link',
		'meta'
	],
	FORBID_ATTR: ['style', 'src', 'srcset', 'action', 'formaction', 'target'],
	ALLOWED_URI_REGEXP: /^https?:\/\//i
};

export function renderMarkdown(source: string): string {
	const html = marked.parse(source ?? '', { async: false }) as string;
	return DOMPurify.sanitize(html, purifyConfig);
}
