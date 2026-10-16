import { describe, it, expect } from 'vitest';
import { specOf, specOfFieldSchema } from './valueSpec';

describe('specOfFieldSchema', () => {
	it('carries the field type, requirement, values and bounds through', () => {
		expect(
			specOfFieldSchema({
				name: 'points',
				field_type: 'integer',
				description: null,
				required: true,
				values: null,
				min: 0,
				max: 10,
				duration_min: null,
				duration_max: null,
				pattern: null,
				resource: null,
				aggregate: false
			})
		).toEqual({ fieldType: 'integer', required: true, values: [], min: 0, max: 10 });
	});
});

describe('specOf', () => {
	it('hands the value editor the choice values', () => {
		expect(specOf('choice', { kind: 'values', values: ['a', 'b'] })).toEqual({
			fieldType: 'choice',
			required: false,
			values: ['a', 'b'],
			min: null,
			max: null
		});
	});

	it('hands the value editor the numeric bounds', () => {
		expect(specOf('float', { kind: 'numeric', min: 0.5, max: null })).toEqual({
			fieldType: 'float',
			required: false,
			values: [],
			min: 0.5,
			max: null
		});
	});

	it('is unconstrained without a shape and never required', () => {
		expect(specOf('duration', null)).toEqual({
			fieldType: 'duration',
			required: false,
			values: [],
			min: null,
			max: null
		});
		expect(specOf('date', { kind: 'scalar' }).required).toBe(false);
	});
});
