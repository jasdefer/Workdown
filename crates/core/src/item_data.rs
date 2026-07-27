//! Single-item read projection — the current field values and body of
//! one work item, for the editing surface (`GET /api/items/:id`).
//!
//! Distinct from [`crate::view_data`] (which projects items *through* a
//! view's slots) and [`crate::schema_data`] (field *definitions*). The
//! detail panel and the standalone item page both need an item's current
//! values without a view in context, so this serves them directly.
//!
//! Reuses [`CardField`] for each field —
//! the typed, coerced value, in schema-declaration order. The `id` field
//! is the parser-stripped identity and is returned separately as `id`,
//! not among `fields` (it isn't mutable via `set`).

use serde::Serialize;

use crate::model::schema::Schema;
use crate::model::views::DisplayConfig;
use crate::model::work_item::WorkItem;
use crate::model::WorkItemId;
use crate::view_data::CardField;

/// One work item's editable state.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct ItemDetail {
    pub id: WorkItemId,
    /// Resolved `#rrggbb` the detail surface tints itself with; `None`
    /// when untinted. Same hex convention as
    /// [`Card::background`](crate::view_data::Card::background). The
    /// detail surface has no view in context, so only the project-wide
    /// rungs of the `color` role apply: `defaults.display.color` from
    /// `config.yaml` (including its `none` off switch), then the
    /// first-`color`-field-in-schema-order fallback. Per-view `display:`
    /// blocks and session overrides never reach here.
    pub background: Option<String>,
    /// Each schema-declared field the item has a value for, in schema
    /// order. Fields the item doesn't set are omitted — the editor pulls
    /// the full field list (and how to render absent ones) from
    /// `GET /api/schema`.
    pub fields: Vec<CardField>,
    /// The freeform Markdown body, rendered read-only in the UI.
    pub body: String,
}

/// Build the detail projection for a single item. `display_defaults` is
/// the project-wide `defaults.display` from `config.yaml` — the only
/// display-role rung that applies to a surface without a view.
pub fn build(item: &WorkItem, schema: &Schema, display_defaults: &DisplayConfig) -> ItemDetail {
    let fields = schema
        .fields
        .iter()
        .filter_map(|(name, config)| {
            item.fields.get(name).map(|value| CardField {
                name: name.clone(),
                value: value.clone(),
                field_type: config.field_type(),
            })
        })
        .collect();

    ItemDetail {
        id: item.id.clone(),
        background: crate::view_data::resolved_background(item, schema, Some(display_defaults)),
        fields,
        body: item.body.clone(),
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::{FieldType, FieldTypeConfig};
    use crate::model::views::ColorRole;
    use crate::model::FieldValue;
    use crate::view_data::test_support::{make_item, make_schema};

    fn color_schema() -> Schema {
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

    #[test]
    fn fields_carry_types_in_schema_order_and_omit_absent() {
        let schema = color_schema();
        // Set out of schema order; `risk_color` absent.
        let item = make_item(
            "task-a",
            vec![
                ("team_color", FieldValue::Color("red".into())),
                ("status", FieldValue::Choice("open".into())),
            ],
            "the body",
        );

        let detail = build(&item, &schema, &DisplayConfig::default());

        assert_eq!(detail.id.as_str(), "task-a");
        assert_eq!(detail.body, "the body");
        let names: Vec<&str> = detail
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect();
        assert_eq!(names, vec!["status", "team_color"]);
        let types: Vec<FieldType> = detail.fields.iter().map(|field| field.field_type).collect();
        assert_eq!(types, vec![FieldType::Choice, FieldType::Color]);
    }

    #[test]
    fn background_applies_the_config_defaults_rung() {
        // The detail surface has no view in context, so
        // `defaults.display.color` is the only configurable rung: it
        // picks the field, `none` disables, unset falls back to the
        // first color field in schema order.
        let schema = color_schema();
        let item = make_item(
            "task-a",
            vec![
                ("team_color", FieldValue::Color("red".into())),
                ("risk_color", FieldValue::Color("#123456".into())),
            ],
            "",
        );

        let by_default = DisplayConfig {
            color: Some(ColorRole::Field("risk_color".into())),
            ..DisplayConfig::default()
        };
        assert_eq!(
            build(&item, &schema, &by_default).background.as_deref(),
            Some("#123456")
        );

        let off = DisplayConfig {
            color: Some(ColorRole::None),
            ..DisplayConfig::default()
        };
        assert_eq!(build(&item, &schema, &off).background, None);

        // Unset: first color field in schema order, palette-resolved.
        assert_eq!(
            build(&item, &schema, &DisplayConfig::default())
                .background
                .as_deref(),
            Some("#ef4444")
        );
    }
}
