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
use crate::model::work_item::WorkItem;
use crate::model::WorkItemId;
use crate::view_data::CardField;

/// One work item's editable state.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct ItemDetail {
    pub id: WorkItemId,
    /// Resolved `#rrggbb` of the item's first `color` field in schema
    /// order; `None` when unset. Same hex convention as
    /// [`Card::background`](crate::view_data::Card::background) — the
    /// detail surface tints itself with it. Unlike a view's cards this
    /// stays on the schema-order fallback: the detail surface honors no
    /// display roles (it shows everything), so the `color` role doesn't
    /// apply here either.
    pub background: Option<String>,
    /// Each schema-declared field the item has a value for, in schema
    /// order. Fields the item doesn't set are omitted — the editor pulls
    /// the full field list (and how to render absent ones) from
    /// `GET /api/schema`.
    pub fields: Vec<CardField>,
    /// The freeform Markdown body, rendered read-only in the UI.
    pub body: String,
}

/// Build the detail projection for a single item.
pub fn build(item: &WorkItem, schema: &Schema) -> ItemDetail {
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
        background: crate::view_data::resolved_background(item, schema, None),
        fields,
        body: item.body.clone(),
    }
}
