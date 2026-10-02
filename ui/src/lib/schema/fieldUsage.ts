// What the usage section says about a field and whether Remove is
// allowed, derived from the raw answer of
// `GET /api/schema/fields/{name}/usage`. The server names the schema's
// own references (rules, recipes) and reports everything outside the
// schema file as the diagnostics removing the field would introduce,
// each with its scope; this sorts all of it into the groups the panel
// shows — rules, views, config roles, recipes, anything else — and
// counts the items holding a value. Pure so each shape can be
// unit-tested.
//
// Every group but the items blocks removal: its break outlives the
// warning. Items holding a value are counted, not blocking: they keep
// the value and get a warning, nothing is lost
// (`schema-editor-web-design`, decision 6).

import type { FieldUsage } from '$lib/api/generated/FieldUsage';

export interface UsageSummary {
	/** Rule names, in file order. */
	rules: string[];
	/** View ids, each once. */
	views: string[];
	/** `config.yaml` role slots, each once. */
	roles: string[];
	/** Fields whose recipe climbs, reads or computes from the field. */
	recipes: string[];
	/**
	 * Findings with no name to show: the parser's message when nothing
	 * above explains why the schema would not load, and any diagnostic
	 * outside the shapes above. Each is its own message.
	 */
	other: string[];
	/** Distinct items holding a value for the field. */
	itemCount: number;
}

export function summarizeUsage(usage: FieldUsage): UsageSummary {
	const views = new Set<string>();
	const roles = new Set<string>();
	const recipes = new Set<string>(usage.recipes);
	const other = new Set<string>();
	const items = new Set<string>();
	for (const diagnostic of usage.introduced) {
		if (diagnostic.scope === 'item') {
			items.add(diagnostic.item_id);
		} else if (diagnostic.scope === 'config' && 'view_id' in diagnostic) {
			views.add(diagnostic.view_id);
		} else if (diagnostic.scope === 'config' && 'slot' in diagnostic) {
			roles.add(diagnostic.slot);
		} else if (diagnostic.scope === 'config' && 'field' in diagnostic) {
			recipes.add(diagnostic.field);
		} else {
			other.add(diagnostic.message);
		}
	}
	// The parser's message explains itself only when no rule and no
	// recipe already does.
	if (usage.parse_error !== null && usage.rules.length === 0 && recipes.size === 0) {
		other.add(usage.parse_error);
	}
	return {
		rules: [...usage.rules],
		views: [...views],
		roles: [...roles],
		recipes: [...recipes],
		other: [...other],
		itemCount: items.size
	};
}

/**
 * What stands in the way of removing the field, as a phrase for the
 * greyed-out button: `2 rules and 1 view`, `1 config role`. `null`
 * when nothing does.
 */
export function blockerPhrase(summary: UsageSummary): string | null {
	const parts = [
		counted(summary.rules.length, 'rule', 'rules'),
		counted(summary.views.length, 'view', 'views'),
		counted(summary.roles.length, 'config role', 'config roles'),
		counted(summary.recipes.length, 'recipe', 'recipes'),
		counted(summary.other.length, 'other reference', 'other references')
	].filter((part): part is string => part !== null);
	if (parts.length === 0) return null;
	if (parts.length === 1) return parts[0] ?? null;
	return `${parts.slice(0, -1).join(', ')} and ${parts[parts.length - 1] ?? ''}`;
}

function counted(count: number, singular: string, plural: string): string | null {
	if (count === 0) return null;
	return `${String(count)} ${count === 1 ? singular : plural}`;
}

/** What the remove confirmation says and offers. */
export interface RemoveDialogText {
	/** The sentence under the title. */
	body: string;
	/** The answer that removes the field and leaves the items alone. */
	keepLabel: string;
	/**
	 * The answer that also drops the values from the items; `null` when
	 * no item holds one or the count is unknown, and the dialog has one
	 * answer.
	 */
	dropLabel: string | null;
}

/**
 * The remove confirmation for a field `itemCount` items hold a value
 * for; `null` when the usage answer did not load and the count is
 * unknown.
 */
export function removeDialogText(itemCount: number | null): RemoveDialogText {
	if (itemCount === null) {
		return {
			body: 'Items holding a value keep it and get a warning.',
			keepLabel: 'Remove',
			dropLabel: null
		};
	}
	if (itemCount === 0) {
		return { body: 'No item holds a value for it.', keepLabel: 'Remove', dropLabel: null };
	}
	const items = counted(itemCount, 'item', 'items') ?? '';
	return {
		body: `${items} ${itemCount === 1 ? 'holds' : 'hold'} a value: keep it with a warning on each, or drop it from the files.`,
		keepLabel: 'Remove field',
		dropLabel: `Remove field and its ${itemCount === 1 ? 'value' : 'values'} (${items})`
	};
}
