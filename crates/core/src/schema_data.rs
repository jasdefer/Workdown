//! Editing vocabulary for the UI — the projection of a project's
//! [`Schema`] (plus the item id index) that the web client needs to
//! render field editors and the create-item form.
//!
//! This mirrors [`crate::view_data`]: a wire-level type with a `ts_rs`
//! derive so `cargo xtask gen-types` can emit the matching TypeScript,
//! built by a thin function the server handler calls. The server stays
//! a thin HTTP wrapper; the projection logic lives here so it can be
//! unit-tested in core.
//!
//! Served by `GET /api/schema`. Fetched once by the client and reused
//! across the detail panel and the create form.
//!
//! Note: resource-backed fields (`resource: <name>` in `schema.yaml`)
//! carry the resource name as a hint, but no option list — `core` does
//! not load or validate `resources.yaml` yet, so the UI edits these as
//! free text for now.

use serde::Serialize;

use crate::model::schema::{FieldDefinition, FieldType, FieldTypeConfig, Schema};
use crate::model::WorkItemId;
use crate::store::Store;

/// Everything the UI needs to build editors for a project.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct SchemaData {
    /// Field definitions in schema-declaration order (matters for form
    /// and panel layout — same order the board uses for columns).
    pub fields: Vec<FieldSchema>,
    /// All work-item ids, sorted. Populates link/links pickers; the UI
    /// prettifies each id for its label (there is no global title field
    /// to resolve against outside a view).
    pub items: Vec<WorkItemId>,
}

/// A single field's editing metadata — enough to pick the right editor
/// and give immediate client-side hints. Server-side coercion remains
/// the source of truth (save-with-warning); these only shape the UI.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct FieldSchema {
    /// Field name (the frontmatter key).
    pub name: String,
    /// Built-in type — drives which editor the UI renders.
    pub field_type: FieldType,
    /// Human-readable explanation from the schema, if any.
    pub description: Option<String>,
    /// Whether the field must be present on every item.
    pub required: bool,
    /// Allowed values for `choice` / `multichoice`; `None` otherwise.
    pub values: Option<Vec<String>>,
    /// Inclusive lower bound for `integer` / `float`; `None` otherwise.
    pub min: Option<f64>,
    /// Inclusive upper bound for `integer` / `float`; `None` otherwise.
    pub max: Option<f64>,
    /// Inclusive lower bound for `duration`, in canonical seconds.
    pub duration_min: Option<i64>,
    /// Inclusive upper bound for `duration`, in canonical seconds.
    pub duration_max: Option<i64>,
    /// Regex the value must match, for `string` fields; `None` otherwise.
    pub pattern: Option<String>,
    /// Resource section that backs this field, if any. A hint only —
    /// see the module note; the UI edits resource fields as free text.
    pub resource: Option<String>,
    /// `true` for computed fields (an `aggregate` config). The UI marks
    /// these so the user knows the value rolls up the link chain.
    pub aggregate: bool,
}

impl FieldSchema {
    fn from_definition(name: &str, definition: &FieldDefinition) -> Self {
        let values = match &definition.type_config {
            FieldTypeConfig::Choice { values } | FieldTypeConfig::Multichoice { values } => {
                Some(values.clone())
            }
            _ => None,
        };
        let (min, max) = match &definition.type_config {
            FieldTypeConfig::Integer { min, max } | FieldTypeConfig::Float { min, max } => {
                (*min, *max)
            }
            _ => (None, None),
        };
        let (duration_min, duration_max) = match &definition.type_config {
            FieldTypeConfig::Duration { min, max } => (*min, *max),
            _ => (None, None),
        };
        let pattern = match &definition.type_config {
            FieldTypeConfig::String { pattern } => pattern.clone(),
            _ => None,
        };

        Self {
            name: name.to_owned(),
            field_type: definition.field_type(),
            description: definition.description.clone(),
            required: definition.required,
            values,
            min,
            max,
            duration_min,
            duration_max,
            pattern,
            resource: definition.resource.clone(),
            aggregate: definition.aggregate.is_some(),
        }
    }
}

/// Build the editing vocabulary from a loaded schema and store.
pub fn build(schema: &Schema, store: &Store) -> SchemaData {
    let fields = schema
        .fields
        .iter()
        .map(|(name, definition)| FieldSchema::from_definition(name, definition))
        .collect();

    let mut items: Vec<WorkItemId> = store.all_items().map(|item| item.id.clone()).collect();
    items.sort_by(|left, right| left.as_str().cmp(right.as_str()));

    SchemaData { fields, items }
}
