import { describe, it, expect } from 'vitest';
import type { Diagnostic } from '$lib/api/generated/Diagnostic';
import type { FieldUsage } from '$lib/api/generated/FieldUsage';
import { blockerPhrase, removeDialogText, summarizeUsage, type UsageSummary } from './fieldUsage';

const viewNamesIt: Diagnostic = {
	severity: 'warning',
	message: "views.yaml: view 'roadmap' slot 'field' names unknown field 'status'",
	scope: 'config',
	source_path: '.workdown/views.yaml',
	type: 'view_unknown_field',
	field_name: 'status',
	view_id: 'roadmap',
	slot: 'field'
};

const roleNamesIt: Diagnostic = {
	severity: 'warning',
	message: "config.yaml: 'board' names unknown field 'status'",
	scope: 'config',
	source_path: '.workdown/config.yaml',
	type: 'config_unknown_field',
	slot: 'board',
	field_name: 'status'
};

const computeReadsIt: Diagnostic = {
	severity: 'warning',
	message: "schema.yaml: compute expression for 'total' is invalid",
	scope: 'config',
	source_path: '.workdown/schema.yaml',
	type: 'compute_invalid_expression',
	field: 'total',
	expression: 'points * 2',
	detail: 'unknown field points'
};

function itemHoldsIt(itemId: string): Diagnostic {
	return {
		severity: 'warning',
		message: `${itemId}: unknown field 'status'`,
		scope: 'item',
		source_path: `workdown-items/${itemId}.md`,
		item_id: itemId,
		type: 'unknown_field',
		field: 'status'
	};
}

function usage(overrides: Partial<FieldUsage>): FieldUsage {
	return { rules: [], recipes: [], parse_error: null, introduced: [], ...overrides };
}

const nothing: UsageSummary = {
	rules: [],
	views: [],
	roles: [],
	recipes: [],
	other: [],
	itemCount: 0
};

describe('summarizeUsage', () => {
	it('is empty for a field nothing depends on', () => {
		expect(summarizeUsage(usage({}))).toEqual(nothing);
	});

	it('lists the rules and recipes the server names, in its order', () => {
		const summary = summarizeUsage(
			usage({ rules: ['sized', 'owned'], recipes: ['total'], parse_error: 'would not load' })
		);
		expect(summary.rules).toEqual(['sized', 'owned']);
		expect(summary.recipes).toEqual(['total']);
		expect(summary.other).toEqual([]);
	});

	it('shows the parser message only when no rule or recipe explains it', () => {
		const summary = summarizeUsage(usage({ parse_error: 'something else is wrong' }));
		expect(summary.other).toEqual(['something else is wrong']);
	});

	it('sorts the diagnostics into views, config roles and recipes', () => {
		const summary = summarizeUsage(
			usage({ introduced: [viewNamesIt, roleNamesIt, computeReadsIt] })
		);
		expect(summary.views).toEqual(['roadmap']);
		expect(summary.roles).toEqual(['board']);
		expect(summary.recipes).toEqual(['total']);
	});

	it('names a view once however many slots of it break', () => {
		const second: Diagnostic = { ...viewNamesIt, slot: 'where' };
		expect(summarizeUsage(usage({ introduced: [viewNamesIt, second] })).views).toEqual(['roadmap']);
	});

	it('merges a recipe the server names with one a diagnostic names', () => {
		const summary = summarizeUsage(usage({ recipes: ['total'], introduced: [computeReadsIt] }));
		expect(summary.recipes).toEqual(['total']);
	});

	it('counts distinct items', () => {
		const summary = summarizeUsage(
			usage({ introduced: [itemHoldsIt('login'), itemHoldsIt('login'), itemHoldsIt('signup')] })
		);
		expect(summary.itemCount).toBe(2);
	});

	it('keeps a diagnostic with no name to show as its message', () => {
		const other: Diagnostic = {
			severity: 'warning',
			message: 'a cycle through parent',
			scope: 'collection',
			type: 'cycle',
			field: 'parent',
			chain: ['a', 'b']
		};
		expect(summarizeUsage(usage({ introduced: [other] })).other).toEqual([
			'a cycle through parent'
		]);
	});
});

describe('blockerPhrase', () => {
	it('is null when only items hold a value', () => {
		expect(blockerPhrase({ ...nothing, itemCount: 5 })).toBeNull();
	});

	it('counts one group with its number', () => {
		expect(blockerPhrase({ ...nothing, rules: ['a', 'b', 'c'] })).toBe('3 rules');
		expect(blockerPhrase({ ...nothing, roles: ['board'] })).toBe('1 config role');
	});

	it('joins several groups with commas and a final and', () => {
		expect(blockerPhrase({ ...nothing, rules: ['a'], views: ['v', 'w'], recipes: ['total'] })).toBe(
			'1 rule, 2 views and 1 recipe'
		);
	});
});

describe('removeDialogText', () => {
	it('offers one answer when the count is unknown', () => {
		const text = removeDialogText(null);
		expect(text.dropLabel).toBeNull();
		expect(text.body).toBe('Items holding a value keep it and get a warning.');
	});

	it('offers one answer when no item holds a value', () => {
		const text = removeDialogText(0);
		expect(text.dropLabel).toBeNull();
		expect(text.keepLabel).toBe('Remove');
		expect(text.body).toBe('No item holds a value for it.');
	});

	it('offers to drop the values and names the count', () => {
		expect(removeDialogText(3)).toEqual({
			body: '3 items hold a value: keep it with a warning on each, or drop it from the files.',
			keepLabel: 'Remove field',
			dropLabel: 'Remove field and its values (3 items)'
		});
	});

	it('words one item in the singular', () => {
		const text = removeDialogText(1);
		expect(text.body).toBe(
			'1 item holds a value: keep it with a warning on each, or drop it from the files.'
		);
		expect(text.dropLabel).toBe('Remove field and its value (1 item)');
	});
});
