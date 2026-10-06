import { describe, it, expect } from 'vitest';
import type { FieldDefinitionData } from '$lib/api/generated/FieldDefinitionData';
import {
	emptyShape,
	generatorsFor,
	initialDraft,
	newDraft,
	retype,
	settingsWithoutEditor,
	shapeKindOf,
	shapeProblem,
	specOfDraft,
	takesResource,
	valueProblem,
	withBound,
	withPattern,
	withValueAdded,
	withValueMoved,
	withValueRemoved,
	withValueRenamed,
	writeBody,
	type BoundedShape,
	type FieldDraft,
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
	]
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
			shape: { kind: 'numeric', min: 0, max: 100 }
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
			shape: { kind: 'values', values: [] }
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
		shape: { kind: 'text', pattern: '^[a-z]+$' }
	};

	it('keeps the header, drops the default and swaps the shape', () => {
		expect(retype(tables, draft, 'date')).toEqual({
			name: 'owner',
			fieldType: 'date',
			required: true,
			description: 'Who has it',
			default: null,
			resource: null,
			shape: { kind: 'scalar' }
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

	it('leave the recipe keys and the edited settings out of those edited in the file', () => {
		expect(settingsWithoutEditor(tables, 'link')).toEqual(['allow_cycles', 'inverse']);
		expect(settingsWithoutEditor(tables, 'string')).toEqual([]);
		expect(settingsWithoutEditor(tables, 'choice')).toEqual([]);
		expect(settingsWithoutEditor(tables, 'integer')).toEqual([]);
		expect(settingsWithoutEditor(tables, 'duration')).toEqual([]);
		expect(settingsWithoutEditor(tables, 'date')).toEqual([]);
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
		shape: { kind: 'numeric', min: 0, max: null }
	};

	it('sends the plain properties without the name, trimmed', () => {
		expect(writeBody(draft)).toEqual({
			field_type: 'integer',
			required: true,
			description: 'Story points',
			default: { kind: 'literal', value: 3 },
			resource: null,
			shape: { kind: 'numeric', min: 0, max: null }
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
