//! Types and helpers shared across view_data extractors.
//!
//! `Card` is the resolved form of a work item for a specific view: id,
//! optional display title (from the view's `title:` slot), every field
//! set on the item (in schema order), and the freeform body text.
//! `UnplacedCard` carries items that couldn't be turned into the view's
//! natural mark (a bar, a point, a cell) — filter-matched but structurally
//! unrenderable — so the renderer can show them in a side panel or ignore
//! them.

use std::ops::Add;

use chrono::NaiveDate;
use serde::Serialize;

use crate::model::field_value::format_field_value;
use crate::model::schema::{FieldType, Schema};
use crate::model::views::{Bucket, View};
use crate::model::{FieldValue, WorkItem, WorkItemId};

// ── Column (shared by table and tree) ───────────────────────────────

/// One user-configured column in a column-bearing view (table, tree).
///
/// Carries the schema field name and its [`FieldType`] so renderers can
/// align and format cells deterministically even when every cell in the
/// column is `None`. The virtual `id` column is represented with
/// [`FieldType::String`].
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct Column {
    pub name: String,
    pub field_type: FieldType,
}

/// Resolve a user-configured column name to its [`Column`] payload.
///
/// `views_check` guarantees every non-`id` name resolves in
/// `schema.fields`; the lookup is `expect`-safe here.
pub fn build_column(name: &str, schema: &Schema) -> Column {
    let field_type = if name == "id" {
        FieldType::String
    } else {
        schema
            .fields
            .get(name)
            .expect("views_check validates column references")
            .field_type()
    };
    Column {
        name: name.to_owned(),
        field_type,
    }
}

/// Resolve a single cell value for a given column and item.
///
/// The virtual `id` column emits the item id as a String cell; real
/// fields emit their typed [`FieldValue`] when set, `None` otherwise.
pub fn column_cell(column_name: &str, item: &WorkItem) -> Option<FieldValue> {
    if column_name == "id" {
        Some(FieldValue::String(item.id.as_str().to_owned()))
    } else {
        item.fields.get(column_name).cloned()
    }
}

// ── Card ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct Card {
    pub id: WorkItemId,
    pub title: Option<String>,
    pub fields: Vec<CardField>,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct CardField {
    pub name: String,
    pub value: FieldValue,
    /// The field's [`FieldType`], carried alongside the value so
    /// card/tooltip renderers can format a date, choice, or link
    /// correctly from the value alone — as the table/tree column
    /// payload already does.
    pub field_type: FieldType,
}

/// A lightweight resolved reference to a work item — just its display
/// title, keyed by id in a view's `items` sidecar map. Link/Links cells
/// (table) and point ids (line chart) resolve through it so renderers can
/// show a linked item by name rather than raw id.
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
pub struct ItemRef {
    /// Resolved via the view's `title:` slot. `None` when the view has
    /// no title slot configured or the linked item lacks that field —
    /// the UI falls back to `prettifyId(id)` in that case.
    pub title: Option<String>,
}

/// Build a Card from a work item, resolving the view's title slot.
///
/// Fields emitted: each schema-declared field the item actually has a
/// value for, in schema-declaration order. Fields not present on the
/// item are omitted (not padded with `None`) — consumers that need a
/// uniform shape can consult the schema themselves.
pub fn build_card(item: &WorkItem, schema: &Schema, view: &View) -> Card {
    let title = resolve_title(item, view);
    let mut fields = Vec::new();
    for (field_name, config) in &schema.fields {
        if let Some(value) = item.fields.get(field_name) {
            fields.push(CardField {
                name: field_name.clone(),
                value: value.clone(),
                field_type: config.field_type(),
            });
        }
    }
    Card {
        id: item.id.clone(),
        title,
        fields,
        body: item.body.clone(),
    }
}

/// Resolve a card's display title via the view's `title:` slot.
///
/// Returns `None` when the view has no `title:` set or when the referenced
/// field isn't set on this item — renderers fall back to the item id.
/// The virtual `id` slot returns the item id as a string.
pub fn resolve_title(item: &WorkItem, view: &View) -> Option<String> {
    let slot = view.title.as_deref()?;
    if slot == "id" {
        return Some(item.id.as_str().to_owned());
    }
    item.fields.get(slot).map(format_field_value)
}

// ── Unplaced items ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct UnplacedCard {
    pub card: Card,
    pub reason: UnplacedReason,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UnplacedReason {
    MissingValue {
        field: String,
    },
    InvalidRange {
        start_field: String,
        end_field: String,
    },
    /// Workload: an item's `[start..=end]` interval is non-empty but
    /// every day inside it falls on a non-working day per the active
    /// calendar. The effort has nowhere to land. Authoring problem worth
    /// surfacing rather than silently dropping or relaxing the calendar.
    NoWorkingDays {
        start_field: String,
        end_field: String,
    },
    NonNumericValue {
        field: String,
        got: FieldType,
    },
    /// Gantt after-mode: a root item (empty `after`) had no `start` field
    /// value to anchor it. Predecessor mode requires every chain to start
    /// somewhere — either an explicit predecessor or an explicit start date.
    NoAnchor,
    /// Gantt after-mode: a predecessor exists but couldn't be resolved
    /// (missing duration, missing start on its own root, transitively
    /// unresolvable, etc.). The chain breaks at `id`.
    PredecessorUnresolved {
        id: String,
    },
    /// Gantt after-mode: defense-in-depth catch when topo sort can't
    /// drain the queue. Indicates a cycle in the `via` link field that
    /// upstream `allow_cycles: false` validation should already have
    /// flagged.
    Cycle {
        via: String,
    },
}

/// Sort unplaced cards by work-item id ascending — the stable order every
/// view presents its "couldn't place these" list in.
pub(super) fn sort_unplaced(unplaced: &mut [UnplacedCard]) {
    unplaced.sort_by(|left, right| left.card.id.as_str().cmp(right.card.id.as_str()));
}

// ── Aggregate / axis values ─────────────────────────────────────────

/// Result of an aggregate (sum/avg/min/max) on a numeric, date, or
/// duration field.
///
/// Count always produces `Number(n as f64)`. Sum applies to numeric or
/// duration fields; avg/min/max apply to numeric, date, or duration
/// fields. `Duration` is returned (rather than `Number(seconds_as_f64)`)
/// when the input field is a duration field, so renderers can format
/// `5d` instead of `432000`.
///
/// Wire shape is tagged (`{type, value}`) so the frontend can recover
/// the variant. JSON has no bigint and an untagged i64 would land as a
/// JS number, indistinguishable from a `Number(seconds_as_f64)` —
/// renderers couldn't tell `5d` from raw `432000`. The Duration's i64
/// fits inside JS number's safe range for any human time scale, so the
/// value is typed as `number` on the TS side.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, ts_rs::TS)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum AggregateValue {
    Number(f64),
    Date(NaiveDate),
    Duration(#[ts(type = "number")] i64),
}

/// A point coordinate on a chart's x-axis (or similar).
///
/// Wire shape is tagged (`{type, value}`) for the same variant-recovery
/// reason as [`AggregateValue`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, ts_rs::TS)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum AxisValue {
    Number(f64),
    Date(NaiveDate),
    Duration(#[ts(type = "number")] i64),
}

/// A magnitude carried by a non-aggregated leaf field — the size column
/// of a treemap, the y-coordinate of a line chart, etc.
///
/// Mirrors the `Number`/`Duration` arms of [`AggregateValue`] without
/// the `Date` arm (sizes can't be dates). Carrying the variant through
/// the data structure lets downstream renderers format `5d` instead of
/// raw seconds — same role `AggregateValue` plays for metric/heatmap.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, ts_rs::TS)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum SizeValue {
    Number(f64),
    Duration(#[ts(type = "number")] i64),
}

impl SizeValue {
    /// Magnitude as `f64`. For `Duration`, this is canonical seconds.
    /// Used for sorting and percentage math, where the variant doesn't
    /// matter — only relative magnitude does.
    pub fn as_f64(self) -> f64 {
        match self {
            SizeValue::Number(number) => number,
            SizeValue::Duration(seconds) => seconds as f64,
        }
    }
}

/// Sum two values of the same variant. Mixing variants is a programming
/// error — every `SizeValue` in a single tree comes from the same
/// schema field, so the variant is uniform.
impl Add for SizeValue {
    type Output = SizeValue;

    fn add(self, other: Self) -> Self {
        match (self, other) {
            (SizeValue::Number(left), SizeValue::Number(right)) => SizeValue::Number(left + right),
            (SizeValue::Duration(left), SizeValue::Duration(right)) => {
                SizeValue::Duration(left + right)
            }
            _ => panic!("SizeValue::add called with mismatched variants"),
        }
    }
}

/// The zero [`SizeValue`] for a size field — `Duration(0)` for a duration
/// field, `Number(0.0)` otherwise — so a treemap frame can start an empty
/// accumulator in the field's own variant. `views_check` guarantees the
/// field resolves to an allowed numeric type; an unexpected type falls
/// back to `Number(0)` defensively.
pub(super) fn zero_for_size_field(field: &str, schema: &Schema) -> SizeValue {
    match schema.fields.get(field).map(|config| config.field_type()) {
        Some(FieldType::Duration) => SizeValue::Duration(0),
        _ => SizeValue::Number(0.0),
    }
}

// ── FieldValue conversions ──────────────────────────────────────────

/// Extract a [`NaiveDate`] from a field value, if it is one.
pub(super) fn as_date(value: Option<&FieldValue>) -> Option<NaiveDate> {
    match value {
        Some(FieldValue::Date(date)) => Some(*date),
        _ => None,
    }
}

/// Extract canonical seconds from a `Duration` field value, if it is one.
pub(super) fn as_duration_seconds(value: Option<&FieldValue>) -> Option<i64> {
    match value {
        Some(FieldValue::Duration(seconds)) => Some(*seconds),
        _ => None,
    }
}

/// Extract a [`SizeValue`] from a numeric field (`Integer`, `Float`, or
/// `Duration`). Preserves the duration variant so renderers can format
/// `5d` instead of raw seconds; for sorting/arithmetic that doesn't
/// care about the unit, callers pull the magnitude via [`SizeValue::as_f64`].
pub(super) fn as_size(value: Option<&FieldValue>) -> Option<SizeValue> {
    match value {
        Some(FieldValue::Integer(integer)) => Some(SizeValue::Number(*integer as f64)),
        Some(FieldValue::Float(float)) => Some(SizeValue::Number(*float)),
        Some(FieldValue::Duration(seconds)) => Some(SizeValue::Duration(*seconds)),
        _ => None,
    }
}

/// Extract an [`AxisValue`] — numeric, date, or duration — from a field
/// value. Duration values keep their variant so renderers can format
/// axis ticks as `1d` instead of `86400`.
pub(super) fn as_axis(value: Option<&FieldValue>) -> Option<AxisValue> {
    match value {
        Some(FieldValue::Integer(integer)) => Some(AxisValue::Number(*integer as f64)),
        Some(FieldValue::Float(float)) => Some(AxisValue::Number(*float)),
        Some(FieldValue::Duration(seconds)) => Some(AxisValue::Duration(*seconds)),
        Some(FieldValue::Date(date)) => Some(AxisValue::Date(*date)),
        _ => None,
    }
}

// ── Grouping / axis keys ────────────────────────────────────────────

/// Stringify a field's value into the group keys (bar chart) or axis
/// labels (heatmap) it contributes. Multichoice/list/links spread across
/// multiple keys; a `Date` formats via `bucket` (day = `YYYY-MM-DD`, week
/// = ISO `YYYY-Www`, month = `YYYY-MM`); everything else stringifies via
/// [`format_field_value`]. `bucket` is `None` for views without date
/// bucketing (bar chart), where a date takes the day format — identical
/// to `format_field_value`'s date output, so the behaviour is unchanged.
pub(super) fn group_keys(item: &WorkItem, field: &str, bucket: Option<Bucket>) -> Vec<String> {
    match item.fields.get(field) {
        None => Vec::new(),
        Some(FieldValue::Multichoice(values)) => values.clone(),
        Some(FieldValue::List(values)) => values.clone(),
        Some(FieldValue::Links(ids)) => ids.iter().map(|id| id.as_str().to_owned()).collect(),
        Some(FieldValue::Date(date)) => {
            let formatted = match bucket {
                Some(Bucket::Week) => date.format("%G-W%V").to_string(),
                Some(Bucket::Month) => date.format("%Y-%m").to_string(),
                Some(Bucket::Day) | None => date.format("%Y-%m-%d").to_string(),
            };
            vec![formatted]
        }
        Some(other) => vec![format_field_value(other)],
    }
}
