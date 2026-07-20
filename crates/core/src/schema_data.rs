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
//! A resource-backed field (`resource: <name>` in `schema.yaml`) still
//! carries its resource name; the resource's entries are reported once, in
//! [`SchemaData::resources`], so a value picker can be populated from the
//! resource rather than free text. The UI joins a field's `resource` name
//! to the matching [`ResourceList`].

use serde::Serialize;

use crate::model::resources::Resources;
use crate::model::schema::{FieldDefinition, FieldType, FieldTypeConfig, Schema};
use crate::model::WorkItemId;
use crate::query::types::{operators_for, Operator};
use crate::store::Store;

/// Every field type, in a stable order — the domain of
/// [`SchemaData::operators_by_type`].
const ALL_FIELD_TYPES: [FieldType; 12] = [
    FieldType::String,
    FieldType::Choice,
    FieldType::Multichoice,
    FieldType::Integer,
    FieldType::Float,
    FieldType::Date,
    FieldType::Duration,
    FieldType::Color,
    FieldType::Boolean,
    FieldType::List,
    FieldType::Link,
    FieldType::Links,
];

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
    /// Resource lists in `resources.yaml` declaration order. A field whose
    /// `resource` names a list here can offer that list's entries as a
    /// value picker. Reported once and joined by name rather than copied
    /// onto every field that references it.
    pub resources: Vec<ResourceList>,
    /// Which comparison operators each field type allows. Keyed by type
    /// (not repeated per field) since the operator set is purely a function
    /// of type — the UI reads a field's `field_type` and looks it up here.
    pub operators_by_type: Vec<FieldTypeOperators>,
    /// The built-in named palette for `color` fields, in declaration
    /// order. Reported once here so the color editor's swatch row and
    /// the filter builder read the same pinned values core resolves
    /// with — the UI keeps no copy of its own.
    pub palette: Vec<PaletteColor>,
}

/// One entry of the built-in color palette.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct PaletteColor {
    /// The name stored on items (e.g. `red`).
    pub name: String,
    /// The pinned `#rrggbb` the name resolves to.
    pub hex: String,
}

/// One resource section and its selectable entries.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct ResourceList {
    /// Section name, e.g. `people` — matches a field's `resource` value.
    pub name: String,
    /// The entries a value picker offers, in `resources.yaml` order.
    pub options: Vec<ResourceOption>,
}

/// A single selectable resource entry: the stored value plus its label.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct ResourceOption {
    /// The value stored on an item (the entry's `id`).
    pub id: String,
    /// Human-readable label (`name ?? id`).
    pub label: String,
}

/// The operators valid for one field type.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct FieldTypeOperators {
    /// The field type these operators apply to.
    pub field_type: FieldType,
    /// Operators the evaluator treats as meaningful for this type.
    pub operators: Vec<Operator>,
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

/// Build the editing vocabulary from a loaded schema, store, and resources.
pub fn build(schema: &Schema, store: &Store, resources: &Resources) -> SchemaData {
    let fields = schema
        .fields
        .iter()
        .map(|(name, definition)| FieldSchema::from_definition(name, definition))
        .collect();

    let mut items: Vec<WorkItemId> = store.all_items().map(|item| item.id.clone()).collect();
    items.sort_by(|left, right| left.as_str().cmp(right.as_str()));

    let resources = resources
        .sections
        .iter()
        .map(|(name, entries)| ResourceList {
            name: name.clone(),
            options: entries
                .iter()
                .map(|entry| ResourceOption {
                    id: entry.id.clone(),
                    label: entry.label().to_owned(),
                })
                .collect(),
        })
        .collect();

    let operators_by_type = ALL_FIELD_TYPES
        .into_iter()
        .map(|field_type| FieldTypeOperators {
            field_type,
            operators: operators_for(field_type),
        })
        .collect();

    let palette = crate::model::color::COLOR_PALETTE
        .iter()
        .map(|(name, hex)| PaletteColor {
            name: (*name).to_owned(),
            hex: (*hex).to_owned(),
        })
        .collect();

    SchemaData {
        fields,
        items,
        resources,
        operators_by_type,
        palette,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::resources::ResourceEntry;
    use crate::model::schema::{FieldDefinition, FieldTypeConfig};
    use indexmap::IndexMap;

    /// A schema with one plain field; enough to exercise `build`.
    fn schema_with_assignee() -> Schema {
        let mut fields = IndexMap::new();
        let mut assignee = FieldDefinition::new(FieldTypeConfig::String { pattern: None });
        assignee.resource = Some("people".to_owned());
        fields.insert("assignee".to_owned(), assignee);
        let inverse_table = Schema::build_inverse_table(&fields);
        Schema {
            fields,
            rules: vec![],
            inverse_table,
        }
    }

    fn empty_store(schema: &Schema) -> Store {
        let dir = tempfile::tempdir().unwrap();
        Store::load(dir.path(), schema).unwrap()
    }

    fn people_resources() -> Resources {
        let mut sections = IndexMap::new();
        sections.insert(
            "people".to_owned(),
            vec![
                ResourceEntry {
                    id: "alice".to_owned(),
                    name: Some("Alice Smith".to_owned()),
                },
                ResourceEntry {
                    id: "bob".to_owned(),
                    name: None,
                },
            ],
        );
        Resources { sections }
    }

    #[test]
    fn build_reports_resource_options_with_labels() {
        let schema = schema_with_assignee();
        let store = empty_store(&schema);
        let data = build(&schema, &store, &people_resources());

        assert_eq!(data.resources.len(), 1);
        let people = &data.resources[0];
        assert_eq!(people.name, "people");
        assert_eq!(people.options.len(), 2);
        // name present → label is the name
        assert_eq!(people.options[0].id, "alice");
        assert_eq!(people.options[0].label, "Alice Smith");
        // name absent → label falls back to id
        assert_eq!(people.options[1].id, "bob");
        assert_eq!(people.options[1].label, "bob");
    }

    #[test]
    fn build_with_no_resources_reports_empty_list() {
        let schema = schema_with_assignee();
        let store = empty_store(&schema);
        let data = build(&schema, &store, &Resources::default());
        assert!(data.resources.is_empty());
    }

    #[test]
    fn build_reports_the_builtin_palette() {
        let schema = schema_with_assignee();
        let store = empty_store(&schema);
        let data = build(&schema, &store, &Resources::default());

        assert_eq!(data.palette.len(), crate::model::color::COLOR_PALETTE.len());
        assert_eq!(data.palette[0].name, "red");
        assert_eq!(
            data.palette[0].hex,
            crate::model::color::resolve_color_to_hex("red").unwrap()
        );
    }

    #[test]
    fn build_reports_operators_for_every_field_type() {
        let schema = schema_with_assignee();
        let store = empty_store(&schema);
        let data = build(&schema, &store, &Resources::default());

        assert_eq!(data.operators_by_type.len(), ALL_FIELD_TYPES.len());
        // Each entry's operators match the canonical mapping.
        for entry in &data.operators_by_type {
            assert_eq!(entry.operators, operators_for(entry.field_type));
        }
    }
}
