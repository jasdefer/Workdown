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

use crate::model::diagnostic::Diagnostic;
use crate::model::schema::{AggregateConfig, FieldType, Generator, Severity};
use crate::model::views::View;
use crate::model::WorkItemId;
use crate::operations::add::AddOutcome;
use crate::operations::schema_write::SchemaWriteOutcome;
use crate::operations::set::{BooleanMode, CollectionMode, SetOperation, SetOutcome};
use crate::operations::view_write::ViewWriteOutcome;
use crate::parser::views::view_to_value;
use crate::query::clause::{decompose_clauses, Clause};
use crate::rule_shape::RuleShape;
use crate::schema_definition_data::{FieldRecipe, FieldShape};

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

// ── Schema writes ─────────────────────────────────────────────────────
//
// The contracts of the schema editor's endpoints. The operation behind
// them is `crate::operations::schema_write`; like the view contracts
// above, the request and response shapes live here, beside the other
// wire types, and the operation takes and returns them.

/// One field definition as the editor writes it: the header every type
/// shares, the type-specific shape, the rollup and the recipe. The read
/// payload's
/// [`FieldDefinitionData`](crate::schema_definition_data::FieldDefinitionData)
/// minus its `name` (the operation takes it separately). What the form
/// GETs is what it PUTs back. The body of `PUT
/// /api/schema/fields/{name}`, and the `definition` of [`CreateField`]
/// and [`PreviewField`].
///
/// The `shape` is not checked against `field_type` here: the pair is
/// written to the tree as the properties it spells out, and the schema
/// parser is the judge of whether they fit the type, exactly as for a
/// hand-edited file.
#[derive(Debug, Clone, Deserialize, ts_rs::TS)]
pub struct FieldDefinitionWrite {
    /// The built-in type.
    pub field_type: FieldType,
    /// Whether every item must carry the field.
    #[serde(default)]
    pub required: bool,
    /// Human-readable explanation; `None` clears it.
    #[serde(default)]
    pub description: Option<String>,
    /// The `default:` applied at `workdown add` time; `None` clears it.
    /// Beside a `when` recipe it is the recipe's evaluated fallback
    /// instead, the same key in the file: a literal only, the parser
    /// refuses a generator there, which the preview reports.
    #[serde(default)]
    pub default: Option<DefaultWrite>,
    /// Resource section in `resources.yaml` that constrains the values;
    /// `None` clears it.
    #[serde(default)]
    pub resource: Option<String>,
    /// The type-specific properties.
    pub shape: FieldShape,
    /// The `aggregate:` block; `None` clears it. Written as sent and
    /// judged by the schema parser, which checks the function against
    /// the type and `over` against the link fields.
    #[serde(default)]
    pub aggregate: Option<AggregateConfig>,
    /// The recipe filling the value; `None` means set by hand and clears
    /// every recipe key. Written as sent; the parser judges the
    /// combination with `default` and the syntax, and the load the
    /// references and the types, which the preview reports as the
    /// draft's problem.
    #[serde(default)]
    pub recipe: Option<FieldRecipe>,
}

/// A field's `default:` as the editor sends it.
#[derive(Debug, Clone, Deserialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DefaultWrite {
    /// A `$`-generator, written as its token.
    Generator { generator: Generator },
    /// A literal, in the form the item editor's set endpoint sends a
    /// value (a JSON scalar; a duration as its shorthand text). Written
    /// to the file as it arrives and judged by the schema parser, which
    /// runs every literal default through the field's own coercion.
    Literal {
        #[ts(type = "unknown")]
        value: serde_yaml::Value,
    },
}

/// A request to add a field to `schema.yaml`: the frontmatter key it
/// will be stored under plus its definition, the same shape `PUT
/// /api/schema/fields/{name}` takes (there the name is the path).
/// Sent to `POST /api/schema/fields`.
#[derive(Debug, Clone, Deserialize, ts_rs::TS)]
pub struct CreateField {
    pub name: String,
    pub definition: FieldDefinitionWrite,
}

/// A request to rewrite the order of `fields:` in `schema.yaml`: every
/// current field name exactly once, in the wanted order. Sent to `PUT
/// /api/schema/field-order`.
#[derive(Debug, Clone, Deserialize, ts_rs::TS)]
pub struct ReorderFields {
    pub order: Vec<String>,
}

/// A request to preview a field without writing it. Sent to `POST
/// /api/schema/field-preview`.
#[derive(Debug, Clone, Deserialize, ts_rs::TS)]
pub struct PreviewField {
    /// The field's name: the one being edited, or the name the add
    /// would create.
    pub name: String,
    /// `true` when the draft replaces the field named `name`; `false`
    /// when it would be added, so a taken or invalid name is a problem.
    #[serde(default)]
    pub replacing: bool,
    pub definition: FieldDefinitionWrite,
}

/// What `POST /api/schema/field-preview` answers: the field's entry as
/// the write would put it in the file, and the verdict on the file
/// with it.
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
pub struct FieldPreview {
    /// The field's properties as YAML text, without the name key: the
    /// mapping an add would create or a replace would leave behind.
    pub yaml: String,
    /// Why the draft cannot be saved: a name the add would refuse, the
    /// parser's message when `schema.yaml` would not load with it, or
    /// the schema diagnostics it would introduce (a recipe the load
    /// disables), one per line. `None` when the write would go through,
    /// with or without warnings.
    pub problem: Option<String>,
    /// Where in the draft's formula the problem sits, as a one-based
    /// column, when it is a `compute` expression the parser or the type
    /// check rejects at a spot: the same column the text names, as
    /// data for the editor to mark. `None` for every other problem.
    pub problem_column: Option<usize>,
}

/// One rule as the editor writes it: the header plus the rows of
/// [`RuleShape`]. The read payload's
/// [`RuleData`](crate::schema_definition_data::RuleData) with its rows
/// in place of the text body. What the form GETs is what it PUTs back;
/// on a replace, `name` may differ from the name in the path, which
/// renames the rule. The body of `POST /api/schema/rules` and `PUT
/// /api/schema/rules/{name}`, and the `definition` of [`PreviewRule`].
#[derive(Debug, Clone, Deserialize, ts_rs::TS)]
pub struct RuleDefinitionWrite {
    /// Unique kebab-case identifier.
    pub name: String,
    /// Human-readable explanation; `None` clears it.
    #[serde(default)]
    pub description: Option<String>,
    /// Whether a violation is an error or a warning. Written only when
    /// it is not the default.
    #[serde(default)]
    pub severity: Severity,
    /// The `match`, `require` and `count` blocks as rows.
    pub shape: RuleShape,
}

/// A request to preview a rule without writing it. Sent to `POST
/// /api/schema/rule-preview`.
#[derive(Debug, Clone, Deserialize, ts_rs::TS)]
pub struct PreviewRule {
    /// The rule the draft would replace, when editing one; `None` for a
    /// draft that would be added. Decides which name a duplicate-name
    /// verdict ignores.
    #[serde(default)]
    pub replacing: Option<String>,
    pub definition: RuleDefinitionWrite,
}

/// What `POST /api/schema/rule-preview` answers: the rule's entry as
/// the write would put it in the file, and the parser's verdict on the
/// file with it.
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
pub struct RulePreview {
    /// The rule's mapping as YAML text, without the list dash. Empty
    /// when the rows cannot be written as one rule at all; `problem`
    /// says why.
    pub yaml: String,
    /// Why `schema.yaml` would not load with this rule in it, in the
    /// parser's words; `None` when it would.
    pub problem: Option<String>,
}

/// What depends on a field: the answer of `GET
/// /api/schema/fields/{name}/usage`
/// ([`crate::operations::schema_write::field_usage`]).
///
/// The schema's own references come by name — the rules and the
/// recipes naming the field — because a client shows them as a list to
/// edit first. Everything outside the schema file comes as the
/// diagnostics the removal would introduce, with their scope, which is
/// how a client tells a view from a config role from an item.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct FieldUsage {
    /// The rules whose `match`, `require` or `count` names the field,
    /// directly or through its inverse (`children.status` names
    /// `status`, and `parent` through `children`). In file order.
    pub rules: Vec<String>,
    /// The other fields whose `aggregate` or `pull` climbs or reads
    /// the field (`aggregate.over`, `pull.over`, `pull.field`). In
    /// declaration order. A `compute` expression reading the field is
    /// reported through `introduced` instead, as the warning it would
    /// become.
    pub recipes: Vec<String>,
    /// Set when removing the field would leave `schema.yaml` unable to
    /// load, with the parser's message. `rules` and `recipes` are the
    /// explanation when either is non-empty; the message is for a
    /// client to show when neither names anything. `introduced` is
    /// empty then: nothing further can be asked of a schema that does
    /// not parse.
    pub parse_error: Option<String>,
    /// The diagnostics removing the field would introduce, each with
    /// its scope: config findings for the views and the `config.yaml`
    /// roles naming the field and for recipes reading it, item findings
    /// for every item holding a value.
    pub introduced: Vec<Diagnostic>,
}

/// The result of a successful schema mutation (add, update, remove,
/// reorder a field; add, update, remove a rule) — the field or the rule
/// it concerned (both `null` for a reorder), whether the write
/// introduced a diagnostic, and the items a removal that dropped the
/// field's values rewrote (empty for every other write). Warnings
/// themselves ride in the envelope's `diagnostics`.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct SchemaMutationResult {
    pub field_name: Option<String>,
    pub rule_name: Option<String>,
    pub mutation_caused_warning: bool,
    pub rewritten_items: Vec<String>,
}

impl SchemaMutationResult {
    pub fn from_outcome(outcome: &SchemaWriteOutcome) -> Self {
        Self {
            field_name: outcome.field_name.clone(),
            rule_name: outcome.rule_name.clone(),
            mutation_caused_warning: outcome.mutation_caused_warning,
            rewritten_items: outcome.rewritten_items.clone(),
        }
    }
}
