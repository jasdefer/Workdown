import { describe, expect, it } from 'vitest';
import type { Diagnostic } from '$lib/api/generated/Diagnostic';
import { foldLabel, foldOpensFor } from './collapsedFields';

const collapsed = new Set(['status_color', 'effort']);

function itemWarning(detail: Record<string, unknown>): Diagnostic {
	return {
		severity: 'warning',
		message: 'rendered elsewhere',
		scope: 'item',
		source_path: 'workdown-items/login.md',
		item_id: 'login',
		...detail
	} as Diagnostic;
}

describe('foldOpensFor', () => {
	it('opens when a warning names a collapsed field', () => {
		const warnings = [itemWarning({ type: 'missing_required', field: 'effort' })];
		expect(foldOpensFor(warnings, collapsed)).toBe(true);
	});

	it('stays closed when every named field is visible', () => {
		const warnings = [itemWarning({ type: 'missing_required', field: 'status' })];
		expect(foldOpensFor(warnings, collapsed)).toBe(false);
	});

	it('stays closed for findings that name no field', () => {
		const warnings = [
			itemWarning({ type: 'rule_violation', rule: 'sized', detail: 'points unset' }),
			itemWarning({ type: 'derive_cycle', chain: ['effort', 'cost'] })
		];
		expect(foldOpensFor(warnings, collapsed)).toBe(false);
	});

	it('ignores findings outside the item scope', () => {
		const config: Diagnostic = {
			severity: 'warning',
			message: 'rendered elsewhere',
			scope: 'config',
			source_path: '.workdown/views.yaml',
			type: 'view_duplicate_id',
			view_id: 'effort'
		};
		expect(foldOpensFor([config], collapsed)).toBe(false);
	});

	it('is false with no warnings', () => {
		expect(foldOpensFor([], collapsed)).toBe(false);
	});
});

describe('foldLabel', () => {
	it('counts and pluralises', () => {
		expect(foldLabel(1, false)).toBe('Show 1 more field');
		expect(foldLabel(3, false)).toBe('Show 3 more fields');
		expect(foldLabel(1, true)).toBe('Hide 1 field');
		expect(foldLabel(3, true)).toBe('Hide 3 fields');
	});
});
