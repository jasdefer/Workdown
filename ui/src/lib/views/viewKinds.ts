// The fixed vocabulary of view kinds and the inputs each one needs — the
// spec that drives the create form.
//
// What lives here is presentation: which controls a kind shows, in what
// order, under what label, and which of them are optional. Those are form
// decisions with no counterpart in the CLI.
//
// What does *not* live here is which names each slot takes — the field
// types it accepts, and whether a relation's inverse name may stand in.
// That is `crates/core/src/model/view_slots.rs`, reaching the UI as the
// generated `VIEW_SLOTS` table below. It used to be hand-copied here, and
// two slots had quietly drifted narrower than the rule the server
// enforces. Hand a control its `slot` from the table; never retype it.
//
// The kinds themselves are not served either: they are baked into the Rust
// `ViewType` enum and identical for every project (unlike fields and
// operators, which are schema-driven and do come from the server).

import type { FieldSchema } from '$lib/api/generated/FieldSchema';
import type { FieldType } from '$lib/api/generated/FieldType';
import type { ViewType } from '$lib/api/generated/ViewType';
import { VIEW_SLOTS, type ViewSlot } from '$lib/api/generated/viewSlots';

/** A control in a kind's create form. */
export type Control =
	// A single schema-field reference; `slot` says which names it takes.
	// `optional` slots may be left unset.
	| {
			control: 'field';
			key: string;
			label: string;
			slot: ViewSlot;
			optional?: boolean;
	  }
	// An ordered list of field names (table/tree columns), constrained by
	// type (empty `accepts` = any field).
	| {
			control: 'fieldList';
			key: string;
			label: string;
			accepts: readonly FieldType[];
			optional?: boolean;
	  }
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

// Slots the create form owns rather than mirrors: `columns` is serialized
// into the `fields` display role on save, which takes any field.
const ANY_FIELD: readonly FieldType[] = [];

/** Controls for each view kind, in form order. */
export const VIEW_KIND_CONTROLS: Record<ViewType, Control[]> = {
	board: [
		{
			control: 'field',
			key: 'field',
			label: 'Group by',
			slot: VIEW_SLOTS.board.field
		}
	],
	tree: [
		{ control: 'field', key: 'field', label: 'Parent link', slot: VIEW_SLOTS.tree.field },
		{ control: 'fieldList', key: 'columns', label: 'Columns', accepts: ANY_FIELD, optional: true }
	],
	graph: [
		{ control: 'field', key: 'field', label: 'Relation', slot: VIEW_SLOTS.graph.field },
		{
			control: 'field',
			key: 'group_by',
			label: 'Group by',
			slot: VIEW_SLOTS.graph.group_by,
			optional: true
		}
	],
	// `columns` is a form-local slot: the create form serializes it into
	// the `fields` display role (`display.fields`) on save. Optional —
	// an unset role falls back to every schema field.
	table: [
		{ control: 'fieldList', key: 'columns', label: 'Columns', accepts: ANY_FIELD, optional: true }
	],
	gantt: [
		{ control: 'ganttInput' },
		{
			control: 'field',
			key: 'group',
			label: 'Group by',
			slot: VIEW_SLOTS.gantt.group,
			optional: true
		}
	],
	gantt_by_initiative: [
		{ control: 'ganttInput' },
		{
			control: 'field',
			key: 'root_link',
			label: 'Initiative link',
			slot: VIEW_SLOTS.gantt_by_initiative.root_link
		}
	],
	gantt_by_depth: [
		{ control: 'ganttInput' },
		{
			control: 'field',
			key: 'depth_link',
			label: 'Depth link',
			slot: VIEW_SLOTS.gantt_by_depth.depth_link
		}
	],
	bar_chart: [
		{
			control: 'field',
			key: 'group_by',
			label: 'Group by',
			slot: VIEW_SLOTS.bar_chart.group_by
		},
		{ control: 'aggregate', key: 'aggregate', label: 'Aggregate' },
		{
			control: 'field',
			key: 'value',
			label: 'Value',
			slot: VIEW_SLOTS.bar_chart.value,
			optional: true
		}
	],
	line_chart: [
		{
			control: 'field',
			key: 'x',
			label: 'X axis',
			slot: VIEW_SLOTS.line_chart.x
		},
		{ control: 'field', key: 'y', label: 'Y axis', slot: VIEW_SLOTS.line_chart.y },
		{
			control: 'field',
			key: 'group',
			label: 'Series',
			slot: VIEW_SLOTS.line_chart.group,
			optional: true
		}
	],
	workload: [
		{ control: 'field', key: 'start', label: 'Start', slot: VIEW_SLOTS.workload.start },
		{ control: 'field', key: 'end', label: 'End', slot: VIEW_SLOTS.workload.end },
		{ control: 'field', key: 'effort', label: 'Effort', slot: VIEW_SLOTS.workload.effort },
		{ control: 'workingDays', key: 'working_days', label: 'Working days' }
	],
	metric: [{ control: 'metrics' }],
	treemap: [
		{ control: 'field', key: 'group', label: 'Group by', slot: VIEW_SLOTS.treemap.group },
		{ control: 'field', key: 'size', label: 'Size', slot: VIEW_SLOTS.treemap.size }
	],
	heatmap: [
		{ control: 'field', key: 'x', label: 'X axis', slot: VIEW_SLOTS.heatmap.x },
		{ control: 'field', key: 'y', label: 'Y axis', slot: VIEW_SLOTS.heatmap.y },
		{ control: 'aggregate', key: 'aggregate', label: 'Aggregate' },
		{
			control: 'field',
			key: 'value',
			label: 'Value',
			slot: VIEW_SLOTS.heatmap.value,
			optional: true
		},
		{ control: 'bucket', key: 'bucket', label: 'Bucket', optional: true }
	]
};

/**
 * Every kind's menu label, in the order the picker offers them.
 *
 * `Record<ViewType, string>` makes a missing kind a TypeScript error, and
 * the key order *is* the picker order — so the list of kinds and their
 * labels are one list rather than two that have to agree.
 */
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

/**
 * All selectable view kinds, in menu order — the keys of `KIND_LABELS`,
 * whose insertion order ES guarantees for non-numeric keys. Derived rather
 * than written out, because a hand-kept second list is what let a kind go
 * missing from the picker unnoticed.
 */
export const VIEW_KINDS = Object.keys(KIND_LABELS) as ViewType[];

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
export function fieldFits(fieldType: FieldType, accepts: readonly FieldType[]): boolean {
	return accepts.length === 0 || accepts.includes(fieldType);
}

/** One entry of a field slot's picker: the name saved, and how it reads. */
export interface SlotOption {
	value: string;
	label: string;
}

/**
 * The names a slot's picker offers: every field of an accepted type, in
 * schema order. In a slot that takes inverse names, a relation that
 * declares an inverse is followed by that inverse name — the same links,
 * read from the other end — labelled so the two can be told apart.
 */
export function slotOptions(
	fields: readonly Pick<FieldSchema, 'name' | 'field_type' | 'inverse'>[],
	slot: ViewSlot
): SlotOption[] {
	const options: SlotOption[] = [];
	for (const field of fields) {
		if (!fieldFits(field.field_type, slot.accepts)) continue;
		options.push({ value: field.name, label: field.name });
		if (slot.inverseNames && field.inverse !== null) {
			options.push({ value: field.inverse, label: `${field.inverse} (${field.name}, reversed)` });
		}
	}
	return options;
}

/** Narrow an unknown slot value to a plain object. */
export function isRecord(value: unknown): value is Record<string, unknown> {
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
