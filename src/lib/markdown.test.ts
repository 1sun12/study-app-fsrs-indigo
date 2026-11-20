// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
import { renderMarkdown } from './markdown';

describe('renderMarkdown', () => {
	it('renders markdown structure', () => {
		const html = renderMarkdown('# Title\n\nSome **bold** text and `code`.');
		expect(html).toContain('<h1>Title</h1>');
		expect(html).toContain('<strong>bold</strong>');
		expect(html).toContain('<code>code</code>');
	});

	it('escapes raw html and strips scripts, handlers and images', () => {
		const html = renderMarkdown(
			'<script>alert(1)</script><img src=x onerror=alert(1)>\n\n![pic](http://evil/x.png)'
		);
		expect(html).not.toContain('<script');
		expect(html).not.toContain('<img');
		expect(html).not.toMatch(/<[^>]*onerror/);
		expect(html).toContain('&lt;img');
		expect(html).toContain('[image: pic]');
	});

	it('keeps http(s) links with the target visible and drops other schemes', () => {
		const html = renderMarkdown('[docs](https://example.com/a?b=1) [bad](javascript:alert(1))');
		expect(html).toContain('href="https://example.com/a?b=1"');
		expect(html).toContain('(https://example.com/a?b=1)');
		expect(html).not.toContain('javascript:');
		expect(html).toContain('bad');
	});
});
