// The fold in the item detail: fields the schema marks `collapsed: true`
// sit behind one "Show N more fields" row, closed every time an item is
// opened. Nothing is remembered. The only decisions worth a test are
// pulled out here: when the fold opens on its own, and what its row says.

import type { Diagnostic } from '$lib/api/generated/Diagnostic';

/**
 * Whether a save's warnings name a collapsed field. The fold opens then,
 * so a warning never points at a field that is out of sight. Only
 * item-scoped findings carry a field name; a rule violation or a derive
 * cycle names none and leaves the fold alone.
 */
export function foldOpensFor(diagnostics: Diagnostic[], collapsed: ReadonlySet<string>): boolean {
	return diagnostics.some(
		(diagnostic) =>
			diagnostic.scope === 'item' && 'field' in diagnostic && collapsed.has(diagnostic.field)
	);
}

/** The fold row's text for `count` collapsed fields, closed or open. */
export function foldLabel(count: number, open: boolean): string {
	const noun = count === 1 ? 'field' : 'fields';
	const number = String(count);
	return open ? `Hide ${number} ${noun}` : `Show ${number} more ${noun}`;
}
