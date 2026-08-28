//! Which field types each view kind's structural slots accept.
//!
//! One statement of the rule, three consumers:
//!
//! - [`crate::views_check`] validates `views.yaml` against it, and words its
//!   mismatch diagnostics with [`describe`] so the prose and the list cannot
//!   disagree;
//! - [`crate::config_check`] validates the field roles in `config.yaml`
//!   against the slots they stand in for (`defaults.board_field` accepts
//!   what a board's `field` accepts, and must not be stricter);
//! - `cargo xtask gen-types` emits [`slots_of`] as a TypeScript table the
//!   view create form reads, so the web UI offers exactly the fields the
//!   CLI would accept.
//!
//! Before this table each of the three carried its own copy — the web UI's
//! was quietly narrower than the CLI's in two places. A slot's accepted
//! types are written here and nowhere else.
//!
//! Only *structural* slots live here: the ones naming a schema field.
//! Cross-slot rules (a gantt's mutually exclusive input modes, a `bucket`
//! needing a date axis, `count` forbidding a `value`) stay in `views_check`,
//! which is where a rule that spans two slots belongs. So does the
//! acyclicity requirement on link slots — a property of the field's config
//! rather than of its type.

use crate::model::schema::FieldType;
use crate::model::views::{Aggregate, ViewType};

/// One structural slot of a view kind.
#[derive(Debug, Clone, Copy)]
pub struct SlotSpec {
    /// The key this slot is written under in `views.yaml` — and the key it
    /// appears under in the generated TypeScript table.
    pub name: &'static str,
    /// The field types the slot accepts. Empty means any field: a slot
    /// that only needs *a* field, like a chart's grouping axis.
    pub accepts: &'static [FieldType],
}

impl SlotSpec {
    const fn new(name: &'static str, accepts: &'static [FieldType]) -> Self {
        Self { name, accepts }
    }
}

// ── Shared type sets ─────────────────────────────────────────────────

/// Field types that partition items into named buckets.
const GROUPABLE: &[FieldType] = &[
    FieldType::Choice,
    FieldType::Multichoice,
    FieldType::String,
    FieldType::List,
    FieldType::Link,
    FieldType::Links,
];

/// The types a board groups by: `GROUPABLE` minus the relation and list
/// types, whose multi-valued nature would put one item in several columns.
const BOARD_GROUPABLE: &[FieldType] =
    &[FieldType::Choice, FieldType::Multichoice, FieldType::String];

/// Field types that carry a magnitude — bar heights, treemap areas,
/// workload effort.
const MEASURABLE: &[FieldType] = &[FieldType::Integer, FieldType::Float, FieldType::Duration];

/// `MEASURABLE` plus dates: what a continuous axis can plot.
const PLOTTABLE: &[FieldType] = &[
    FieldType::Integer,
    FieldType::Float,
    FieldType::Date,
    FieldType::Duration,
];

const DATE: &[FieldType] = &[FieldType::Date];
const DURATION: &[FieldType] = &[FieldType::Duration];
const SINGLE_LINK: &[FieldType] = &[FieldType::Link];
const ANY_LINK: &[FieldType] = &[FieldType::Link, FieldType::Links];

/// Any field at all — see [`SlotSpec::accepts`].
const ANY: &[FieldType] = &[];

// ── The slots ────────────────────────────────────────────────────────

/// The column a board groups items into.
pub const BOARD_FIELD: SlotSpec = SlotSpec::new("field", BOARD_GROUPABLE);

/// The parent relation a tree nests by.
pub const TREE_FIELD: SlotSpec = SlotSpec::new("field", SINGLE_LINK);

/// The relation a graph draws edges for. Inverse relation names are also
/// accepted here and resolve to their declaring field at extraction; that
/// is a name-resolution rule, not a type, so `views_check` owns it.
pub const GRAPH_FIELD: SlotSpec = SlotSpec::new("field", ANY_LINK);

/// The relation whose chain becomes graph subgraph nesting. Single-target,
/// so each item lands in exactly one box.
pub const GRAPH_GROUP_BY: SlotSpec = SlotSpec::new("group_by", SINGLE_LINK);

/// A gantt bar's start date. Shared by all three gantt kinds.
pub const GANTT_START: SlotSpec = SlotSpec::new("start", DATE);

/// A gantt bar's end date, in end-mode.
pub const GANTT_END: SlotSpec = SlotSpec::new("end", DATE);

/// A gantt bar's length, in duration-mode.
pub const GANTT_DURATION: SlotSpec = SlotSpec::new("duration", DURATION);

/// The predecessors a gantt bar starts after, in after-mode.
pub const GANTT_AFTER: SlotSpec = SlotSpec::new("after", ANY_LINK);

/// The field a plain gantt groups its bars by.
pub const GANTT_GROUP: SlotSpec = SlotSpec::new("group", GROUPABLE);

/// The relation walked upward to find an item's initiative.
pub const GANTT_ROOT_LINK: SlotSpec = SlotSpec::new("root_link", SINGLE_LINK);

/// The relation walked upward to find an item's depth.
pub const GANTT_DEPTH_LINK: SlotSpec = SlotSpec::new("depth_link", SINGLE_LINK);

/// A bar chart's category axis. Any field: one bar per distinct value.
pub const BAR_CHART_GROUP_BY: SlotSpec = SlotSpec::new("group_by", ANY);

/// A line chart's continuous x axis.
pub const LINE_CHART_X: SlotSpec = SlotSpec::new("x", PLOTTABLE);

/// A line chart's y axis.
pub const LINE_CHART_Y: SlotSpec = SlotSpec::new("y", MEASURABLE);

/// The field a line chart splits into series.
pub const LINE_CHART_GROUP: SlotSpec = SlotSpec::new("group", GROUPABLE);

/// The start of an item's workload window.
pub const WORKLOAD_START: SlotSpec = SlotSpec::new("start", DATE);

/// The end of an item's workload window.
pub const WORKLOAD_END: SlotSpec = SlotSpec::new("end", DATE);

/// The effort spread across an item's workload window.
pub const WORKLOAD_EFFORT: SlotSpec = SlotSpec::new("effort", MEASURABLE);

/// The relation a treemap nests by.
pub const TREEMAP_GROUP: SlotSpec = SlotSpec::new("group", SINGLE_LINK);

/// The field a treemap sizes its boxes by.
pub const TREEMAP_SIZE: SlotSpec = SlotSpec::new("size", MEASURABLE);

/// A heatmap's x axis. Any field, like a bar chart's categories.
pub const HEATMAP_X: SlotSpec = SlotSpec::new("x", ANY);

/// A heatmap's y axis.
pub const HEATMAP_Y: SlotSpec = SlotSpec::new("y", ANY);

/// The field an aggregate reduces — a bar chart's, heatmap's or metric
/// row's `value:`.
///
/// The set here is the widest any aggregate function accepts, because the
/// function is chosen in the same breath as the field and a form has to
/// offer something before it knows which. [`aggregate_value_types`]
/// narrows it per function, and `views_check` applies that narrower rule;
/// `count` rejects a `value:` outright.
pub const AGGREGATE_VALUE: SlotSpec = SlotSpec::new("value", PLOTTABLE);

/// Every structural slot of `kind`, in `views.yaml` reading order.
///
/// The exhaustive match is the point: a fourteenth view kind cannot be
/// added without listing its slots, which is what keeps the generated
/// TypeScript table complete.
pub fn slots_of(kind: ViewType) -> &'static [SlotSpec] {
    match kind {
        ViewType::Board => &[BOARD_FIELD],
        ViewType::Tree => &[TREE_FIELD],
        ViewType::Graph => &[GRAPH_FIELD, GRAPH_GROUP_BY],
        // A table's columns come from the `fields` display role, which
        // takes any field and is checked as a display role.
        ViewType::Table => &[],
        ViewType::Gantt => &[
            GANTT_START,
            GANTT_END,
            GANTT_DURATION,
            GANTT_AFTER,
            GANTT_GROUP,
        ],
        ViewType::GanttByInitiative => &[
            GANTT_START,
            GANTT_END,
            GANTT_DURATION,
            GANTT_AFTER,
            GANTT_ROOT_LINK,
        ],
        ViewType::GanttByDepth => &[
            GANTT_START,
            GANTT_END,
            GANTT_DURATION,
            GANTT_AFTER,
            GANTT_DEPTH_LINK,
        ],
        ViewType::BarChart => &[BAR_CHART_GROUP_BY, AGGREGATE_VALUE],
        ViewType::LineChart => &[LINE_CHART_X, LINE_CHART_Y, LINE_CHART_GROUP],
        ViewType::Workload => &[WORKLOAD_START, WORKLOAD_END, WORKLOAD_EFFORT],
        // `value` sits on each metric row rather than on the view; the
        // rule is the same, and the row is a shape the form owns.
        ViewType::Metric => &[AGGREGATE_VALUE],
        ViewType::Treemap => &[TREEMAP_GROUP, TREEMAP_SIZE],
        ViewType::Heatmap => &[HEATMAP_X, HEATMAP_Y, AGGREGATE_VALUE],
    }
}

/// The field types `aggregate` can reduce.
///
/// `count` counts items and takes no field at all — the empty set here is
/// "no field is acceptable", which is why `views_check` reports a `value:`
/// alongside `count` before ever consulting this.
pub fn aggregate_value_types(aggregate: Aggregate) -> &'static [FieldType] {
    match aggregate {
        Aggregate::Count => &[],
        Aggregate::Sum => MEASURABLE,
        // `min` and `max` order any comparable value, and an average date
        // is the midpoint of a window — all three carry dates through.
        Aggregate::Avg | Aggregate::Min | Aggregate::Max => PLOTTABLE,
    }
}

/// The prose form of an accepted-type list, for mismatch diagnostics:
/// `"choice, multichoice, or string"`.
///
/// Derived rather than written next to each slot, so a type added to a
/// slot cannot leave the message describing the old set.
pub fn describe(types: &[FieldType]) -> String {
    let names: Vec<String> = types.iter().map(ToString::to_string).collect();
    match names.as_slice() {
        [] => String::new(),
        [only] => only.clone(),
        [first, second] => format!("{first} or {second}"),
        [leading @ .., last] => format!("{}, or {last}", leading.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::VariantArray;

    #[test]
    fn describe_words_lists_of_every_length() {
        assert_eq!(describe(&[]), "");
        assert_eq!(describe(DATE), "date");
        assert_eq!(describe(ANY_LINK), "link or links");
        assert_eq!(describe(MEASURABLE), "integer, float, or duration");
        assert_eq!(
            describe(GROUPABLE),
            "choice, multichoice, string, list, link, or links"
        );
    }

    #[test]
    fn aggregate_value_slot_offers_every_type_some_function_accepts() {
        // The `value:` slot advertises one set before the function is
        // known; it has to be the union of the per-function sets, or the
        // form hides a field the CLI would have taken.
        let union: Vec<FieldType> = AGGREGATE_VALUE.accepts.to_vec();
        for &aggregate in Aggregate::VARIANTS {
            for accepted in aggregate_value_types(aggregate) {
                assert!(
                    union.contains(accepted),
                    "{aggregate} accepts {accepted}, which the `value` slot does not offer"
                );
            }
        }
        for offered in union {
            assert!(
                Aggregate::VARIANTS
                    .iter()
                    .any(|&aggregate| aggregate_value_types(aggregate).contains(&offered)),
                "the `value` slot offers {offered}, which no aggregate accepts"
            );
        }
    }

    #[test]
    fn every_slot_of_a_kind_has_a_distinct_name() {
        // The generated TypeScript table is keyed by slot name, so a
        // duplicate would silently drop one of the two.
        for &kind in ViewType::VARIANTS {
            let slots = slots_of(kind);
            for (index, slot) in slots.iter().enumerate() {
                assert!(
                    !slots[..index]
                        .iter()
                        .any(|earlier| earlier.name == slot.name),
                    "{kind} lists the slot `{}` twice",
                    slot.name
                );
            }
        }
    }
}
