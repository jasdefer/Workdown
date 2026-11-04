import { describe, expect, it } from 'vitest';
import { stringsOf } from './fieldValue';

describe('stringsOf', () => {
	it('keeps the string entries of a list', () => {
		expect(stringsOf(['auth-epic', 'login'])).toEqual(['auth-epic', 'login']);
	});

	it('wraps a non-empty string', () => {
		expect(stringsOf('auth-epic')).toEqual(['auth-epic']);
	});

	it('names nothing for an absent or empty value', () => {
		expect(stringsOf(null)).toEqual([]);
		expect(stringsOf('')).toEqual([]);
		expect(stringsOf([])).toEqual([]);
	});

	it('names nothing for a value of another type', () => {
		expect(stringsOf(42)).toEqual([]);
		expect(stringsOf(true)).toEqual([]);
	});
});
