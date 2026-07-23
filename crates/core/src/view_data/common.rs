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
use crate::model::views::{Bucket, ColorRole, DisplayConfig, View};
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
    /// Secondary line, resolved via the view's `subtitle` display role.
    /// `None` when the role is unset or the item lacks the field.
    pub subtitle: Option<String>,
    /// Resolved `#rrggbb` of the item's value for the field the view's
    /// `color` display role picks (see [`resolved_background`]); `None`
    /// when the role is `none` or the item has no value. Renderers tint
    /// the item's surface with it and keep their neutral default
    /// otherwise.
    pub background: Option<String>,
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

/// Build a Card from a work item, resolving the view's display roles.
///
/// Fields emitted: each entry of [`effective_fields`] the item actually
/// has a value for, in role order (user's order when the `fields` role
/// is set, schema-declaration order in the fallback). Fields not present
/// on the item are omitted (not padded with `None`) — consumers that
/// need a uniform shape can consult the schema themselves.
pub fn build_card(item: &WorkItem, schema: &Schema, view: &View) -> Card {
    let mut fields = Vec::new();
    for field_name in effective_fields(view, schema) {
        let Some(config) = schema.fields.get(&field_name) else {
            continue; // the virtual `id` — carried on the card itself
        };
        if let Some(value) = item.fields.get(&field_name) {
            fields.push(CardField {
                name: field_name,
                value: value.clone(),
                field_type: config.field_type(),
            });
        }
    }
    Card {
        id: item.id.clone(),
        title: resolve_title(item, view),
        subtitle: resolve_subtitle(item, view),
        background: resolved_background(item, schema, Some(&view.display)),
        fields,
        body: item.body.clone(),
    }
}

/// Resolve which schema field feeds the background tint — the single
/// implementation of the `color` display role's resolution, shared by
/// every surface that tints (view extractors and the item detail).
///
/// By the time extraction runs, the role's upper rungs are already
/// merged into one value (session override › view `display:` › config
/// defaults — see [`DisplayConfig::or_inherit`]):
///
/// - [`ColorRole::None`] — tinting is off; resolves to no field.
/// - [`ColorRole::Field`] — that field, provided it exists and is
///   `color`-typed. Anything else falls through to the fallback:
///   `views_check` guarantees view-level config, but a stale session
///   override or an unvalidated `defaults.display` entry must degrade
///   gracefully, not panic or mistint.
/// - Unset (including `display: None` — surfaces with no view in
///   context) — the fallback: the first `color`-typed field in schema
///   order, mirroring how the first compatible `choice` field backs a
///   board.
pub fn resolve_color_field<'schema>(
    schema: &'schema Schema,
    display: Option<&DisplayConfig>,
) -> Option<&'schema str> {
    match display.and_then(|config| config.color.as_ref()) {
        Some(ColorRole::None) => return None,
        Some(ColorRole::Field(name)) => {
            if let Some((schema_name, definition)) = schema.fields.get_key_value(name.as_str()) {
                if definition.field_type() == FieldType::Color {
                    return Some(schema_name.as_str());
                }
            }
        }
        None => {}
    }
    schema
        .fields
        .iter()
        .find(|(_, definition)| definition.field_type() == FieldType::Color)
        .map(|(name, _)| name.as_str())
}

/// Resolve an item's background tint to `#rrggbb`: the item's value for
/// the field [`resolve_color_field`] picks, or `None` when tinting is
/// off, the item has no value, or coercion already dropped an invalid
/// one — keeping the neutral default background. Resolution happens
/// here, once, so every consumer downstream only ever sees finished hex.
pub fn resolved_background(
    item: &WorkItem,
    schema: &Schema,
    display: Option<&DisplayConfig>,
) -> Option<String> {
    let field_name = resolve_color_field(schema, display)?;
    match item.fields.get(field_name) {
        Some(FieldValue::Color(canonical)) => crate::model::color::resolve_color_to_hex(canonical),
        _ => None,
    }
}

/// The view's `fields` display role, or every schema field in
/// declaration order when the role is unset — preserving the
/// show-everything behavior views had before display roles existed.
///
/// Names that resolve neither in `schema.fields` nor to the virtual
/// `id` are dropped: `views_check` guarantees the view's own role
/// entries resolve, but entries inherited from `defaults.display` in
/// `config.yaml` are not yet validated anywhere, and must not panic
/// the extractor.
pub fn effective_fields(view: &View, schema: &Schema) -> Vec<String> {
    if view.display.fields.is_empty() {
        schema.fields.keys().cloned().collect()
    } else {
        view.display
            .fields
            .iter()
            .filter(|name| *name == "id" || schema.fields.contains_key(*name))
            .cloned()
            .collect()
    }
}

/// Resolve a card's display title via the view's `title` display role.
///
/// Returns `None` when the role is unset or when the referenced field
/// isn't set on this item — renderers fall back to the item id. The
/// virtual `id` returns the item id as a string.
pub fn resolve_title(item: &WorkItem, view: &View) -> Option<String> {
    resolve_text_role(item, view.display.title.as_deref())
}

/// Resolve a card's secondary line via the view's `subtitle` display
/// role. Same semantics as [`resolve_title`].
pub fn resolve_subtitle(item: &WorkItem, view: &View) -> Option<String> {
    resolve_text_role(item, view.display.subtitle.as_deref())
}

fn resolve_text_role(item: &WorkItem, slot: Option<&str>) -> Option<String> {
    let slot = slot?;
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

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::FieldTypeConfig;
    use crate::view_data::test_support::{make_item, make_schema};

    fn two_color_schema() -> Schema {
        make_schema(vec![
            (
                "status",
                FieldTypeConfig::Choice {
                    values: vec!["open".into()],
                },
            ),
            ("team_color", FieldTypeConfig::Color),
            ("risk_color", FieldTypeConfig::Color),
        ])
    }

    fn display_with_color(color: Option<ColorRole>) -> DisplayConfig {
        DisplayConfig {
            color,
            ..DisplayConfig::default()
        }
    }

    #[test]
    fn color_field_defaults_to_first_in_schema_order() {
        let schema = two_color_schema();
        assert_eq!(resolve_color_field(&schema, None), Some("team_color"));
        let unset = display_with_color(None);
        assert_eq!(
            resolve_color_field(&schema, Some(&unset)),
            Some("team_color")
        );
    }

    #[test]
    fn color_role_picks_the_named_field() {
        let schema = two_color_schema();
        let display = display_with_color(Some(ColorRole::Field("risk_color".into())));
        assert_eq!(
            resolve_color_field(&schema, Some(&display)),
            Some("risk_color")
        );
    }

    #[test]
    fn color_role_none_disables_tinting() {
        let schema = two_color_schema();
        let display = display_with_color(Some(ColorRole::None));
        assert_eq!(resolve_color_field(&schema, Some(&display)), None);
    }

    #[test]
    fn stale_color_role_falls_back_to_schema_order() {
        // A session override can outlive its field (deleted or retyped
        // since it was saved) — degrade to the fallback, never panic.
        let schema = two_color_schema();
        let deleted = display_with_color(Some(ColorRole::Field("gone".into())));
        assert_eq!(
            resolve_color_field(&schema, Some(&deleted)),
            Some("team_color")
        );
        let retyped = display_with_color(Some(ColorRole::Field("status".into())));
        assert_eq!(
            resolve_color_field(&schema, Some(&retyped)),
            Some("team_color")
        );
    }

    #[test]
    fn no_color_fields_resolves_to_nothing() {
        let schema = make_schema(vec![(
            "status",
            FieldTypeConfig::Choice {
                values: vec!["open".into()],
            },
        )]);
        assert_eq!(resolve_color_field(&schema, None), None);
    }

    #[test]
    fn background_follows_the_resolved_field() {
        let schema = two_color_schema();
        let item = make_item(
            "a",
            vec![
                ("team_color", FieldValue::Color("red".into())),
                ("risk_color", FieldValue::Color("#123456".into())),
            ],
            "",
        );

        let by_role = display_with_color(Some(ColorRole::Field("risk_color".into())));
        assert_eq!(
            resolved_background(&item, &schema, Some(&by_role)).as_deref(),
            Some("#123456")
        );

        let off = display_with_color(Some(ColorRole::None));
        assert_eq!(resolved_background(&item, &schema, Some(&off)), None);

        // No display in context (the item detail surface): first color
        // field in schema order, resolved to its pinned hex.
        assert_eq!(
            resolved_background(&item, &schema, None).as_deref(),
            Some("#ef4444")
        );
    }
}
