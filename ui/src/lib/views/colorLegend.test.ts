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

interface EntryInput {
	color: string;
	conditions?: ConditionShape[];
	otherwise?: boolean;
	values?: (string | null)[];
}

function conditions(
	subject: string | null,
	entries: EntryInput[],
	written: { color: string; count: number }[] = []
): ColorLegend {
	return {
		field: 'color',
		meaning: {
			kind: 'conditions',
			subject,
			entries: entries.map((entry) => ({
				color: entry.color,
				count: 1,
				conditions: entry.conditions ?? [],
				otherwise: entry.otherwise ?? false,
				values: entry.values ?? []
			})),
			written
		}
	};
}

describe('legendModel', () => {
	it('names the subject field and labels each entry with the values on screen', () => {
		const model = legendModel(
			conditions('status', [
				{ color: '#22c55e', conditions: [fieldIs('status', 'done')], values: ['done'] },
				// Gray is `removed` and the default; only to-do items wear it.
				{
					color: '#6b7280',
					conditions: [fieldIs('status', 'removed')],
					otherwise: true,
					values: ['to_do']
				}
			])
		);
		expect(model.caption).toBe('Color by Status');
		expect(model.entries.map((entry) => entry.label)).toEqual(['done', 'to_do']);
	});

	it('joins several values of one color and names a missing one', () => {
		const model = legendModel(
			conditions('status', [
				{ color: '#6b7280', otherwise: true, values: ['to_do', 'removed', null] }
			])
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
			conditions(null, [
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
			conditions(null, [
				{ color: '#ef4444', conditions: [{ kind: 'text', source: 'effort * 2 > budget' }] }
			])
		);
		expect(model.entries[0]?.label).toBe('effort * 2 > budget');
	});

	it('lists hand-written colors after the conditions', () => {
		const model = legendModel(
			conditions(
				'status',
				[{ color: '#22c55e', conditions: [fieldIs('status', 'done')], values: ['done'] }],
				[{ color: '#ef4444', count: 3 }]
			)
		);
		expect(model.entries[1]).toEqual({ color: '#ef4444', label: 'set by hand', count: 3 });
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

	it('names the fields a recipe without a listable mapping reads', () => {
		const model = legendModel({
			field: 'color',
			meaning: { kind: 'depends_on', fields: ['status', 'end_date'] }
		});
		expect(model).toEqual({ caption: 'Color depends on Status, End Date', entries: [] });
	});
});
