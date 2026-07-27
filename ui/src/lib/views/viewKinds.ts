// The fixed vocabulary of view kinds and the inputs each one needs — the
// static spec that drives the create form.
//
// This is deliberately a hand-maintained table, not server data: the 13
// kinds and their slots are baked into the Rust `ViewType` enum + `RawView`
// and are identical for every project (unlike fields/operators, which are
// project-schema-driven and must be served). The accepted-type lists mirror
// `crates/core/src/views_check.rs`; the server re-validates on save, so a
// drift here is a UX gap, never a corrupt write. Keep this in sync with
// `views_check` / `convert_view` when the view model changes.

import type { FieldType } from '$lib/api/generated/FieldType';
import type { ViewType } from '$lib/api/generated/ViewType';

/** A control in a kind's create form. */
export type Control =
	// A single schema-field reference, constrained by type (empty `accepts`
	// = any field). `optional` slots may be left unset.
	| { control: 'field'; key: string; label: string; accepts: FieldType[]; optional?: boolean }
	// An ordered list of field names (table/tree columns).
	| { control: 'fieldList'; key: string; label: string; accepts: FieldType[]; optional?: boolean }
	// The chart aggregate function.
	| { control: 'aggregate'; key: string; label: string }
	// Optional date-bucketing for heatmap axes.
	| { control: 'bucket'; key: string; label: string; optional?: boolean }
	// Gantt's start field plus its mutually-exclusive end/duration/after mode.
	| { control: 'ganttInput' }
	// Metric's repeatable rows (label? + aggregate + value?).
	| { control: 'metrics' }
	// Workload's optional working-days override.
	| { control: 'workingDays'; key: string; label: string };

const DATE: FieldType[] = ['date'];
const SCALAR: FieldType[] = ['integer', 'float', 'duration'];
const GROUPABLE: FieldType[] = ['choice', 'multichoice', 'string', 'list', 'link', 'links'];

/** Controls for each view kind, in form order. */
export const VIEW_KIND_CONTROLS: Record<ViewType, Control[]> = {
	board: [
		{
			control: 'field',
			key: 'field',
			label: 'Group by',
			accepts: ['choice', 'multichoice', 'string']
		}
	],
	tree: [
		{ control: 'field', key: 'field', label: 'Parent link', accepts: ['link'] },
		{ control: 'fieldList', key: 'columns', label: 'Columns', accepts: [], optional: true }
	],
	graph: [
		{ control: 'field', key: 'field', label: 'Relation', accepts: ['link', 'links'] },
		{ control: 'field', key: 'group_by', label: 'Group by', accepts: ['link'], optional: true }
	],
	// `columns` is a form-local slot: the create form serializes it into
	// the `fields` display role (`display.fields`) on save. Optional —
	// an unset role falls back to every schema field.
	table: [{ control: 'fieldList', key: 'columns', label: 'Columns', accepts: [], optional: true }],
	gantt: [
		{ control: 'ganttInput' },
		{ control: 'field', key: 'group', label: 'Group by', accepts: GROUPABLE, optional: true }
	],
	gantt_by_initiative: [
		{ control: 'ganttInput' },
		{ control: 'field', key: 'root_link', label: 'Initiative link', accepts: ['link'] }
	],
	gantt_by_depth: [
		{ control: 'ganttInput' },
		{ control: 'field', key: 'depth_link', label: 'Depth link', accepts: ['link'] }
	],
	bar_chart: [
		{ control: 'field', key: 'group_by', label: 'Group by', accepts: [] },
		{ control: 'aggregate', key: 'aggregate', label: 'Aggregate' },
		{ control: 'field', key: 'value', label: 'Value', accepts: SCALAR, optional: true }
	],
	line_chart: [
		{
			control: 'field',
			key: 'x',
			label: 'X axis',
			accepts: ['integer', 'float', 'date', 'duration']
		},
		{ control: 'field', key: 'y', label: 'Y axis', accepts: SCALAR },
		{ control: 'field', key: 'group', label: 'Series', accepts: GROUPABLE, optional: true }
	],
	workload: [
		{ control: 'field', key: 'start', label: 'Start', accepts: DATE },
		{ control: 'field', key: 'end', label: 'End', accepts: DATE },
		{ control: 'field', key: 'effort', label: 'Effort', accepts: SCALAR },
		{ control: 'workingDays', key: 'working_days', label: 'Working days' }
	],
	metric: [{ control: 'metrics' }],
	treemap: [
		{ control: 'field', key: 'group', label: 'Group by', accepts: ['link'] },
		{ control: 'field', key: 'size', label: 'Size', accepts: SCALAR }
	],
	heatmap: [
		{ control: 'field', key: 'x', label: 'X axis', accepts: [] },
		{ control: 'field', key: 'y', label: 'Y axis', accepts: [] },
		{ control: 'aggregate', key: 'aggregate', label: 'Aggregate' },
		{ control: 'field', key: 'value', label: 'Value', accepts: SCALAR, optional: true },
		{ control: 'bucket', key: 'bucket', label: 'Bucket', optional: true }
	]
};

/** All selectable view kinds, in a sensible menu order. */
export const VIEW_KINDS: ViewType[] = [
	'board',
	'table',
	'tree',
	'graph',
	'gantt',
	'gantt_by_initiative',
	'gantt_by_depth',
	'bar_chart',
	'line_chart',
	'heatmap',
	'treemap',
	'workload',
	'metric'
];

const KIND_LABELS: Record<ViewType, string> = {
	board: 'Board',
	table: 'Table',
	tree: 'Tree',
	graph: 'Graph',
	gantt: 'Gantt',
	gantt_by_initiative: 'Gantt by initiative',
	gantt_by_depth: 'Gantt by depth',
	bar_chart: 'Bar chart',
	line_chart: 'Line chart',
	heatmap: 'Heatmap',
	treemap: 'Treemap',
	workload: 'Workload',
	metric: 'Metric'
};

export function kindLabel(kind: ViewType): string {
	return KIND_LABELS[kind];
}

export const AGGREGATES = ['count', 'sum', 'avg', 'min', 'max'] as const;
export const BUCKETS = ['day', 'week', 'month'] as const;
export const WEEKDAYS = [
	'monday',
	'tuesday',
	'wednesday',
	'thursday',
	'friday',
	'saturday',
	'sunday'
] as const;

/** Whether a field of `fieldType` is acceptable for a slot's `accepts` list. */
export function fieldFits(fieldType: FieldType, accepts: FieldType[]): boolean {
	return accepts.length === 0 || accepts.includes(fieldType);
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null;
}

function isFilledString(value: unknown): boolean {
	return typeof value === 'string' && value !== '';
}

/**
 * Whether a definition has every *required* slot for its kind filled — the
 * client-side gate for the Save button. The server re-validates and can
 * still warn (e.g. a type mismatch), but this catches the missing-slot case
 * before the write.
 */
export function isDefinitionComplete(kind: ViewType, definition: Record<string, unknown>): boolean {
	return VIEW_KIND_CONTROLS[kind].every((control) => {
		switch (control.control) {
			case 'field':
				return control.optional === true || isFilledString(definition[control.key]);
			case 'fieldList':
				return (
					control.optional === true ||
					(Array.isArray(definition[control.key]) &&
						(definition[control.key] as unknown[]).length > 0)
				);
			case 'aggregate':
				return isFilledString(definition[control.key]);
			case 'bucket':
			case 'workingDays':
				return true; // optional
			case 'ganttInput':
				// start plus at least one of end / duration (after-mode implies
				// duration); the server checks the finer input-mode rules.
				return (
					isFilledString(definition.start) &&
					(isFilledString(definition.end) || isFilledString(definition.duration))
				);
			case 'metrics': {
				const rows = definition.metrics;
				return (
					Array.isArray(rows) &&
					rows.length > 0 &&
					rows.every((row) => isRecord(row) && isFilledString(row.aggregate))
				);
			}
		}
	});
}
