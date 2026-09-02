import { describe, it, expect } from 'vitest';
import type { ViewSummary } from '$lib/api/generated/ViewSummary';
import { FALLBACK_TITLE, documentTitle, pageLabel } from './documentTitle';

const views: ViewSummary[] = [
	{ id: 'status-board', title: null, kind: 'board' },
	{ id: 'roadmap', title: 'Q3 Roadmap', kind: 'gantt' }
];

describe('pageLabel', () => {
	it('names a view page from the views index', () => {
		expect(pageLabel('/views/[id]', { id: 'roadmap' }, views)).toBe('Q3 Roadmap');
	});

	it('prettifies the view id when the view carries no title', () => {
		expect(pageLabel('/views/[id]', { id: 'status-board' }, views)).toBe('Status Board');
	});

	it('prettifies the id of a view the index does not know', () => {
		// A 404 view still gets a readable tab while `+error.svelte`
		// explains itself; waiting on the index would leave it unnamed.
		expect(pageLabel('/views/[id]', { id: 'ghost-view' }, views)).toBe('Ghost View');
	});

	it('names an item page from its prettified id', () => {
		// The same label the page's own heading shows — no second fetch,
		// nothing to wait for.
		expect(pageLabel('/items/[id]', { id: 'implement-login' }, views)).toBe('Implement Login');
	});

	it('gives the fixed routes their fixed words', () => {
		expect(pageLabel('/items/new', {}, views)).toBe('New item');
		expect(pageLabel('/views/new', {}, views)).toBe('New view');
		expect(pageLabel('/views/[id]/edit', { id: 'roadmap' }, views)).toBe('Edit view');
	});

	it('leaves the root and unknown routes unnamed', () => {
		// `/` only redirects to the first view, and a route with no name
		// of its own should show the project alone rather than a guess.
		expect(pageLabel('/', {}, views)).toBeNull();
		expect(pageLabel(null, {}, views)).toBeNull();
	});
});

describe('documentTitle', () => {
	it('puts the project first, then the page', () => {
		expect(documentTitle('Acme Backlog', 'Status Board')).toBe('Acme Backlog — Status Board');
	});

	it('is the project alone when the page has no name', () => {
		expect(documentTitle('Acme Backlog', null)).toBe('Acme Backlog');
	});

	it('falls back to the tool name before the project is known', () => {
		// The first paint, and any tab whose project fetch failed.
		expect(documentTitle(null, null)).toBe(FALLBACK_TITLE);
		expect(documentTitle(undefined, 'Status Board')).toBe(`${FALLBACK_TITLE} — Status Board`);
	});

	it('treats a blank project name as no name at all', () => {
		expect(documentTitle('   ', 'Status Board')).toBe(`${FALLBACK_TITLE} — Status Board`);
	});

	it('trims a padded project name', () => {
		expect(documentTitle('  Acme Backlog  ', null)).toBe('Acme Backlog');
	});
});
