import { describe, expect, it } from 'vitest';
import type { FieldDerivation } from '$lib/api/generated/FieldDerivation';
import { originBadge } from './valueOrigin';

function derivation(
	mechanism: FieldDerivation['mechanism'],
	state: FieldDerivation['state']
): FieldDerivation {
	return { field: 'cost', mechanism, state };
}

describe('originBadge', () => {
	it('shows nothing for a plain field', () => {
		expect(originBadge(undefined)).toBeNull();
	});

	it('names the mechanism that supplied a derived value', () => {
		const badge = originBadge(derivation('aggregated', 'derived'));
		expect(badge?.label).toBe('aggregated');
		expect(badge?.title).toContain('rolled up from the items below');
		expect(badge?.restoreLabel).toBeNull();
	});

	it('marks a written value that overrides a mechanism, and offers the way back', () => {
		const badge = originBadge(derivation('computed', 'overridden'));
		expect(badge?.state).toBe('overridden');
		expect(badge?.label).toBe('written');
		expect(badge?.title).toContain('computed by a formula');
		expect(badge?.restoreLabel).toBe('Use the computed value');
	});

	it('says which mechanism is responsible when it produced nothing', () => {
		const badge = originBadge(derivation('pulled', 'empty'));
		expect(badge?.label).toBe('pulled · no value');
		expect(badge?.restoreLabel).toBeNull();
	});
});
