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

import type { DisplayConfig } from '$lib/api/generated/DisplayConfig';

/**
 * A partial DisplayConfig — the same wire shape the server's
 * `?display=` parameter deserializes (generated from the Rust type, so
 * the two cannot drift). `color` carries a color-typed field name or
 * the sentinel `'none'` (no tint).
 */
export type DisplayOverride = DisplayConfig;

function storageKey(viewId: string): string {
	return `workdown.display.${viewId}`;
}

/**
 * Copy carrying only the set roles — the single definition of "set"
 * shared by emptiness checks and the wire format. An empty `fields`
 * array counts as unset here: the Display bar's multi-select cannot
 * express "show no fields", so an empty selection means "configured"
 * (the wire format itself can express `[]`; the bar offers no
 * affordance for it yet).
 */
function setRoles(override: DisplayOverride): DisplayOverride {
	const cleaned: DisplayOverride = {};
	if (override.title !== undefined) cleaned.title = override.title;
	if (override.subtitle !== undefined) cleaned.subtitle = override.subtitle;
	if (override.fields !== undefined && override.fields.length > 0) cleaned.fields = override.fields;
	if (override.color !== undefined) cleaned.color = override.color;
	return cleaned;
}

/** Whether the override sets any role at all. */
export function isEmptyOverride(override: DisplayOverride): boolean {
	return Object.keys(setRoles(override)).length === 0;
}

function isStringArray(value: unknown): value is string[] {
	return Array.isArray(value) && value.every((entry) => typeof entry === 'string');
}

/**
 * Narrow parsed storage JSON to a DisplayOverride, or null when any
 * known role carries the wrong type. A corrupt or drifted entry must
 * degrade to "no override" here: forwarded as `?display=` it would 422
 * on every load and strand the page on the error boundary, where the
 * bar's "Clear" affordance doesn't exist. Unknown keys are dropped, not
 * rejected, so an entry written by a newer version degrades gracefully.
 */
function asDisplayOverride(parsed: unknown): DisplayOverride | null {
	if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) return null;
	const candidate = parsed as Record<string, unknown>;
	const override: DisplayOverride = {};
	if (candidate.title !== undefined) {
		if (typeof candidate.title !== 'string') return null;
		override.title = candidate.title;
	}
	if (candidate.subtitle !== undefined) {
		if (typeof candidate.subtitle !== 'string') return null;
		override.subtitle = candidate.subtitle;
	}
	if (candidate.fields !== undefined) {
		if (!isStringArray(candidate.fields)) return null;
		override.fields = candidate.fields;
	}
	if (candidate.color !== undefined) {
		if (typeof candidate.color !== 'string') return null;
		override.color = candidate.color;
	}
	return override;
}

export function loadDisplayOverride(viewId: string): DisplayOverride | null {
	if (typeof localStorage === 'undefined') return null;
	const raw = localStorage.getItem(storageKey(viewId));
	if (raw === null) return null;
	try {
		const override = asDisplayOverride(JSON.parse(raw));
		if (override === null || isEmptyOverride(override)) return null;
		return override;
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
	return JSON.stringify(setRoles(override));
}
