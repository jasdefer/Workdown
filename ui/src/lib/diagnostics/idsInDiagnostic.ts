// Set of work-item ids referenced by a diagnostic. Used by the
// DiagnosticBanner together with `idsInView` to classify diagnostics
// as primary (touches an item in the current view) or secondary
// (everywhere else).
//
// `file` and `config` diagnostics don't reference items at all — they
// return empty sets. `item` carries one id; `files::duplicate_id`
// carries one id; `collection::cycle` carries the full chain;
// `collection::count_violation` is rule-level, no ids.

import type { Diagnostic } from '$lib/api/generated/Diagnostic';
import type { WorkItemId } from '$lib/api/generated/WorkItemId';

export function idsInDiagnostic(diagnostic: Diagnostic): Set<WorkItemId> {
	const ids = new Set<WorkItemId>();
	switch (diagnostic.scope) {
		case 'item':
			ids.add(diagnostic.item_id);
			break;

		case 'files':
			// FilesDiagnostic only has the `duplicate_id` variant today; the
			// `id` field is on the struct directly.
			ids.add(diagnostic.id);
			break;

		case 'collection':
			if (diagnostic.type === 'cycle') {
				for (const id of diagnostic.chain) {
					ids.add(id);
				}
			}
			break;

		// File-scope and config-scope diagnostics don't reference items.
		case 'file':
		case 'config':
			break;
	}
	return ids;
}
