//! Types and helpers shared across view_data extractors.
//!
//! `Card` is the resolved form of a work item for a specific view: id,
//! optional display title (from the view's `title:` slot), every field
//! set on the item (in schema order), and the freeform body text.
//! `UnplacedCard` carries items that couldn't be turned into the view's
//! natural mark (a bar, a point, a cell) — filter-matched but structurally
//! unrenderable — so the renderer can show them in a side panel or ignore
//! them.

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

/// Extract a numeric value (`Integer`, `Float`, or `Duration`) as `f64`.
///
/// Duration converts to its canonical seconds magnitude. Chart axes
/// using duration values display as raw seconds in v1 — there's no
/// per-axis unit-formatting hook yet.
pub(super) fn as_number(value: Option<&FieldValue>) -> Option<f64> {
    match value {
        Some(FieldValue::Integer(integer)) => Some(*integer as f64),
        Some(FieldValue::Float(float)) => Some(*float),
        Some(FieldValue::Duration(seconds)) => Some(*seconds as f64),
        _ => None,
    }
}

/// Extract an [`AxisValue`] — numeric or date — from a field value.
pub(super) fn as_axis(value: Option<&FieldValue>) -> Option<AxisValue> {
    match value {
        Some(FieldValue::Integer(integer)) => Some(AxisValue::Number(*integer as f64)),
        Some(FieldValue::Float(float)) => Some(AxisValue::Number(*float)),
        Some(FieldValue::Duration(seconds)) => Some(AxisValue::Number(*seconds as f64)),
        Some(FieldValue::Date(date)) => Some(AxisValue::Date(*date)),
        _ => None,
    }
}
