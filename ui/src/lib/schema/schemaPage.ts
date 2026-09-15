// What the schema page derives from the `/api/schema/definition` payload
// before it renders: the row order, each field's fill mechanisms, the
// settings line under its type, the description cut, and the shape of a
// failed load. Pure so every input shape can be unit-tested without a
// DOM — the page component is the only caller and owns none of the
// reasoning.
//
// Nothing here knows a type's rules. Every line is printed from what the
// payload carries (`FieldShape`, `DerivedBlocks`); the type system stays
// in Rust, and a new shape or generator reaches this page by changing
// the payload, not this file.

import type { ApiResult } from '$lib/api/client';
import type { FieldDefinitionData } from '$lib/api/generated/FieldDefinitionData';
import type { FieldShape } from '$lib/api/generated/FieldShape';
import type { SchemaDefinitionData } from '$lib/api/generated/SchemaDefinitionData';
import { formatDurationSeconds, formatNumber } from '$lib/views/format';

/**
 * The fields in table order: `id` first, then declaration order. The
 * payload is already in declaration order; `id` is the one privileged
 * field, so the table pins it to the top wherever the file declares it.
 * A schema without an `id` field is returned unchanged.
 */
export function idFirst<Field extends Pick<FieldDefinitionData, 'name'>>(fields: Field[]): Field[] {
	const index = fields.findIndex((field) => field.name === 'id');
	if (index <= 0) return fields;
	const idField = fields[index];
	if (idField === undefined) return fields;
	return [idField, ...fields.slice(0, index), ...fields.slice(index + 1)];
}

/**
 * Where a field's value comes from when nobody types it: the fill
 * mechanisms the field declares. The "Filled by" column lists them;
 * blank means the value is written by hand.
 */
export type FillMechanism = 'computed' | 'conditional' | 'pulled' | 'aggregated';

/**
 * The fill mechanisms a field declares, in a fixed order. Empty for a
 * plain field. A field can carry two — compute fills the leaves and the
 * aggregate rolls them up — so this is a list, not one value.
 */
export function fillMechanisms(field: Pick<FieldDefinitionData, 'derived'>): FillMechanism[] {
	const mechanisms: FillMechanism[] = [];
	if (field.derived.compute !== null) mechanisms.push('computed');
	if (field.derived.when !== null) mechanisms.push('conditional');
	if (field.derived.pull !== null) mechanisms.push('pulled');
	if (field.derived.aggregate !== null) mechanisms.push('aggregated');
	return mechanisms;
}

/**
 * The settings line under the type: the shape's summary, then the
 * resource list that constrains the values (`from people`) when the
 * field names one. A resource is the same kind of fact as a choice's
 * values or a numeric range — which values are allowed — so it sits
 * with them rather than in the "Filled by" column, which says where a
 * value comes from. `null` when there is nothing to say.
 */
export function settingsLine(
	field: Pick<FieldDefinitionData, 'shape' | 'resource'>
): string | null {
	const parts: string[] = [];
	const summary = typeSummary(field.shape);
	if (summary !== null) parts.push(summary);
	if (field.resource !== null) parts.push(`from ${field.resource}`);
	return parts.length === 0 ? null : parts.join(SEPARATOR);
}

/**
 * One line summarising the type-specific settings, rendered muted under
 * the type — `to_do · in_progress · done` for a choice, `0 to 100` for a
 * numeric, `at most 1d 16h` for a duration, `pattern ^[a-z-]+$` for
 * text, `inverse children · no cycles` for a relation. `null` when the
 * shape carries nothing to say: a plain scalar, or a shape whose every
 * setting is unset. Without this line the page shows less than the
 * YAML file; with it, it answers the question people open the file for.
 * Durations go through the formatter every table and chart uses, so a
 * bound reads the same here as everywhere else in the app.
 */
export function typeSummary(shape: FieldShape): string | null {
	switch (shape.kind) {
		case 'scalar':
			return null;
		case 'numeric':
			return rangeSummary(
				shape.min === null ? null : formatNumber(shape.min),
				shape.max === null ? null : formatNumber(shape.max)
			);
		case 'duration':
			return rangeSummary(
				shape.min_seconds === null ? null : formatDurationSeconds(Number(shape.min_seconds)),
				shape.max_seconds === null ? null : formatDurationSeconds(Number(shape.max_seconds))
			);
		case 'text':
			return shape.pattern === null ? null : `pattern ${shape.pattern}`;
		case 'values':
			return shape.values.length === 0 ? null : shape.values.join(SEPARATOR);
		case 'relation': {
			const parts: string[] = [];
			if (shape.inverse !== null) parts.push(`inverse ${shape.inverse}`);
			if (shape.allow_cycles === true) parts.push('cycles allowed');
			if (shape.allow_cycles === false) parts.push('no cycles');
			return parts.length === 0 ? null : parts.join(SEPARATOR);
		}
	}
}

/** Between the parts of a summary line. */
const SEPARATOR = ' · ';

/** A min/max pair as one phrase; `null` when neither bound is set. */
function rangeSummary(min: string | null, max: string | null): string | null {
	if (min !== null && max !== null) return `${min} to ${max}`;
	if (min !== null) return `at least ${min}`;
	if (max !== null) return `at most ${max}`;
	return null;
}

/**
 * The first non-blank line of a description, trimmed, or `null` when
 * there is none. Every real description is one line today; this is the
 * guard for a multi-line one. The column cuts the line with an ellipsis
 * at its width and shows the full text on hover.
 */
export function firstLine(description: string | null): string | null {
	if (description === null) return null;
	const line = description
		.split('\n')
		.map((candidate) => candidate.trim())
		.find((candidate) => candidate.length > 0);
	return line ?? null;
}

/**
 * Why the definition did not arrive, for the page's broken state: the
 * schema file's path and the parser's message when the server answered
 * with its load diagnostic (`422`), otherwise the request-level error
 * line — the server unreachable, or a reply that was not the envelope.
 */
export interface LoadFailure {
	/** The `schema.yaml` path the server tried, when the failure names one. */
	path: string | null;
	/** One human-readable line saying what went wrong. */
	detail: string;
}

export function loadFailure(result: ApiResult<SchemaDefinitionData>): LoadFailure {
	const fileDiagnostic = result.diagnostics.find((diagnostic) => diagnostic.scope === 'file');
	if (fileDiagnostic !== undefined) {
		return { path: fileDiagnostic.source_path, detail: fileDiagnostic.message };
	}
	return { path: null, detail: result.error ?? 'The schema could not be loaded.' };
}
