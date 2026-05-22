// Format a Diagnostic as a short human-readable line for the
// DiagnosticBanner.
//
// The full Display logic lives on the Rust side; replicating it in TS
// would invite drift across the same vocabulary. Instead this helper
// surfaces the most useful fields generically: the kind label (with
// underscores spaced) plus a few common context fields when present.
// Good enough for v1 — the banner is for at-a-glance triage, not
// detailed exposition. If a specific variant needs richer formatting
// later, add a special case.

import type { Diagnostic } from '$lib/api/generated/Diagnostic';

export function formatDiagnostic(diagnostic: Diagnostic): string {
	const raw = diagnostic as Record<string, unknown>;
	const kindLabel = humanize((raw.type as string | undefined) ?? diagnostic.scope);

	const parts: string[] = [kindLabel];

	if (typeof raw.field === 'string') {
		parts.push(`field '${raw.field}'`);
	}
	if (typeof raw.target_id === 'string') {
		parts.push(`→ '${raw.target_id}'`);
	}
	if (typeof raw.field_name === 'string') {
		parts.push(`field '${raw.field_name}'`);
	}
	if (typeof raw.slot === 'string') {
		parts.push(`slot '${raw.slot}'`);
	}
	if (typeof raw.rule === 'string') {
		parts.push(`rule '${raw.rule}'`);
	}
	if (typeof raw.detail === 'string') {
		parts.push(raw.detail);
	}
	if (Array.isArray(raw.chain)) {
		parts.push((raw.chain as string[]).join(' → '));
	}

	return parts.join(': ');
}

function humanize(value: string): string {
	return value.replace(/_/g, ' ');
}
