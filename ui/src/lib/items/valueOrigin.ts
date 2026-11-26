// The origin badge in the item detail: whether a field's value was
// written on the item or derived by workdown, and by which mechanism.
// The server decides the state per field (`ItemDetail.derivations`);
// this module only words it. A field with no entry there is a plain
// one and gets no badge.

import type { FieldDerivation } from '$lib/api/generated/FieldDerivation';
import type { FillMechanism } from '$lib/api/generated/FillMechanism';

/** What the detail shows next to one field's name. */
export interface OriginBadge {
	/** The badge's text. */
	label: string;
	/** The explanation behind it, shown on hover. */
	title: string;
	/**
	 * The text of the action that removes the written value so the
	 * mechanism applies again; `null` where there is nothing to remove.
	 */
	restoreLabel: string | null;
}

/** How each mechanism arrives at a value, as the end of a sentence. */
const HOW: Record<FillMechanism, string> = {
	computed: "computed by a formula from this item's other fields",
	conditional: 'chosen by the first condition that holds',
	pulled: 'pulled from the linked items',
	aggregated: 'rolled up from the items below this one'
};

/** The badge for one field, or `null` for a plain field. */
export function originBadge(derivation: FieldDerivation | undefined): OriginBadge | null {
	if (derivation === undefined) return null;
	const { mechanism, state } = derivation;
	const how = HOW[mechanism];
	switch (state) {
		case 'derived':
			return {
				label: mechanism,
				title: `Not written on this item: ${how}.`,
				restoreLabel: null
			};
		case 'overridden':
			return {
				label: 'written',
				title: `Written on this item. Without it the value would be ${how}.`,
				restoreLabel: `Use the ${mechanism} value`
			};
		case 'empty':
			return {
				label: `${mechanism} · no value`,
				title: `Nothing written, and nothing to derive yet. The value is normally ${how}.`,
				restoreLabel: null
			};
	}
}
