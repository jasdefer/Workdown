//! Resource lists and project constants loaded from `resources.yaml`.
//!
//! Resources are named lists of entities (people, teams, sprints, …) that
//! a work-item field can reference via `resource: <name>` in `schema.yaml`.
//! A field so declared only accepts values matching an `id` from the named
//! section.
//!
//! The reserved top-level key `constants` holds named scalar values defined
//! once per project (a daily rate, a work-hours-per-day convention) that
//! schema-level consumers — computed-field expressions, rule configs —
//! resolve by name. Constants live here rather than in `schema.yaml`
//! because they are *data* that changes over a project's life, not
//! structure.
//!
//! The model carries only what the editing vocabulary needs: each entry's
//! `id` (the value stored on items) and an optional `name` (its display
//! label). The rest of an entry's freeform attributes are intentionally
//! dropped — nothing in the current milestone reads them, and typing them
//! would mean inventing a schema for `resources.yaml`, which is out of
//! scope. See the `schema-metadata-api` issue.
//!
//! Loading does not validate anything. Whether a field's section exists
//! and is populated is checked in [`crate::resources_check`]; whether an
//! item's stored value matches a known entry is checked in the store.

use indexmap::IndexMap;

use crate::model::FieldValue;

/// All resource lists and constants in a project, keyed by section name
/// in declaration order (the order they appear in `resources.yaml`).
///
/// An absent or empty `resources.yaml` yields an empty `Resources` — a
/// valid configuration meaning "this project references no resources,"
/// not an error.
#[derive(Debug, Clone, Default)]
pub struct Resources {
    /// Section name (e.g. `people`) → its entries.
    pub sections: IndexMap<String, Vec<ResourceEntry>>,
    /// Constant name → its typed value, from the reserved `constants`
    /// section, in declaration order. Values are already coerced to the
    /// scalar type each constant declares.
    pub constants: IndexMap<String, FieldValue>,
    /// Whether a `resources.yaml` document was successfully parsed into
    /// this value. Load provenance rather than data, but the
    /// resource-reference check needs it to tell two situations apart:
    /// a section name the document does not declare is a typo (error),
    /// while no document at all is just an unconfigured project
    /// (warning). A file that failed to parse leaves this `false` — its
    /// read error already explains the absence, and downgrading the
    /// follow-on findings avoids stacking a second complaint on one
    /// cause.
    pub document_loaded: bool,
}

impl Resources {
    /// The entries of one section, or `None` if no such section exists.
    pub fn section(&self, name: &str) -> Option<&[ResourceEntry]> {
        self.sections.get(name).map(Vec::as_slice)
    }

    /// The value of one constant, or `None` if no such constant exists.
    pub fn constant(&self, name: &str) -> Option<&FieldValue> {
        self.constants.get(name)
    }

    /// Whether the project declares no resources or constants at all.
    pub fn is_empty(&self) -> bool {
        self.sections.is_empty() && self.constants.is_empty()
    }
}

/// A single entry within a resource section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceEntry {
    /// Unique identifier within its section. This is the value a
    /// `resource:`-backed field stores.
    pub id: String,
    /// Human-readable display name, if the entry sets one.
    pub name: Option<String>,
}

impl ResourceEntry {
    /// Display label: the `name` when present, otherwise the `id`. This is
    /// the default labelling policy (`name ?? id`); a future display-config
    /// feature may let a project pick a different attribute.
    pub fn label(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.id)
    }
}
