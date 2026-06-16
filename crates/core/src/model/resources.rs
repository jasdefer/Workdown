//! Resource lists loaded from `resources.yaml`.
//!
//! Resources are named lists of entities (people, teams, sprints, …) that
//! a work-item field can reference via `resource: <name>` in `schema.yaml`.
//! A field so declared only accepts values matching an `id` from the named
//! section.
//!
//! The model carries only what the editing vocabulary needs: each entry's
//! `id` (the value stored on items) and an optional `name` (its display
//! label). The rest of an entry's freeform attributes are intentionally
//! dropped — nothing in the current milestone reads them, and typing them
//! would mean inventing a schema for `resources.yaml`, which is out of
//! scope. See the `schema-metadata-api` issue.
//!
//! Loading does not validate that an item's stored value matches a known
//! resource id — that check lives in the `resource-option-lists` issue.

use indexmap::IndexMap;

/// All resource lists in a project, keyed by section name in
/// declaration order (the order they appear in `resources.yaml`).
///
/// An absent or empty `resources.yaml` yields an empty `Resources` — a
/// valid configuration meaning "this project references no resources,"
/// not an error.
#[derive(Debug, Clone, Default)]
pub struct Resources {
    /// Section name (e.g. `people`) → its entries.
    pub sections: IndexMap<String, Vec<ResourceEntry>>,
}

impl Resources {
    /// The entries of one section, or `None` if no such section exists.
    pub fn section(&self, name: &str) -> Option<&[ResourceEntry]> {
        self.sections.get(name).map(Vec::as_slice)
    }

    /// Whether the project declares no resources at all.
    pub fn is_empty(&self) -> bool {
        self.sections.is_empty()
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
