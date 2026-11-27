// The wording of a view's color legend. The server says what the tints
// mean as structure (`ColorLegend`, read from the color field's recipe
// in the schema and the items on screen); this module turns that into
// the caption and the entries the legend row shows. Pure, so the
// wording has tests.
//
// Field names are prettified and values are shown as the schema spells
// them, as a board's column headers do.

import type { ColorCount } from '$lib/api/generated/ColorCount';
import type { ColorLegend } from '$lib/api/generated/ColorLegend';
import type { ComparisonOperator } from '$lib/api/generated/ComparisonOperator';
import type { ConditionEntry } from '$lib/api/generated/ConditionEntry';
import type { ConditionShape } from '$lib/api/generated/ConditionShape';
import type { Operand } from '$lib/api/generated/Operand';
import type { ValueEntry } from '$lib/api/generated/ValueEntry';
import { noValueLabel, prettifyId } from './prettify';

/** One swatch of the legend with what it stands for. */
export interface LegendEntry {
	/** `#rrggbb`. */
	color: string;
	label: string;
	/** Items of the view wearing the color for this reason. */
	count: number;
}

/** What the legend row shows: a caption, then its entries (often none). */
export interface LegendModel {
	caption: string;
	entries: LegendEntry[];
}

const OPERATOR_SIGNS: Record<ComparisonOperator, string> = {
	equal: '=',
	not_equal: '≠',
	less_than: '<',
	less_or_equal: '≤',
	greater_than: '>',
	greater_or_equal: '≥'
};

export function legendModel(legend: ColorLegend): LegendModel {
	const meaning = legend.meaning;
	switch (meaning.kind) {
		case 'by_value':
			return {
				caption: `Color by ${prettifyId(meaning.subject)}`,
				entries: [
					...meaning.entries.map((entry) => ({
						color: entry.color,
						label: valuesLabel(entry, meaning.subject),
						count: entry.count
					})),
					...writtenEntries(meaning.written)
				]
			};
		case 'by_condition':
			return {
				caption: 'Color',
				entries: [
					...meaning.entries.map((entry) => ({
						color: entry.color,
						label: conditionsLabel(entry),
						count: entry.count
					})),
					...writtenEntries(meaning.written)
				]
			};
		case 'inherited': {
			const source =
				meaning.source_field === legend.field ? '' : ` (${prettifyId(meaning.source_field)})`;
			return { caption: `Color inherited from ${prettifyId(meaning.over)}${source}`, entries: [] };
		}
	}
}

/** Colors written by hand on items, which no condition explains. */
function writtenEntries(written: ColorCount[]): LegendEntry[] {
	return written.map(({ color, count }) => ({ color, label: 'set by hand', count }));
}

/**
 * An entry under a subject ("Color by Status"): the values the subject
 * field has on the entry's items. The server reads them off the items
 * on screen, so a color shared by a branch and the default names what
 * is there rather than what the recipe allows.
 */
function valuesLabel(entry: ValueEntry, subject: string): string {
	return entry.values.map((value) => value ?? noValueLabel(subject)).join(', ');
}

/** An entry without a subject: each condition spelled out. */
function conditionsLabel(entry: ConditionEntry): string {
	const parts = entry.conditions.map(conditionText);
	if (entry.otherwise) parts.push(parts.length === 0 ? 'everything else' : 'otherwise');
	return parts.join(' or ');
}

function conditionText(condition: ConditionShape): string {
	if (condition.kind === 'text') return condition.source;
	const sign = OPERATOR_SIGNS[condition.operator];
	return `${operandText(condition.left)} ${sign} ${operandText(condition.right)}`;
}

function operandText(operand: Operand): string {
	switch (operand.kind) {
		case 'field':
			return prettifyId(operand.name);
		// Spelled as the schema spells it, so it is not taken for a field.
		case 'constant':
			return `$constants.${operand.name}`;
		case 'today':
			return 'today';
		case 'literal':
			return String(operand.value);
	}
}
