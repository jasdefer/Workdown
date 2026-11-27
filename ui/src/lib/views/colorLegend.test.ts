import { describe, it, expect } from 'vitest';
import type { ColorLegend } from '$lib/api/generated/ColorLegend';
import type { ConditionShape } from '$lib/api/generated/ConditionShape';
import { legendModel } from './colorLegend';

function fieldIs(field: string, value: string): ConditionShape {
	return {
		kind: 'row',
		left: { kind: 'field', name: field },
		operator: 'equal',
		right: { kind: 'literal', value }
	};
}

type Written = { color: string; count: number }[];

function byValue(
	subject: string,
	entries: { color: string; values: (string | null)[] }[],
	written: Written = []
): ColorLegend {
	return {
		field: 'color',
		meaning: {
			kind: 'by_value',
			subject,
			entries: entries.map((entry) => ({ ...entry, count: 1 })),
			written
		}
	};
}

function byCondition(
	entries: { color: string; conditions?: ConditionShape[]; otherwise?: boolean }[],
	written: Written = []
): ColorLegend {
	return {
		field: 'color',
		meaning: {
			kind: 'by_condition',
			entries: entries.map((entry) => ({
				color: entry.color,
				count: 1,
				conditions: entry.conditions ?? [],
				otherwise: entry.otherwise ?? false
			})),
			written
		}
	};
}

describe('legendModel', () => {
	it('names the subject field and labels each entry with the values on screen', () => {
		const model = legendModel(
			byValue('status', [
				{ color: '#22c55e', values: ['done'] },
				// Gray is `removed` and the default; only to-do items wear it.
				{ color: '#6b7280', values: ['to_do'] }
			])
		);
		expect(model.caption).toBe('Color by Status');
		expect(model.entries.map((entry) => entry.label)).toEqual(['done', 'to_do']);
	});

	it('joins several values of one color and names a missing one', () => {
		const model = legendModel(
			byValue('status', [{ color: '#6b7280', values: ['to_do', 'removed', null] }])
		);
		expect(model.entries[0]?.label).toBe('to_do, removed, (no Status)');
	});

	it('spells every condition out when there is no subject', () => {
		const overdue: ConditionShape = {
			kind: 'row',
			left: { kind: 'field', name: 'end_date' },
			operator: 'less_than',
			right: { kind: 'today' }
		};
		const model = legendModel(
			byCondition([
				{ color: '#22c55e', conditions: [fieldIs('status', 'done')] },
				{ color: '#ef4444', conditions: [overdue], otherwise: true },
				{ color: '#6b7280', otherwise: true }
			])
		);
		expect(model.caption).toBe('Color');
		expect(model.entries.map((entry) => entry.label)).toEqual([
			'Status = done',
			'End Date < today or otherwise',
			'everything else'
		]);
	});

	it('shows a condition that is not one plain comparison as written', () => {
		const model = legendModel(
			byCondition([
				{ color: '#ef4444', conditions: [{ kind: 'text', source: 'effort * 2 > budget' }] }
			])
		);
		expect(model.entries[0]?.label).toBe('effort * 2 > budget');
	});

	it('lists hand-written colors after the conditions', () => {
		const model = legendModel(
			byValue('status', [{ color: '#22c55e', values: ['done'] }], [{ color: '#ef4444', count: 3 }])
		);
		expect(model.entries[1]).toEqual({ color: '#ef4444', label: 'set by hand', count: 3 });
	});

	it('spells a constant as the schema does, so it is not taken for a field', () => {
		const overLimit: ConditionShape = {
			kind: 'row',
			left: { kind: 'field', name: 'effort' },
			operator: 'greater_than',
			right: { kind: 'constant', name: 'limit' }
		};
		const model = legendModel(byCondition([{ color: '#ef4444', conditions: [overLimit] }]));
		expect(model.entries[0]?.label).toBe('Effort > $constants.limit');
	});

	it('says where a pulled color comes from, without entries', () => {
		const same = legendModel({
			field: 'color',
			meaning: { kind: 'inherited', over: 'parent', source_field: 'color' }
		});
		expect(same).toEqual({ caption: 'Color inherited from Parent', entries: [] });

		const other = legendModel({
			field: 'color',
			meaning: { kind: 'inherited', over: 'parent', source_field: 'team_color' }
		});
		expect(other.caption).toBe('Color inherited from Parent (Team Color)');
	});
});
