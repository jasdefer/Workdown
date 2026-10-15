import { describe, it, expect } from 'vitest';
import type { FieldDefinitionData } from '$lib/api/generated/FieldDefinitionData';
import type { RuleShape } from '$lib/api/generated/RuleShape';
import {
	anchorsOf,
	comparableFields,
	emptyMatchRow,
	emptyRequireRow,
	initialRuleDraft,
	joinReference,
	listSpecOfField,
	choiceOfTest,
	matchTestChoices,
	testOfChoice,
	newRuleDraft,
	referenceKind,
	requireTestKinds,
	ruleProblem,
	ruleWriteBody,
	scalarOfValue,
	scalarsOfValue,
	specOfField,
	splitReference,
	withMatchReference,
	withRequireReference,
	withMatchRow,
	withoutMatchRow,
	withoutRequireRow,
	type Anchor,
	type RuleDraft
} from './ruleDraft';

const plain = { compute: null, when: null, pull: null, aggregate: null };

function field(
	name: string,
	field_type: FieldDefinitionData['field_type'],
	shape: FieldDefinitionData['shape'] = { kind: 'scalar' }
): FieldDefinitionData {
	return {
		name,
		field_type,
		required: false,
		description: null,
		default: null,
		resource: null,
		shape,
		derived: plain,
		aggregate: null
	};
}

const statusField = field('status', 'choice', { kind: 'values', values: ['open', 'done'] });
const pointsField = field('points', 'integer', { kind: 'numeric', min: 0, max: 100 });
const startField = field('start', 'date');

const fields: FieldDefinitionData[] = [
	field('id', 'string', { kind: 'text', pattern: null }),
	statusField,
	pointsField,
	startField,
	field('end', 'date'),
	field('parent', 'link', { kind: 'relation', allow_cycles: false, inverse: 'children' }),
	field('depends_on', 'links', { kind: 'relation', allow_cycles: false, inverse: null })
];

const anchors: Anchor[] = anchorsOf(fields);

const validShape: RuleShape = {
	match_rows: [{ reference: 'status', quantifier: null, test: { kind: 'equals', value: 'open' } }],
	require_rows: [{ reference: 'points', test: { kind: 'required' } }],
	count: null
};

function draft(overrides: Partial<RuleDraft> = {}): RuleDraft {
	return {
		name: 'open-needs-points',
		severity: 'error',
		description: '',
		shape: validShape,
		...overrides
	};
}

describe('anchorsOf', () => {
	it('lists each relation field and its inverse, many for links and every inverse', () => {
		expect(anchors).toEqual([
			{ name: 'parent', many: false },
			{ name: 'children', many: true },
			{ name: 'depends_on', many: true }
		]);
	});
});

describe('splitReference and joinReference', () => {
	it('splits a dotted reference into anchor and field', () => {
		expect(splitReference('children.status', fields, anchors)).toEqual({
			anchor: 'children',
			field: 'status'
		});
	});

	it('reads a bare field name as the field on this item, even a links field', () => {
		expect(splitReference('depends_on', fields, anchors)).toEqual({
			anchor: null,
			field: 'depends_on'
		});
	});

	it('reads a bare inverse as the anchor alone', () => {
		expect(splitReference('children', fields, anchors)).toEqual({
			anchor: 'children',
			field: null
		});
	});

	it('keeps an unknown bare name as a field, so it shows as stray', () => {
		expect(splitReference('gone', fields, anchors)).toEqual({ anchor: null, field: 'gone' });
	});

	it('joins back to the served spelling and null for nothing picked', () => {
		expect(joinReference({ anchor: 'children', field: 'status' })).toBe('children.status');
		expect(joinReference({ anchor: 'children', field: null })).toBe('children');
		expect(joinReference({ anchor: null, field: 'status' })).toBe('status');
		expect(joinReference({ anchor: null, field: null })).toBeNull();
	});
});

describe('referenceKind', () => {
	it('tells one value, one per related item, the related items, and nothing', () => {
		expect(referenceKind({ anchor: null, field: 'status' }, anchors)).toBe('single');
		expect(referenceKind({ anchor: 'parent', field: 'status' }, anchors)).toBe('single');
		expect(referenceKind({ anchor: 'children', field: 'status' }, anchors)).toBe('many');
		expect(referenceKind({ anchor: 'children', field: null }, anchors)).toBe('many_bare');
		expect(referenceKind({ anchor: 'parent', field: null }, anchors)).toBe('incomplete');
		expect(referenceKind({ anchor: null, field: null }, anchors)).toBe('incomplete');
	});
});

describe('which tests a reference takes', () => {
	it('offers every match test on a value and only set-or-not on the related items', () => {
		const every = ['equals', 'one_of', 'not', 'is_set', 'is_not_set'];
		expect(matchTestChoices('single')).toEqual(every);
		expect(matchTestChoices('many')).toEqual(every);
		expect(matchTestChoices('many_bare')).toEqual(['is_set', 'is_not_set']);
		expect(matchTestChoices('incomplete')).toEqual([]);
	});

	it('shows is_set as two choices and reads them back', () => {
		expect(choiceOfTest({ kind: 'is_set', value: true })).toBe('is_set');
		expect(choiceOfTest({ kind: 'is_set', value: false })).toBe('is_not_set');
		expect(choiceOfTest({ kind: 'not', values: [] })).toBe('not');
		expect(testOfChoice('is_not_set')).toEqual({ kind: 'is_set', value: false });
		expect(testOfChoice('one_of')).toEqual({ kind: 'one_of', values: [] });
	});

	it('offers require tests by target type and counts only on the related items', () => {
		expect(requireTestKinds('single', 'choice')).toEqual([
			'required',
			'forbidden',
			'values',
			'not'
		]);
		expect(requireTestKinds('single', 'date')).toEqual([
			'required',
			'forbidden',
			'values',
			'not',
			'compare'
		]);
		expect(requireTestKinds('many', 'links')).toEqual(['required', 'forbidden']);
		expect(requireTestKinds('many_bare', null)).toEqual(['min_count', 'max_count']);
		expect(requireTestKinds('incomplete', null)).toEqual([]);
	});

	it('compares with the other fields of the same type, plus $today for dates', () => {
		expect(comparableFields(fields, startField)).toEqual(['$today', 'end']);
		expect(comparableFields(fields, pointsField)).toEqual([]);
	});
});

describe('value specs and conversions', () => {
	it('builds the editor spec from the field, free text without one', () => {
		expect(specOfField(statusField)).toEqual({
			fieldType: 'choice',
			required: false,
			values: ['open', 'done'],
			min: null,
			max: null
		});
		expect(specOfField(pointsField).min).toBe(0);
		expect(specOfField(null).fieldType).toBe('string');
	});

	it('edits a list of a choice as checkboxes and of anything else as tags', () => {
		expect(listSpecOfField(statusField)).toMatchObject({
			fieldType: 'multichoice',
			values: ['open', 'done']
		});
		expect(listSpecOfField(pointsField).fieldType).toBe('list');
	});

	it('reads one value from the editor, dropping lists and clears', () => {
		expect(scalarOfValue('open')).toBe('open');
		expect(scalarOfValue(3)).toBe(3);
		expect(scalarOfValue(true)).toBe(true);
		expect(scalarOfValue(BigInt(90))).toBe(90);
		expect(scalarOfValue(['a'])).toBeNull();
		expect(scalarOfValue(null)).toBeNull();
	});

	it('reads tags back as the target type where the text allows', () => {
		expect(scalarsOfValue(['1', 'x', '2.5'], 'integer')).toEqual([1, 'x', 2.5]);
		expect(scalarsOfValue(['true', 'no'], 'boolean')).toEqual([true, 'no']);
		expect(scalarsOfValue(['open'], 'choice')).toEqual(['open']);
		expect(scalarsOfValue('open', 'choice')).toEqual([]);
	});
});

describe('rows after a reference change', () => {
	it('adds a quantifier for a many reference and drops it for a single one', () => {
		const row = emptyMatchRow();
		const many = withMatchReference(row, { anchor: 'children', field: 'status' }, anchors);
		expect(many).toEqual({
			reference: 'children.status',
			quantifier: 'any',
			test: { kind: 'equals', value: '' }
		});
		const single = withMatchReference(many, { anchor: null, field: 'status' }, anchors);
		expect(single.quantifier).toBeNull();
		expect(single.test).toEqual({ kind: 'equals', value: '' });
	});

	it('resets a match test the new reference cannot take', () => {
		const row = withMatchReference(emptyMatchRow(), { anchor: 'children', field: null }, anchors);
		expect(row).toEqual({
			reference: 'children',
			quantifier: null,
			test: { kind: 'is_set', value: true }
		});
	});

	it('keeps a require test the new reference still takes and resets one it cannot', () => {
		const compare = withRequireReference(
			{ reference: 'start', test: { kind: 'compare', comparison: 'lt', field: 'end' } },
			{ anchor: null, field: 'end' },
			anchors,
			'date'
		);
		expect(compare.test).toEqual({ kind: 'compare', comparison: 'lt', field: 'end' });
		const bare = withRequireReference(compare, { anchor: 'children', field: null }, anchors, null);
		expect(bare).toEqual({ reference: 'children', test: { kind: 'min_count', count: 1 } });
	});

	it('replaces and removes rows by position', () => {
		const added = withMatchRow(validShape, 0, emptyMatchRow());
		expect(added.match_rows[0]?.reference).toBe('');
		expect(withoutMatchRow(validShape, 0).match_rows).toEqual([]);
		expect(withoutRequireRow(validShape, 0).require_rows).toEqual([]);
		expect(emptyRequireRow()).toEqual({ reference: '', test: { kind: 'required' } });
	});
});

describe('drafts and the write body', () => {
	it('opens an existing rule as its header and rows, and a new one empty', () => {
		const opened = initialRuleDraft(
			{ name: 'r', severity: 'warning', description: null },
			validShape
		);
		expect(opened).toEqual({ name: 'r', severity: 'warning', description: '', shape: validShape });
		expect(newRuleDraft().shape).toEqual({ match_rows: [], require_rows: [], count: null });
	});

	it('sends blank text as no description and trims the name', () => {
		expect(ruleWriteBody(draft({ name: ' r ', description: '  ' }))).toEqual({
			name: 'r',
			description: null,
			severity: 'error',
			shape: validShape
		});
	});
});

describe('ruleProblem', () => {
	it('accepts a complete draft', () => {
		expect(ruleProblem(draft(), ['other'], null)).toBeNull();
	});

	it('judges the name: blank, not kebab-case, taken, but not its own', () => {
		expect(ruleProblem(draft({ name: '' }), [], null)).toBe('Enter a name.');
		expect(ruleProblem(draft({ name: 'Open_Needs' }), [], null)).toContain('kebab-case');
		expect(ruleProblem(draft(), ['open-needs-points'], null)).toBe(
			"A rule named 'open-needs-points' already exists."
		);
		expect(ruleProblem(draft(), ['open-needs-points'], 'open-needs-points')).toBeNull();
	});

	it('wants every row complete', () => {
		const noField = { ...validShape, match_rows: [emptyMatchRow()] };
		expect(ruleProblem(draft({ shape: noField }), [], null)).toBe('Every condition needs a field.');
		const noValue = {
			...validShape,
			match_rows: [
				{ reference: 'status', quantifier: null, test: { kind: 'one_of' as const, values: [] } }
			]
		};
		expect(ruleProblem(draft({ shape: noValue }), [], null)).toContain('at least one value');
		const noCompare = {
			...validShape,
			require_rows: [
				{
					reference: 'end',
					test: { kind: 'compare' as const, comparison: 'gte' as const, field: '' }
				}
			]
		};
		expect(ruleProblem(draft({ shape: noCompare }), [], null)).toBe(
			'Pick a field to compare with.'
		);
	});

	it('refuses the same test twice on one reference', () => {
		const twice = {
			...validShape,
			match_rows: [
				{ reference: 'status', quantifier: null, test: { kind: 'not' as const, values: ['open'] } },
				{ reference: 'status', quantifier: null, test: { kind: 'not' as const, values: ['done'] } }
			]
		};
		expect(ruleProblem(draft({ shape: twice }), [], null)).toBe(
			"'status' has the same test twice."
		);
	});

	it('wants something to require, and a bound on a count', () => {
		const nothing = { ...validShape, require_rows: [], count: null };
		expect(ruleProblem(draft({ shape: nothing }), [], null)).toBe('Add a requirement or a count.');
		const unbounded = { ...validShape, require_rows: [], count: { min: null, max: null } };
		expect(ruleProblem(draft({ shape: unbounded }), [], null)).toBe(
			'The count needs a minimum or a maximum.'
		);
		const counted = { ...validShape, require_rows: [], count: { min: null, max: 3 } };
		expect(ruleProblem(draft({ shape: counted }), [], null)).toBeNull();
	});
});
