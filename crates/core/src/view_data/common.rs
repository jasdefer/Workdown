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

use crate::model::schema::{FieldType, Schema};
use crate::model::views::View;
use crate::model::{FieldValue, WorkItem, WorkItemId};
use crate::query::format::format_field_value;

// ── Card ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct Card {
    pub id: WorkItemId,
    pub title: Option<String>,
    pub fields: Vec<CardField>,
    pub body: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CardField {
    pub name: String,
    pub value: FieldValue,
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
    for field_name in schema.fields.keys() {
        if let Some(value) = item.fields.get(field_name) {
            fields.push(CardField {
                name: field_name.clone(),
                value: value.clone(),
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

#[derive(Debug, Clone, Serialize)]
pub struct UnplacedCard {
    pub card: Card,
    pub reason: UnplacedReason,
}

#[derive(Debug, Clone, Serialize)]
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

// ── Aggregate / axis values ─────────────────────────────────────────

/// Result of an aggregate (sum/avg/min/max) on a numeric, date, or
/// duration field.
///
/// Count always produces `Number(n as f64)`. Sum applies to numeric or
/// duration fields; avg/min/max apply to numeric, date, or duration
/// fields. `Duration` is returned (rather than `Number(seconds_as_f64)`)
/// when the input field is a duration field, so renderers can format
/// `5d` instead of `432000`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(untagged)]
pub enum AggregateValue {
    Number(f64),
    Date(NaiveDate),
    Duration(i64),
}

/// A point coordinate on a chart's x-axis (or similar).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(untagged)]
pub enum AxisValue {
    Number(f64),
    Date(NaiveDate),
    Duration(i64),
}

/// A magnitude carried by a non-aggregated leaf field — the size column
/// of a treemap, the y-coordinate of a line chart, etc.
///
/// Mirrors the `Number`/`Duration` arms of [`AggregateValue`] without
/// the `Date` arm (sizes can't be dates). Carrying the variant through
/// the data structure lets downstream renderers format `5d` instead of
/// raw seconds — same role `AggregateValue` plays for metric/heatmap.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(untagged)]
pub enum SizeValue {
    Number(f64),
    Duration(i64),
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
