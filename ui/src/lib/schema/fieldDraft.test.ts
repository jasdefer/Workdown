import { describe, it, expect } from 'vitest';
import type { AggregateConfig } from '$lib/api/generated/AggregateConfig';
import type { FieldDefinitionData } from '$lib/api/generated/FieldDefinitionData';
import type { Operand } from '$lib/api/generated/Operand';
import type { PullConfig } from '$lib/api/generated/PullConfig';
import type { WhenBranchData } from '$lib/api/generated/WhenBranchData';
import {
	aggregateFunctionsFor,
	choicesFacing,
	emptyShape,
	expressionReferences,
	forbidsCycles,
	generatorsFor,
	initialDraft,
	insertAt,
	literalOf,
	literalSpecFor,
	newBranch,
	newCompute,
	newDraft,
	newPull,
	newRollup,
	newWhen,
	operandChoices,
	operandKey,
	pullRelations,
	pullSources,
	retype,
	rollupRelations,
	shapeKindOf,
	shapeProblem,
	specOfDraft,
	takesAggregate,
	takesCompute,
	takesLiteral,
	takesPull,
	takesResource,
	takesWhen,
	valueOfLiteral,
	valueProblem,
	withBound,
	withBranch,
	withBranchAdded,
	withBranchMoved,
	withBranchRemoved,
	withForbidCycles,
	withInverse,
	withOperand,
	withPattern,
	withPullSource,
	withValueAdded,
	withValueMoved,
	withValueRemoved,
	withValueRenamed,
	writeBody,
	type BoundedShape,
	type FieldDraft,
	type RelationShape,
	type TypeTables,
	type ValuesShape
} from './fieldDraft';

/** The rows the editor reads, as the server's tables spell them. */
const tables: TypeTables = {
	properties_by_type: [
		{ field_type: 'string', properties: ['pattern', 'resource'], shape_kind: 'text' },
		{ field_type: 'choice', properties: ['values'], shape_kind: 'values' },
		{
			field_type: 'integer',
			properties: ['min', 'max', 'aggregate', 'compute', 'pull'],
			shape_kind: 'numeric'
		},
		{ field_type: 'float', properties: ['min', 'max', 'compute'], shape_kind: 'numeric' },
		{ field_type: 'date', properties: ['compute'], shape_kind: 'scalar' },
		{ field_type: 'duration', properties: ['min', 'max'], shape_kind: 'duration' },
		{ field_type: 'color', properties: [], shape_kind: 'scalar' },
		{ field_type: 'list', properties: ['resource'], shape_kind: 'scalar' },
		{ field_type: 'link', properties: ['allow_cycles', 'inverse'], shape_kind: 'relation' }
	],
	generators_by_type: [
		{ field_type: 'string', generators: ['$filename', '$filename_pretty', '$uuid'] },
		{ field_type: 'date', generators: ['$today'] },
		{ field_type: 'integer', generators: ['$max_plus_one'] },
		{ field_type: 'color', generators: [] }
	],
	aggregate_functions_by_type: [
		{ field_type: 'integer', functions: ['sum', 'min', 'max', 'average', 'median', 'count'] },
		{ field_type: 'date', functions: ['min', 'max', 'average'] },
		{ field_type: 'string', functions: [] }
	],
	pull_sources_by_type: [
		{
			field_type: 'integer',
			sources: [
				{ field_type: 'integer', functions: ['sum', 'min', 'max', 'count'] },
				{ field_type: 'date', functions: ['count'] }
			]
		},
		{ field_type: 'date', sources: [{ field_type: 'date', functions: ['min', 'max', 'average'] }] },
		{ field_type: 'string', sources: [] }
	],
	expression_readable_types: [
		'string',
		'choice',
		'integer',
		'float',
		'date',
		'duration',
		'boolean'
	],
	conditional_types: ['string', 'choice', 'integer', 'float', 'date', 'duration', 'color', 'list']
};

function served(overrides: Partial<FieldDefinitionData>): FieldDefinitionData {
	return {
		name: 'points',
		field_type: 'integer',
		required: false,
		description: null,
		default: null,
		resource: null,
		shape: { kind: 'numeric', min: 0, max: 100 },
		derived: { compute: null, when: null, pull: null, aggregate: null },
		aggregate: null,
		recipe: null,
		...overrides
	};
}

describe('initialDraft', () => {
	it('copies the plain properties and blanks a missing description', () => {
		const draft = initialDraft(served({}));
		expect(draft).toEqual({
			name: 'points',
			fieldType: 'integer',
			required: false,
			description: '',
			default: null,
			resource: null,
			shape: { kind: 'numeric', min: 0, max: 100 },
			recipe: null,
			aggregate: null
		});
	});

	it('keeps a generator default as a generator', () => {
		const draft = initialDraft(served({ default: { kind: 'generator', generator: '$today' } }));
		expect(draft.default).toEqual({ kind: 'generator', generator: '$today' });
	});

	it('keeps a literal default as its value', () => {
		const draft = initialDraft(served({ default: { kind: 'literal', value: 3 } }));
		expect(draft.default).toEqual({ kind: 'literal', value: 3 });
	});

	it('shows an invalid default as a literal of its text', () => {
		const draft = initialDraft(
			served({ default: { kind: 'invalid', text: 'lots', reason: 'not an integer' } })
		);
		expect(draft.default).toEqual({ kind: 'literal', value: 'lots' });
	});
});

describe('newDraft', () => {
	it('starts blank with the empty shape of the type', () => {
		expect(newDraft(tables, 'choice')).toEqual({
			name: '',
			fieldType: 'choice',
			required: false,
			description: '',
			default: null,
			resource: null,
			shape: { kind: 'values', values: [] },
			recipe: null,
			aggregate: null
		});
	});
});

describe('retype', () => {
	const draft: FieldDraft = {
		name: 'owner',
		fieldType: 'string',
		required: true,
		description: 'Who has it',
		default: { kind: 'generator', generator: '$filename' },
		resource: 'people',
		shape: { kind: 'text', pattern: '^[a-z]+$' },
		recipe: null,
		aggregate: null
	};

	it('keeps the header, drops the default and swaps the shape', () => {
		expect(retype(tables, draft, 'date')).toEqual({
			name: 'owner',
			fieldType: 'date',
			required: true,
			description: 'Who has it',
			default: null,
			resource: null,
			shape: { kind: 'scalar' },
			recipe: null,
			aggregate: null
		});
	});

	it('keeps the shape across types edited as the same shape', () => {
		const numeric: FieldDraft = {
			...draft,
			fieldType: 'integer',
			shape: { kind: 'numeric', min: 1, max: 5 }
		};
		expect(retype(tables, numeric, 'float').shape).toEqual({ kind: 'numeric', min: 1, max: 5 });
	});

	it('keeps the resource where the new type takes one', () => {
		expect(retype(tables, draft, 'list').resource).toBe('people');
	});

	it('keeps the rollup only where the new type takes one', () => {
		const rollup: AggregateConfig = { function: 'sum', error_on_missing: false, over: 'parent' };
		const total: FieldDraft = {
			...draft,
			fieldType: 'float',
			shape: { kind: 'numeric', min: null, max: null },
			recipe: null,
			aggregate: rollup
		};
		expect(retype(tables, total, 'integer').aggregate).toEqual(rollup);
		expect(retype(tables, total, 'string').aggregate).toBeNull();
	});
});

describe('emptyShape', () => {
	it('has every setting unset for every kind', () => {
		expect(emptyShape('scalar')).toEqual({ kind: 'scalar' });
		expect(emptyShape('numeric')).toEqual({ kind: 'numeric', min: null, max: null });
		expect(emptyShape('duration')).toEqual({ kind: 'duration', min: null, max: null });
		expect(emptyShape('text')).toEqual({ kind: 'text', pattern: null });
		expect(emptyShape('values')).toEqual({ kind: 'values', values: [] });
		expect(emptyShape('relation')).toEqual({ kind: 'relation', allow_cycles: null, inverse: null });
	});
});

describe('the type tables', () => {
	it('answer the shape kind from the served row', () => {
		expect(shapeKindOf(tables, 'duration')).toBe('duration');
	});

	it('refuse a type the payload has no row for', () => {
		expect(() => shapeKindOf(tables, 'links')).toThrow(/no properties row for type 'links'/);
	});

	it('list the generators of a type, none for a type without', () => {
		expect(generatorsFor(tables, 'date')).toEqual(['$today']);
		expect(generatorsFor(tables, 'color')).toEqual([]);
		expect(generatorsFor(tables, 'link')).toEqual([]);
	});

	it('say which types may name a resource', () => {
		expect(takesResource(tables, 'string')).toBe(true);
		expect(takesResource(tables, 'list')).toBe(true);
		expect(takesResource(tables, 'integer')).toBe(false);
	});
});

describe('withPattern', () => {
	it('sets the pattern as typed and clears it on blank', () => {
		const shape = { kind: 'text', pattern: null } as const;
		expect(withPattern(shape, '^[a-z-]+$')).toEqual({ kind: 'text', pattern: '^[a-z-]+$' });
		expect(withPattern({ kind: 'text', pattern: '^x' }, '   ')).toEqual(shape);
	});
});

describe('the relation shape', () => {
	const unset: RelationShape = { kind: 'relation', allow_cycles: null, inverse: null };

	it('reads only an explicit false as forbidding cycles', () => {
		expect(forbidsCycles({ ...unset, allow_cycles: false })).toBe(true);
		expect(forbidsCycles({ ...unset, allow_cycles: true })).toBe(false);
		expect(forbidsCycles(unset)).toBe(false);
	});

	it('writes false when forbidding and removes the key otherwise', () => {
		expect(withForbidCycles(unset, true)).toEqual({ ...unset, allow_cycles: false });
		expect(withForbidCycles({ ...unset, allow_cycles: false }, false)).toEqual(unset);
	});

	it('sets the inverse trimmed and clears it on blank', () => {
		expect(withInverse(unset, '  children ')).toEqual({ ...unset, inverse: 'children' });
		expect(withInverse({ ...unset, inverse: 'children' }, '   ')).toEqual(unset);
	});

	it('leaves the inverse name to the server', () => {
		expect(shapeProblem({ ...unset, inverse: 'Not An Identifier' })).toBeNull();
	});
});

describe('withBound', () => {
	it('sets a numeric bound from the editor and clears it on null', () => {
		const shape: BoundedShape = { kind: 'numeric', min: null, max: 10 };
		expect(withBound(shape, 'min', 2)).toEqual({ kind: 'numeric', min: 2, max: 10 });
		expect(withBound(shape, 'max', null)).toEqual({ kind: 'numeric', min: null, max: null });
	});

	it('sets a duration bound as the shorthand typed and clears it on blank', () => {
		const shape: BoundedShape = { kind: 'duration', min: null, max: null };
		expect(withBound(shape, 'max', '2w 1d')).toEqual({ kind: 'duration', min: null, max: '2w 1d' });
		expect(withBound({ ...shape, min: '1h' }, 'min', '')).toEqual(shape);
	});

	it('treats a value of the wrong kind as cleared', () => {
		expect(withBound({ kind: 'numeric', min: 1, max: null }, 'min', 'two').min).toBeNull();
	});
});

describe('the value list', () => {
	const shape: ValuesShape = { kind: 'values', values: ['open', 'in_progress', 'done'] };

	it('refuses blank text and a value already listed, trimmed', () => {
		expect(valueProblem(shape, '   ')).toBe('Enter a value.');
		expect(valueProblem(shape, ' done ')).toBe("'done' is already in the list.");
		expect(valueProblem(shape, 'review')).toBeNull();
	});

	it('lets a rename keep its own value and refuses another row’s', () => {
		expect(valueProblem(shape, 'done', 2)).toBeNull();
		expect(valueProblem(shape, 'done', 0)).toBe("'done' is already in the list.");
	});

	it('appends, renames in place and removes, trimming the text', () => {
		expect(withValueAdded(shape, ' review ').values).toEqual([
			'open',
			'in_progress',
			'done',
			'review'
		]);
		expect(withValueRenamed(shape, 1, ' active ').values).toEqual(['open', 'active', 'done']);
		expect(withValueRemoved(shape, 0).values).toEqual(['in_progress', 'done']);
	});

	it('swaps a value with its neighbour and stays put at the ends', () => {
		expect(withValueMoved(shape, 1, 'up').values).toEqual(['in_progress', 'open', 'done']);
		expect(withValueMoved(shape, 1, 'down').values).toEqual(['open', 'done', 'in_progress']);
		expect(withValueMoved(shape, 0, 'up')).toBe(shape);
		expect(withValueMoved(shape, 2, 'down')).toBe(shape);
	});
});

describe('shapeProblem', () => {
	it('names an inverted numeric pair and accepts an equal one', () => {
		expect(shapeProblem({ kind: 'numeric', min: 10, max: 5 })).toBe('Min must not exceed max.');
		expect(shapeProblem({ kind: 'numeric', min: 5, max: 5 })).toBeNull();
		expect(shapeProblem({ kind: 'numeric', min: null, max: 5 })).toBeNull();
	});

	it('wants at least one value in a value list', () => {
		expect(shapeProblem({ kind: 'values', values: [] })).toBe('Add at least one value.');
		expect(shapeProblem({ kind: 'values', values: ['open'] })).toBeNull();
	});

	it('leaves duration bounds and patterns to the server', () => {
		expect(shapeProblem({ kind: 'duration', min: '4w', max: '1d' })).toBeNull();
		expect(shapeProblem({ kind: 'text', pattern: '(' })).toBeNull();
	});
});

describe('writeBody', () => {
	const draft: FieldDraft = {
		name: 'points',
		fieldType: 'integer',
		required: true,
		description: '  Story points  ',
		default: { kind: 'literal', value: 3 },
		resource: null,
		shape: { kind: 'numeric', min: 0, max: null },
		recipe: null,
		aggregate: null
	};

	it('sends the plain properties without the name, trimmed', () => {
		expect(writeBody(draft)).toEqual({
			field_type: 'integer',
			required: true,
			description: 'Story points',
			default: { kind: 'literal', value: 3 },
			resource: null,
			shape: { kind: 'numeric', min: 0, max: null },
			recipe: null,
			aggregate: null
		});
	});

	it('sends blank text as none', () => {
		const body = writeBody({ ...draft, description: '   ', resource: '' });
		expect(body.description).toBeNull();
		expect(body.resource).toBeNull();
	});

	it('sends a generator default as a generator', () => {
		const body = writeBody({
			...draft,
			default: { kind: 'generator', generator: '$max_plus_one' }
		});
		expect(body.default).toEqual({ kind: 'generator', generator: '$max_plus_one' });
	});

	it('sends a fixed-value choice with nothing entered as no default', () => {
		expect(writeBody({ ...draft, default: { kind: 'literal', value: null } }).default).toBeNull();
	});

	it('sends the rollup as it came', () => {
		const rollup: AggregateConfig = { function: 'max', error_on_missing: true, over: 'parent' };
		expect(writeBody({ ...draft, aggregate: rollup }).aggregate).toEqual(rollup);
	});
});

describe('rollups', () => {
	const parent = served({
		name: 'parent',
		field_type: 'link',
		shape: { kind: 'relation', allow_cycles: false, inverse: 'children' }
	});
	const related = served({
		name: 'related_to',
		field_type: 'links',
		shape: { kind: 'relation', allow_cycles: false, inverse: null }
	});
	const loose = served({
		name: 'follows',
		field_type: 'link',
		shape: { kind: 'relation', allow_cycles: null, inverse: null }
	});

	it('offers a rollup to the types with aggregate functions', () => {
		expect(takesAggregate(tables, 'integer')).toBe(true);
		expect(takesAggregate(tables, 'string')).toBe(false);
	});

	it('reads the functions from the served table', () => {
		expect(aggregateFunctionsFor(tables, 'date')).toEqual(['min', 'max', 'average']);
		expect(aggregateFunctionsFor(tables, 'color')).toEqual([]);
	});

	it('climbs only link fields that forbid cycles', () => {
		expect(rollupRelations([related, loose, parent, served({})])).toEqual(['parent']);
	});

	it('starts with the first function over the first relation', () => {
		expect(newRollup(tables, [parent], 'integer')).toEqual({
			function: 'sum',
			error_on_missing: false,
			over: 'parent'
		});
	});

	it('has nothing to start with when no relation can be climbed or the type has no functions', () => {
		expect(newRollup(tables, [loose], 'integer')).toBeNull();
		expect(newRollup(tables, [parent], 'string')).toBeNull();
	});
});

describe('formulas', () => {
	const points = served({ name: 'points', field_type: 'integer' });
	const due = served({ name: 'due', field_type: 'date', shape: { kind: 'scalar' } });
	const tags = served({ name: 'tags', field_type: 'list', shape: { kind: 'scalar' } });
	const parent = served({
		name: 'parent',
		field_type: 'link',
		shape: { kind: 'relation', allow_cycles: false, inverse: null }
	});

	it('offers a formula to the types the server lists it for', () => {
		expect(takesCompute(tables, 'integer')).toBe(true);
		expect(takesCompute(tables, 'string')).toBe(false);
	});

	it('starts empty with no options', () => {
		expect(newCompute()).toEqual({
			kind: 'compute',
			expression: '',
			round: 'nearest',
			error_on_missing: false
		});
	});

	it('lists the readable fields but itself, then the constants, then today', () => {
		const constants = [{ name: 'daily_rate', value: 800 }];
		expect(expressionReferences(tables, [points, due, tags, parent], constants, 'due')).toEqual([
			{ text: 'points', detail: 'integer' },
			{ text: '$constants.daily_rate', detail: '= 800' },
			{ text: '$today', detail: 'the date the project is evaluated on' }
		]);
		expect(expressionReferences(tables, [], [], null)).toEqual([
			{ text: '$today', detail: 'the date the project is evaluated on' }
		]);
	});

	it('inserts a token with the spaces a run-on would need', () => {
		expect(insertAt('', 0, 0, 'points')).toEqual({ expression: 'points', caret: 6 });
		expect(insertAt('points *', 8, 8, '2')).toEqual({ expression: 'points * 2', caret: 10 });
		expect(insertAt('points * ', 9, 9, '2')).toEqual({ expression: 'points * 2', caret: 10 });
		expect(insertAt('(points)', 1, 1, 'due')).toEqual({ expression: '(due points)', caret: 5 });
		expect(insertAt('a + b', 4, 5, 'c')).toEqual({ expression: 'a + c', caret: 5 });
		expect(insertAt('a', 9, 9, 'b')).toEqual({ expression: 'a b', caret: 3 });
	});
});

describe('conditions', () => {
	const status = served({
		name: 'status',
		field_type: 'choice',
		shape: { kind: 'values', values: ['open', 'done'] }
	});
	const points = served({ name: 'points', field_type: 'integer' });
	const tags = served({ name: 'tags', field_type: 'list', shape: { kind: 'scalar' } });
	const fields = [status, points, tags];
	const constants = [
		{ name: 'limit', value: 3 },
		{ name: 'ratio', value: 0.5 },
		{ name: 'strict', value: true }
	];
	const choices = operandChoices(tables, fields, constants, 'points');

	it('offers conditions to the types the server lists', () => {
		expect(takesWhen(tables, 'color')).toBe(true);
		expect(takesWhen(tables, 'link')).toBe(false);
	});

	it('lists the readable fields but itself, the constants, today and a value', () => {
		expect(choices.map((choice) => choice.key)).toEqual([
			'field:status',
			'constant:limit',
			'constant:ratio',
			'constant:strict',
			'today',
			'literal'
		]);
		expect(choices[0]?.operand).toEqual({ kind: 'field', name: 'status' });
		for (const choice of choices) expect(operandKey(choice.operand)).toBe(choice.key);
	});

	it('starts with the first choice compared for equality against a value', () => {
		expect(newWhen(choices)).toEqual({
			kind: 'when',
			branches: [
				{
					condition: {
						kind: 'row',
						left: { kind: 'field', name: 'status' },
						operator: 'equal',
						right: { kind: 'literal', value: '' }
					},
					then: null
				}
			]
		});
		expect(newBranch([]).condition).toMatchObject({ left: { kind: 'today' } });
	});

	it('edits a literal with the other side in mind', () => {
		expect(literalSpecFor({ kind: 'field', name: 'status' }, fields, constants)).toMatchObject({
			fieldType: 'choice',
			values: ['open', 'done']
		});
		expect(literalSpecFor({ kind: 'constant', name: 'limit' }, fields, constants).fieldType).toBe(
			'integer'
		);
		expect(literalSpecFor({ kind: 'constant', name: 'ratio' }, fields, constants).fieldType).toBe(
			'float'
		);
		expect(literalSpecFor({ kind: 'constant', name: 'strict' }, fields, constants).fieldType).toBe(
			'boolean'
		);
		expect(literalSpecFor({ kind: 'today' }, fields, constants).fieldType).toBe('date');
		expect(literalSpecFor({ kind: 'literal', value: 1 }, fields, constants).fieldType).toBe(
			'string'
		);
		expect(literalSpecFor({ kind: 'field', name: 'gone' }, fields, constants).fieldType).toBe(
			'string'
		);
	});

	it('turns the value editor output into a literal and back', () => {
		expect(literalOf(3)).toBe(3);
		expect(literalOf('done')).toBe('done');
		expect(literalOf(true)).toBe(true);
		expect(literalOf(null)).toBe('');
		expect(literalOf(['a'])).toBe('');
		expect(valueOfLiteral(2.5)).toBe(2.5);
	});

	it('adds, moves and removes branches, order being meaning', () => {
		const first = newBranch(choices);
		const second: WhenBranchData = {
			condition: { kind: 'text', source: 'points > 1' },
			then: 'red'
		};
		const two = withBranchAdded(newWhen(choices), second);
		expect(two.branches).toEqual([first, second]);
		expect(withBranchMoved(two, 1, 'up').branches).toEqual([second, first]);
		expect(withBranchMoved(two, 1, 'down')).toBe(two);
		expect(withBranchRemoved(two, 0).branches).toEqual([second]);
		expect(withBranch(two, 1, { ...second, then: 'blue' }).branches[1]?.then).toBe('blue');
	});

	it('withholds a value where the grammar has no literal of that type', () => {
		const due = served({ name: 'due', field_type: 'date' });
		const tint = served({ name: 'tint', field_type: 'color' });
		const withDue = [status, points, due, tint, tags];
		expect(takesLiteral({ kind: 'field', name: 'status' }, withDue, constants)).toBe(true);
		expect(takesLiteral({ kind: 'field', name: 'tint' }, withDue, constants)).toBe(true);
		expect(takesLiteral({ kind: 'constant', name: 'limit' }, withDue, constants)).toBe(true);
		expect(takesLiteral({ kind: 'literal', value: 1 }, withDue, constants)).toBe(true);
		expect(takesLiteral({ kind: 'today' }, withDue, constants)).toBe(false);
		expect(takesLiteral({ kind: 'field', name: 'due' }, withDue, constants)).toBe(false);
		expect(choicesFacing(choices, { kind: 'field', name: 'status' }, withDue, constants)).toBe(
			choices
		);
		expect(
			choicesFacing(choices, { kind: 'today' }, withDue, constants).map((choice) => choice.key)
		).not.toContain('literal');
	});

	it('swaps a literal the picked operand cannot face for a partner of its type', () => {
		const due = served({ name: 'due', field_type: 'date' });
		const withDue = [status, points, due, tags];
		const dueOperand: Operand = { kind: 'field', name: 'due' };
		const row = newBranch(choices).condition;
		if (row.kind !== 'row') throw new Error('a fresh branch is a row');
		// A date field opposite "a value": the value becomes `$today`.
		expect(withOperand(row, 'left', dueOperand, choices, withDue, constants)).toEqual({
			kind: 'row',
			left: dueOperand,
			operator: 'equal',
			right: { kind: 'today' }
		});
		// `$today` opposite "a value" with no date field on offer: the
		// literal stays, and the server says why.
		expect(withOperand(row, 'left', { kind: 'today' }, choices, fields, constants).right).toEqual({
			kind: 'literal',
			value: ''
		});
		// A side that is not a literal is left alone.
		const dated: typeof row = { ...row, left: dueOperand, right: { kind: 'today' } };
		expect(
			withOperand(dated, 'right', { kind: 'field', name: 'points' }, choices, withDue, constants)
		).toEqual({ ...dated, right: { kind: 'field', name: 'points' } });
		// A literal the operand can face is kept.
		expect(
			withOperand(row, 'left', { kind: 'constant', name: 'limit' }, choices, withDue, constants)
				.right
		).toEqual({ kind: 'literal', value: '' });
	});
});

describe('pulls', () => {
	const parent = served({
		name: 'parent',
		field_type: 'link',
		shape: { kind: 'relation', allow_cycles: false, inverse: 'children' }
	});
	const dependsOn = served({
		name: 'depends_on',
		field_type: 'links',
		shape: { kind: 'relation', allow_cycles: false, inverse: null }
	});
	const loose = served({
		name: 'related_to',
		field_type: 'links',
		shape: { kind: 'relation', allow_cycles: true, inverse: null }
	});
	const points = served({ name: 'points', field_type: 'integer' });
	const due = served({ name: 'due', field_type: 'date', shape: { kind: 'scalar' } });
	const title = served({
		name: 'title',
		field_type: 'string',
		shape: { kind: 'text', pattern: null }
	});
	const fields = [title, parent, dependsOn, loose, points, due];

	it('offers a pull to the types the server lists it for', () => {
		expect(takesPull(tables, 'integer')).toBe(true);
		expect(takesPull(tables, 'date')).toBe(false);
	});

	it('follows link and links fields that forbid cycles', () => {
		expect(pullRelations(fields)).toEqual(['parent', 'depends_on']);
	});

	it('reads the fields whose type the served table lists, with its functions', () => {
		expect(pullSources(tables, fields, 'integer')).toEqual([
			{ name: 'points', functions: ['sum', 'min', 'max', 'count'] },
			{ name: 'due', functions: ['count'] }
		]);
		expect(pullSources(tables, fields, 'date')).toEqual([
			{ name: 'due', functions: ['min', 'max', 'average'] }
		]);
		expect(pullSources(tables, fields, 'string')).toEqual([]);
	});

	it('starts with the first relation, the first source and its first function', () => {
		expect(newPull(tables, fields, 'integer')).toEqual({
			over: 'parent',
			field: 'points',
			function: 'sum',
			error_on_missing: false
		});
	});

	it('has nothing to start with when no relation or no source fits', () => {
		expect(newPull(tables, [loose, points], 'integer')).toBeNull();
		expect(newPull(tables, [parent, title], 'integer')).toBeNull();
	});

	it('keeps the function across a source switch when the new source offers it', () => {
		const sources = pullSources(tables, fields, 'integer');
		const counted: PullConfig = {
			over: 'parent',
			field: 'points',
			function: 'count',
			error_on_missing: true
		};
		expect(withPullSource(counted, sources, 'due')).toEqual({ ...counted, field: 'due' });
		expect(withPullSource({ ...counted, function: 'sum' }, sources, 'due')).toEqual({
			...counted,
			field: 'due',
			function: 'count'
		});
	});

	it('is kept on retype only where the new type takes a pull', () => {
		const pulled: FieldDraft = {
			name: 'blocked_points',
			fieldType: 'integer',
			required: false,
			description: '',
			default: null,
			resource: null,
			shape: { kind: 'numeric', min: null, max: null },
			aggregate: null,
			recipe: {
				kind: 'pull',
				over: 'parent',
				field: 'points',
				function: 'sum',
				error_on_missing: false
			}
		};
		expect(retype(tables, pulled, 'integer').recipe).toEqual(pulled.recipe);
		expect(retype(tables, pulled, 'date').recipe).toBeNull();
	});
});

describe('specOfDraft', () => {
	it('hands the value editor the choice values', () => {
		expect(
			specOfDraft({ fieldType: 'choice', shape: { kind: 'values', values: ['a', 'b'] } })
		).toEqual({ fieldType: 'choice', required: false, values: ['a', 'b'], min: null, max: null });
	});

	it('hands the value editor the numeric bounds', () => {
		expect(
			specOfDraft({ fieldType: 'float', shape: { kind: 'numeric', min: 0.5, max: null } })
		).toEqual({ fieldType: 'float', required: false, values: [], min: 0.5, max: null });
	});

	it('is never required, so clearing means no default', () => {
		expect(specOfDraft({ fieldType: 'date', shape: { kind: 'scalar' } }).required).toBe(false);
	});
});
