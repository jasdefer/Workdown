//! `GET /api/schema` — the project's editing vocabulary — and the
//! `/api/schema/…` family behind the schema editor: `GET
//! /api/schema/definition` (its payload), the field writes under
//! `/api/schema/fields`, and the per-field usage question.
//!
//! `/schema` cold-loads the project per request (same as the view
//! endpoints) and projects its
//! [`Schema`](workdown_core::model::schema::Schema) plus the item id
//! index into [`SchemaData`]. The client fetches this once and reuses it
//! to render field editors (detail panel) and the create form.
//!
//! `/schema/definition` reads `schema.yaml` alone — no items, resources
//! or views — and serves every field's full definition, the rules, the
//! type system's tables and a content hash of the file
//! ([`SchemaDefinitionData`]). Only the `/schema` page fetches it, so
//! `/schema` stays as small as the item editors need. Reading one file
//! is the second exemption from the load-everything rule (the first is
//! `GET /api/project`): the payload needs nothing else, and the page
//! then works while the items directory is broken.
//!
//! The writes — `POST /schema/fields`, `PUT` and `DELETE
//! /schema/fields/{name}`, `PUT /schema/field-order`, and for rules
//! `POST /schema/rules`, `PUT` and `DELETE /schema/rules/{name}` — are
//! thin wrappers over `core::operations::schema_write`, which owns every
//! refusal; this module only maps each to a status (see
//! `schema_write_error_status`). `POST /schema/rule-preview` is the
//! rule write without the write: the YAML a draft would become and the
//! parser's verdict, for the editor's preview pane.
//! Save-with-warning applies as for items and views: a write that loads
//! but fails a cross-file check is written and answers `200`/`201` with
//! the findings in `diagnostics`; only a write that would leave
//! `schema.yaml` unable to load is refused. The reorder route is
//! `/schema/field-order` rather than `/schema/fields/order` so that a
//! field named `order` stays addressable.
//!
//! `GET /schema/fields/{name}/usage` answers what depends on a field —
//! computed by simulating the removal, see
//! [`workdown_core::operations::schema_write::field_usage`].
//! It is its own address so `/schema/definition` stays schema-file-only;
//! the editor asks it when a field's panel opens.
//!
//! Failure mapping matches the view endpoints' tier 1 for the reads: if
//! the project (or, for `/definition`, the schema file) can't load,
//! return 422 with the load diagnostic. On success the response carries
//! the data and no diagnostics — project health surfaces on the views
//! and on mutations, not on these metadata fetches.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;

use workdown_core::mutation_data::{CreateField, PreviewRule, ReorderFields, SchemaMutationResult};
use workdown_core::operations::schema_write::{
    add_field, add_rule, field_usage, preview_rule, remove_field, remove_rule, reorder_fields,
    update_field, update_rule, FieldDefinitionWrite, FieldUsage, RemovedValues,
    RuleDefinitionWrite, RulePreview, SchemaWriteError, SchemaWriteOutcome,
};
use workdown_core::schema_data::{self, SchemaData};
use workdown_core::schema_definition_data::{self, SchemaDefinitionData};

use crate::envelope::ApiResponse;
use crate::state::{load_state_project, AppState};

/// Router for `/schema`, `/schema/definition`, `/schema/fields`,
/// `/schema/fields/{name}`, `/schema/fields/{name}/usage`,
/// `/schema/field-order`, `/schema/rules`, `/schema/rules/{name}` and
/// `/schema/rule-preview` under `/api`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/schema", get(get_schema))
        .route("/schema/definition", get(get_schema_definition))
        .route("/schema/fields", post(create_field_handler))
        .route(
            "/schema/fields/{name}",
            put(update_field_handler).delete(delete_field_handler),
        )
        .route("/schema/fields/{name}/usage", get(get_field_usage))
        .route("/schema/field-order", put(reorder_fields_handler))
        .route("/schema/rules", post(create_rule_handler))
        .route(
            "/schema/rules/{name}",
            put(update_rule_handler).delete(delete_rule_handler),
        )
        .route("/schema/rule-preview", post(preview_rule_handler))
}

async fn get_schema_definition(State(state): State<AppState>) -> ApiResponse<SchemaDefinitionData> {
    let schema_path = state.project_root.join(&state.config.schema);
    match schema_definition_data::load(&schema_path) {
        Ok(data) => ApiResponse::ok(data),
        Err(error) => ApiResponse::rejected(vec![error.to_diagnostic()]),
    }
}

async fn get_schema(State(state): State<AppState>) -> ApiResponse<SchemaData> {
    match load_state_project(&state) {
        Err(response) => response,
        Ok(project) => ApiResponse::ok(schema_data::build(
            &project.schema,
            &project.store,
            &project.resources,
        )),
    }
}

/// `POST /api/schema/fields` — add a field at the end of `fields:`.
/// `201` with the field's name; `409` when the name is taken.
async fn create_field_handler(
    State(state): State<AppState>,
    Json(request): Json<CreateField>,
) -> ApiResponse<SchemaMutationResult> {
    match add_field(
        &state.config,
        &state.project_root,
        &state.config_path,
        &request.name,
        &request.definition,
    ) {
        Ok(outcome) => {
            let result = SchemaMutationResult::from_outcome(&outcome);
            ApiResponse::created(result, outcome.warnings)
        }
        Err(error) => ApiResponse::failed(schema_write_error_status(&error), error.to_string()),
    }
}

/// `PUT /api/schema/fields/{name}` — replace the field's plain
/// properties, keeping its position and its recipe. An unknown name is
/// a `404`; a type change the widening table does not allow is a `422`.
async fn update_field_handler(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(definition): Json<FieldDefinitionWrite>,
) -> ApiResponse<SchemaMutationResult> {
    schema_mutation_response(update_field(
        &state.config,
        &state.project_root,
        &state.config_path,
        &name,
        &definition,
    ))
}

/// Query string for `DELETE /api/schema/fields/{name}`.
#[derive(Deserialize)]
struct DeleteFieldQuery {
    /// `true` also rewrites every item holding the key, dropping it.
    /// Absent or `false`, the items keep the key and their unknown-field
    /// warning rides back in `diagnostics`.
    #[serde(default)]
    drop_values: bool,
}

/// `DELETE /api/schema/fields/{name}` — remove the field; with
/// `?drop_values=true` also its values from every item holding one,
/// named in the result's `rewritten_items`.
async fn delete_field_handler(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<DeleteFieldQuery>,
) -> ApiResponse<SchemaMutationResult> {
    let removed_values = if query.drop_values {
        RemovedValues::Drop
    } else {
        RemovedValues::Keep
    };
    schema_mutation_response(remove_field(
        &state.config,
        &state.project_root,
        &state.config_path,
        &name,
        removed_values,
    ))
}

/// `PUT /api/schema/field-order` — rewrite the order of `fields:` from
/// the full name list. A list that is not a permutation of the current
/// names is a `422`.
async fn reorder_fields_handler(
    State(state): State<AppState>,
    Json(request): Json<ReorderFields>,
) -> ApiResponse<SchemaMutationResult> {
    schema_mutation_response(reorder_fields(
        &state.config,
        &state.project_root,
        &state.config_path,
        &request.order,
    ))
}

/// `GET /api/schema/fields/{name}/usage` — what depends on the field:
/// the parse error its removal would cause, or the diagnostics it would
/// introduce. `200` with the answer; an unknown name is a `404`.
async fn get_field_usage(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResponse<FieldUsage> {
    match field_usage(
        &state.config,
        &state.project_root,
        &state.config_path,
        &name,
    ) {
        Ok(usage) => ApiResponse::ok(usage),
        Err(error) => ApiResponse::failed(schema_write_error_status(&error), error.to_string()),
    }
}

/// `POST /api/schema/rules` — add a rule at the end of `rules:`. `201`
/// with the rule's name; `409` when the name is taken.
async fn create_rule_handler(
    State(state): State<AppState>,
    Json(definition): Json<RuleDefinitionWrite>,
) -> ApiResponse<SchemaMutationResult> {
    match add_rule(
        &state.config,
        &state.project_root,
        &state.config_path,
        &definition,
    ) {
        Ok(outcome) => {
            let result = SchemaMutationResult::from_outcome(&outcome);
            ApiResponse::created(result, outcome.warnings)
        }
        Err(error) => ApiResponse::failed(schema_write_error_status(&error), error.to_string()),
    }
}

/// `PUT /api/schema/rules/{name}` — replace the rule, keeping its
/// position; a different name in the body renames it. An unknown name
/// is a `404`; a new name another rule has is a `409`.
async fn update_rule_handler(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(definition): Json<RuleDefinitionWrite>,
) -> ApiResponse<SchemaMutationResult> {
    schema_mutation_response(update_rule(
        &state.config,
        &state.project_root,
        &state.config_path,
        &name,
        &definition,
    ))
}

/// `DELETE /api/schema/rules/{name}` — remove the rule. An unknown name
/// is a `404`.
async fn delete_rule_handler(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResponse<SchemaMutationResult> {
    schema_mutation_response(remove_rule(
        &state.config,
        &state.project_root,
        &state.config_path,
        &name,
    ))
}

/// `POST /api/schema/rule-preview` — the YAML a rule draft would be
/// written as, and the parser's verdict on the schema with it, without
/// writing. `200` whatever the draft's problems, which ride in the
/// answer; `422` only when the existing `schema.yaml` does not load.
/// Its own address, like `/schema/field-order`, so that a rule named
/// `preview` stays addressable under `/schema/rules/{name}`.
async fn preview_rule_handler(
    State(state): State<AppState>,
    Json(request): Json<PreviewRule>,
) -> ApiResponse<RulePreview> {
    match preview_rule(
        &state.config,
        &state.project_root,
        request.replacing.as_deref(),
        &request.definition,
    ) {
        Ok(preview) => ApiResponse::ok(preview),
        Err(error) => ApiResponse::failed(schema_write_error_status(&error), error.to_string()),
    }
}

/// The shared outcome-to-envelope mapping for the schema mutation
/// handlers: a success carries its save-with-warning diagnostics, a hard
/// error maps to its HTTP status. Create stays inline — it differs in
/// returning `201`.
fn schema_mutation_response(
    result: Result<SchemaWriteOutcome, SchemaWriteError>,
) -> ApiResponse<SchemaMutationResult> {
    match result {
        Ok(outcome) => {
            let result = SchemaMutationResult::from_outcome(&outcome);
            ApiResponse::ok_with(result, outcome.warnings)
        }
        Err(error) => ApiResponse::failed(schema_write_error_status(&error), error.to_string()),
    }
}

/// Map a hard [`SchemaWriteError`] to its HTTP status. Save-with-warning
/// never reaches here — it's an `Ok` outcome.
///
/// - `404` — the field or rule name in the path doesn't exist (update,
///   delete, usage).
/// - `409` — adding a field or a rule whose name is already taken, or
///   renaming a rule onto another's name.
/// - `422` — well-formed but unprocessable: the existing `schema.yaml` or
///   the work items won't load, the candidate would not load, the name
///   is invalid, the `id` field or a field a config role names is being
///   removed, the type change is not a widening, or the order is not a
///   permutation of the current names.
/// - `500` — a server-side failure: serialization or a write I/O error.
fn schema_write_error_status(error: &SchemaWriteError) -> StatusCode {
    match error {
        SchemaWriteError::FieldNotFound { .. } | SchemaWriteError::RuleNotFound { .. } => {
            StatusCode::NOT_FOUND
        }

        SchemaWriteError::DuplicateName { .. } | SchemaWriteError::DuplicateRuleName { .. } => {
            StatusCode::CONFLICT
        }

        SchemaWriteError::ExistingInvalid { .. }
        | SchemaWriteError::ProjectLoad(_)
        | SchemaWriteError::InvalidName { .. }
        | SchemaWriteError::IdNotRemovable
        | SchemaWriteError::NamedByConfig { .. }
        | SchemaWriteError::RecipeTypeLocked { .. }
        | SchemaWriteError::TypeChangeNotAllowed { .. }
        | SchemaWriteError::InvalidOrder { .. }
        | SchemaWriteError::InvalidRuleName { .. }
        | SchemaWriteError::InvalidRule { .. }
        | SchemaWriteError::Unloadable { .. } => StatusCode::UNPROCESSABLE_ENTITY,

        SchemaWriteError::Serialize(_) | SchemaWriteError::WriteFile { .. } => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}
