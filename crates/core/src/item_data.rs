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
