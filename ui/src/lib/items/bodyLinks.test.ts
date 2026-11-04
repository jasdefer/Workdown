import { Marked } from 'marked';
import { describe, expect, it } from 'vitest';
import {
	itemLinkExtension,
	referenceOfHref,
	referenceOfId,
	type ItemLinkResolver
} from './bodyLinks';

const resolver: ItemLinkResolver = {
	knownIds: new Set(['auth-epic', 'login']),
	hrefFor: (id) => `?item=${id}`
};

describe('referenceOfId', () => {
	it('points at an item that exists', () => {
		expect(referenceOfId('login', resolver)).toEqual({
			kind: 'item',
			id: 'login',
			href: '?item=login'
		});
	});

	it('marks an id no item has as missing', () => {
		expect(referenceOfId('nope', resolver)).toEqual({ kind: 'missing', id: 'nope' });
	});

	it('takes every id to exist while the index is unknown', () => {
		const unknown: ItemLinkResolver = { ...resolver, knownIds: null };
		expect(referenceOfId('nope', unknown)).toEqual({
			kind: 'item',
			id: 'nope',
			href: '?item=nope'
		});
	});
});

describe('referenceOfHref', () => {
	it('resolves a sibling .md file, with or without ./', () => {
		expect(referenceOfHref('login.md', resolver)).toMatchObject({ kind: 'item', id: 'login' });
		expect(referenceOfHref('./login.md', resolver)).toMatchObject({ kind: 'item', id: 'login' });
	});

	it('marks a sibling .md file no item has as missing', () => {
		expect(referenceOfHref('nope.md', resolver)).toEqual({ kind: 'missing', id: 'nope' });
	});

	it('leaves everything that is not a sibling .md file alone', () => {
		for (const href of [
			'../docs/architecture.md',
			'docs/login.md',
			'https://example.com/login.md',
			'login.md#notes',
			'login.md?x=1',
			'login',
			'#top'
		]) {
			expect(referenceOfHref(href, resolver), href).toEqual({ kind: 'other' });
		}
	});
});

describe('itemLinkExtension', () => {
	const render = (markdown: string): string =>
		new Marked(itemLinkExtension(resolver)).parse(markdown, { async: false }).trim();

	it('renders [[id]] as a link to the item', () => {
		expect(render('see [[login]]')).toBe(
			'<p>see <a class="item-link" href="?item=login">login</a></p>'
		);
	});

	it('renders [[id]] of a missing item as dead text that says so', () => {
		expect(render('see [[nope]]')).toBe(
			'<p>see <span class="item-link dead" title="No item &#39;nope&#39;">nope</span></p>'
		);
	});

	it('points a [text](id.md) link at the item, keeping the text', () => {
		expect(render('the [login *task*](login.md)')).toBe(
			'<p>the <a class="item-link" href="?item=login">login <em>task</em></a></p>'
		);
	});

	it('renders a [text](id.md) link to a missing item as dead text', () => {
		expect(render('[gone](nope.md)')).toBe(
			'<p><span class="item-link dead" title="No item &#39;nope&#39;">gone</span></p>'
		);
	});

	it('leaves other links to marked', () => {
		expect(render('[docs](https://example.com/a.md)')).toBe(
			'<p><a href="https://example.com/a.md">docs</a></p>'
		);
	});

	it('leaves [[id]] inside code alone', () => {
		expect(render('write `[[login]]`')).toBe('<p>write <code>[[login]]</code></p>');
	});
});
