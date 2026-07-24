// Per-session display-role overrides, persisted per view in
// localStorage. An override is a partial DisplayConfig: roles it sets
// take highest precedence on the server (over the view's `display:`
// block and the config defaults); roles it leaves unset inherit as
// usual. Nothing is ever written to views.yaml.
//
// The page `load()` re-reads the override on every invalidation (SSE
// pings included), so an active override survives live updates for
// free — a views.yaml change still re-renders, just with the override
// still applied on top.

export interface DisplayOverride {
	title?: string;
	subtitle?: string;
	/**
	 * Overriding field list. This bar treats an empty selection as unset
	 * (the server-side wire format can also express an explicit "show no
	 * fields" via `[]`, but the bar offers no affordance for it yet).
	 */
	fields?: string[];
	/** A color-typed field name, or the sentinel 'none' for no tint. */
	color?: string;
}

function storageKey(viewId: string): string {
	return `workdown.display.${viewId}`;
}

/** Whether the override sets any role at all. */
export function isEmptyOverride(override: DisplayOverride): boolean {
	return (
		override.title === undefined &&
		override.subtitle === undefined &&
		(override.fields === undefined || override.fields.length === 0) &&
		override.color === undefined
	);
}

export function loadDisplayOverride(viewId: string): DisplayOverride | null {
	if (typeof localStorage === 'undefined') return null;
	const raw = localStorage.getItem(storageKey(viewId));
	if (raw === null) return null;
	try {
		const parsed: unknown = JSON.parse(raw);
		if (typeof parsed !== 'object' || parsed === null) return null;
		const override = parsed as DisplayOverride;
		return isEmptyOverride(override) ? null : override;
	} catch {
		return null;
	}
}

/** Persist an override, or remove it when `null` / empty. */
export function saveDisplayOverride(viewId: string, override: DisplayOverride | null): void {
	if (typeof localStorage === 'undefined') return;
	if (override === null || isEmptyOverride(override)) {
		localStorage.removeItem(storageKey(viewId));
	} else {
		localStorage.setItem(storageKey(viewId), JSON.stringify(override));
	}
}

/** The `?display=` parameter value for an override (server-side JSON shape). */
export function displayOverrideParam(override: DisplayOverride): string {
	// Drop unset keys so the server-side partial only carries set roles.
	const wire: DisplayOverride = {};
	if (override.title !== undefined) wire.title = override.title;
	if (override.subtitle !== undefined) wire.subtitle = override.subtitle;
	if (override.fields !== undefined && override.fields.length > 0) wire.fields = override.fields;
	if (override.color !== undefined) wire.color = override.color;
	return JSON.stringify(wire);
}
