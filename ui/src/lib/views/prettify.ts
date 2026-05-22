// Prettify a kebab/snake-case identifier into a human-readable label.
// "status-board" → "Status Board"; "implement-user-login" → "Implement User Login".
// Used as the title fallback when a Card has no explicit title and as
// the navigation label fallback when a ViewSummary has no title.

export function prettifyId(id: string): string {
	return id
		.split(/[-_]/)
		.filter((part) => part.length > 0)
		.map((part) => part.charAt(0).toUpperCase() + part.slice(1))
		.join(' ');
}
