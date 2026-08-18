//! HTTP mutation contracts — the request and response wire types for
//! the server's write endpoints, plus the mapping to the `operations`
//! layer.
//!
//! Like [`crate::view_data`] and [`crate::schema_data`], these carry a
//! `ts_rs` derive so `cargo xtask gen-types` emits matching TypeScript.
//! The op→[`SetOperation`] mapping lives here (next to the wire shape it
//! decodes) so the server handler stays a thin deserialize-and-dispatch
//! wrapper and the contract is unit-testable in core.
//!
//! Values cross the wire as opaque JSON (`unknown` in TS): the UI knows
//! each field's type from `GET /api/schema` and sends the right JSON
//! shape, while `core`'s coercion remains the source of truth
//! (save-with-warning per ADR-001). `serde_yaml::Value` deserializes
//! straight from a JSON body — its `Deserialize` impl is deserializer-
//! agnostic — so no JSON→YAML conversion step is needed.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::model::views::View;
use crate::model::WorkItemId;
use crate::operations::add::AddOutcome;
use crate::operations::set::{BooleanMode, CollectionMode, SetOperation, SetOutcome};
use crate::operations::view_write::ViewWriteOutcome;
use crate::parser::views::view_to_value;
use crate::query::clause::{decompose_clauses, Clause};

/// A single field mutation as sent by the client, tagged by `op`.
///
/// Mirrors the field-type-independent subset of [`SetOperation`]:
/// `replace`, `unset`, `append`, `remove`, `toggle`. The type-aware
/// `delta` modes are intentionally CLI-only — the UI edits numbers,
/// durations, and dates by setting an absolute value (`replace`), so
/// the server never has to pick a delta variant from the field type.
#[derive(Debug, Clone, Deserialize, ts_rs::TS)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum FieldMutation {
    /// Set (or overwrite) the field's value. Valid for every field type.
    Replace {
        #[ts(type = "unknown")]
        value: serde_yaml::Value,
    },
    /// Clear the field entirely. Valid for every field type.
    Unset,
    /// Add one or more entries to a `list` / `links` / `multichoice` field.
    Append {
        #[ts(type = "Array<unknown>")]
        values: Vec<serde_yaml::Value>,
    },
    /// Remove every occurrence of each value from a collection field.
    Remove {
        #[ts(type = "Array<unknown>")]
        values: Vec<serde_yaml::Value>,
    },
    /// Flip a `boolean` field.
    Toggle,
}

impl FieldMutation {
    /// Map the wire request to the core [`SetOperation`]. Validity of the
    /// op against the field's type is enforced downstream by `run_set`
    /// (`SetError::ModeNotValidForFieldType`), not here — this is a pure
    /// structural translation.
    pub fn into_operation(self) -> SetOperation {
        match self {
            FieldMutation::Replace { value } => SetOperation::Replace(value),
            FieldMutation::Unset => SetOperation::Unset,
            FieldMutation::Append { values } => {
                SetOperation::Collection(CollectionMode::Append(values))
            }
            FieldMutation::Remove { values } => {
                SetOperation::Collection(CollectionMode::Remove(values))
            }
            FieldMutation::Toggle => SetOperation::Boolean(BooleanMode::Toggle),
        }
    }
}

/// The result of a successful field mutation — the projection of
/// [`SetOutcome`] the client receives in the envelope's `data`. Warnings
/// from the post-write reload ride in the envelope's `diagnostics`, not
/// here. `null` for `previous_value` means the field was absent before;
/// `null` for `new_value` means it was cleared.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct FieldMutationResult {
    pub id: WorkItemId,
    pub field: String,
    #[ts(type = "unknown")]
    pub previous_value: Option<serde_yaml::Value>,
    #[ts(type = "unknown")]
    pub new_value: Option<serde_yaml::Value>,
    /// `true` when this mutation introduced a diagnostic that wasn't
    /// present before — lets the UI emphasize "your change caused this"
    /// among the (always-complete) diagnostics list.
    pub mutation_caused_warning: bool,
    /// Operation-level notes that aren't problems (e.g. appending a value
    /// that was already present). Shown as informational feedback.
    pub info_messages: Vec<String>,
}

impl FieldMutationResult {
    pub fn from_outcome(id: WorkItemId, field: String, outcome: &SetOutcome) -> Self {
        Self {
            id,
            field,
            previous_value: outcome.previous_value.clone(),
            new_value: outcome.new_value.clone(),
            mutation_caused_warning: outcome.mutation_caused_warning,
            info_messages: outcome.info_messages.clone(),
        }
    }
}

/// A request to create a new work item. `core::run_add` derives the
/// slug/filename from an explicit `id` in `fields`, or — falling back —
/// from `title`; schema defaults fill any field the form left unset.
///
/// The UI's create form sends `title` (auto-slugged) plus whichever
/// fields it gathered, and may set an explicit `id` for the override
/// path. If neither `id` nor `title` is present, `run_add` returns
/// `MissingFilenameSource`.
#[derive(Debug, Clone, Deserialize, ts_rs::TS)]
pub struct CreateItem {
    #[ts(type = "Record<string, unknown>")]
    pub fields: HashMap<String, serde_yaml::Value>,
    /// Optional template to seed frontmatter and body from. Form values
    /// in `fields` override the template per-field.
    #[serde(default)]
    pub template: Option<String>,
}

/// The result of a successful create — the new item's id (so the UI can
/// navigate to it) and whether the create introduced a warning. Warnings
/// themselves ride in the envelope's `diagnostics`.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct CreateItemResult {
    pub id: WorkItemId,
    pub mutation_caused_warning: bool,
}

impl CreateItemResult {
    pub fn from_outcome(outcome: &AddOutcome) -> Self {
        Self {
            id: outcome.id.clone(),
            mutation_caused_warning: outcome.mutation_caused_warning,
        }
    }
}

/// A request to create a new view. `name` is a human label slugged to the
/// view's id server-side (the same rule work-item ids use). `definition` is
/// the flat view shape — `type`, optional `where`, and the type-specific
/// slots, **without** an `id` — the rest of one entry in `views.yaml`'s
/// `views:` list. It crosses the wire as opaque JSON (`Record<string,
/// unknown>` in TS) because the valid slots depend on the chosen `type`;
/// `core` validates it against the schema (see
/// [`crate::parser::views::view_from_value`]). A metric view's rows may
/// each carry a structured `filter` clause list in place of `where`
/// strings — serialized server-side exactly like the view-level filter.
#[derive(Debug, Clone, Deserialize, ts_rs::TS)]
pub struct CreateView {
    pub name: String,
    #[ts(type = "Record<string, unknown>")]
    pub definition: serde_yaml::Value,
    /// Optional filter to attach at creation, as structured clauses (same
    /// shape the filter editor uses). `core` serializes them into the
    /// view's `where:`. Omitted → no filter.
    #[serde(default)]
    pub filter: Vec<Clause>,
}

/// A request to replace an existing view's whole definition — the write
/// half of the edit form. `definition` and `filter` take the same shapes
/// [`CreateView`] does. `name`, when present, renames the view: it is
/// slugged to a new id server-side exactly like creation. Name → id is
/// lossy (only the id is persisted), so the form seeds its name field by
/// prettifying the id and omits `name` here unless the user actually
/// edited it — an untouched label can never cause an accidental rename.
#[derive(Debug, Clone, Deserialize, ts_rs::TS)]
pub struct UpdateView {
    #[serde(default)]
    pub name: Option<String>,
    #[ts(type = "Record<string, unknown>")]
    pub definition: serde_yaml::Value,
    #[serde(default)]
    pub filter: Vec<Clause>,
}

/// A persisted view decomposed for the edit form: the flat definition
/// (without `id` and `where`) plus the filter as structured clauses —
/// exactly the shape [`UpdateView`] takes back, so what the form GETs is
/// what it PUTs. A metric view's rows get the same treatment: each entry's
/// `where:` strings are replaced by a structured `filter` clause list.
/// Returned by `GET /api/views/{id}/definition`.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct ViewDefinition {
    #[ts(type = "Record<string, unknown>")]
    pub definition: serde_yaml::Value,
    pub filter: Vec<Clause>,
}

impl ViewDefinition {
    /// Decompose a persisted view. `id` leaves the mapping because the
    /// write path derives it (from the path, or a rename's `name`);
    /// `where` leaves — at the view level and per metric row — because
    /// filters travel as structured clauses.
    pub fn from_view(view: &View) -> Result<Self, serde_yaml::Error> {
        let mut value = view_to_value(view)?;
        if let serde_yaml::Value::Mapping(ref mut mapping) = value {
            mapping.shift_remove("id");
            mapping.shift_remove("where");
            crate::operations::view_write::metric_where_to_filters(mapping)?;
        }
        Ok(Self {
            definition: value,
            filter: decompose_clauses(&view.where_clauses),
        })
    }
}

/// A request to replace a view's `where:` filter. Each [`Clause`] is either
/// a guided condition or a raw passthrough string; `core` serializes them
/// to clause strings (so the UI never builds filter syntax) and stores them
/// verbatim. A clause that fails to parse or references an unknown field is
/// written and reported as a warning, not rejected.
#[derive(Debug, Clone, Deserialize, ts_rs::TS)]
pub struct SetViewFilter {
    pub clauses: Vec<Clause>,
}

/// The result of a successful view mutation (create, update, filter
/// change, delete) — the view's id (so the UI can navigate to / re-fetch
/// it; after a rename this is the *new* id) and whether the write
/// introduced a diagnostic. Warnings themselves ride in the envelope's
/// `diagnostics`; `info_messages` carries non-problem housekeeping notes
/// (e.g. a stale rendered file that was removed).
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct ViewMutationResult {
    pub view_id: String,
    pub mutation_caused_warning: bool,
    pub info_messages: Vec<String>,
}

impl ViewMutationResult {
    pub fn from_outcome(outcome: &ViewWriteOutcome) -> Self {
        Self {
            view_id: outcome.view_id.clone(),
            mutation_caused_warning: outcome.mutation_caused_warning,
            info_messages: outcome.info_messages.clone(),
        }
    }
}
