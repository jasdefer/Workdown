//! `POST /api/items/:id/fields/:field` — mutate a single field.
//!
//! The field name is part of the resource path; the JSON body is the
//! operation alone (`{ "op": "replace", "value": ... }`), which maps to
//! a core [`SetOperation`](workdown_core::operations::set::SetOperation)
//! via [`FieldMutation::into_operation`]. The handler is a thin wrapper
//! over `core::operations::set::run_set` — the same code path the CLI's
//! `workdown set` uses.
//!
//! Outcome mapping:
//!
//! - `Ok(outcome)` → `200 OK`. The mutation happened. Any post-write
//!   reload diagnostics (the save-with-warning warnings) ride in the
//!   envelope's `diagnostics`; the new/previous values come back in
//!   `data`. Per ADR-001 a schema violation still *saves* — it is not a
//!   failure here.
//! - `Err(SetError)` → a hard failure mapped to a status (see
//!   [`set_error_status`]). Nothing was written; the message goes in the
//!   envelope's `error`.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};

use workdown_core::item_data::{self, ItemDetail};
use workdown_core::model::WorkItemId;
use workdown_core::mutation_data::{
    CreateItem, CreateItemResult, FieldMutation, FieldMutationResult,
};
use workdown_core::operations::add::{run_add, AddError};
use workdown_core::operations::set::{run_set, SetError};
use workdown_core::project::load_project;

use crate::envelope::ApiResponse;
use crate::state::AppState;

/// Router for the item endpoints under `/api`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/items", post(create_item))
        .route("/items/{id}", get(get_item))
        .route("/items/{id}/fields/{field}", post(set_field))
}

/// Read one item's current field values and body — the data source for
/// the editing surface (detail panel and standalone item page).
async fn get_item(State(state): State<AppState>, Path(id): Path<String>) -> ApiResponse<ItemDetail> {
    let project = match load_project(&state.config, &state.project_root) {
        Err(error) => return ApiResponse::rejected(vec![error.to_diagnostic()]),
        Ok(project) => project,
    };

    match project.store.get(&id) {
        Some(item) => ApiResponse::ok(item_data::build(item, &project.schema)),
        None => ApiResponse::not_found(),
    }
}

async fn create_item(
    State(state): State<AppState>,
    Json(request): Json<CreateItem>,
) -> ApiResponse<CreateItemResult> {
    match run_add(
        &state.config,
        &state.project_root,
        request.fields,
        request.template.as_deref(),
    ) {
        Ok(outcome) => {
            let result = CreateItemResult::from_outcome(&outcome);
            ApiResponse::created(result, outcome.warnings)
        }
        Err(error) => ApiResponse::failed(add_error_status(&error), error.to_string()),
    }
}

/// Map a hard [`AddError`] to its HTTP status.
///
/// - `409` — the id (explicit or slugged from the title) already exists.
/// - `422` — well-formed but unprocessable: no naming source, an invalid
///   slug/id, an unknown template, or the project won't load.
/// - `500` — a server-side I/O failure writing the new file.
fn add_error_status(error: &AddError) -> StatusCode {
    match error {
        AddError::AlreadyExists { .. } => StatusCode::CONFLICT,

        AddError::SchemaLoad(_)
        | AddError::StoreLoad(_)
        | AddError::MissingFilenameSource
        | AddError::InvalidSlug { .. }
        | AddError::InvalidId { .. }
        | AddError::Template(_) => StatusCode::UNPROCESSABLE_ENTITY,

        AddError::WriteFile { .. } => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn set_field(
    State(state): State<AppState>,
    Path((id, field)): Path<(String, String)>,
    Json(mutation): Json<FieldMutation>,
) -> ApiResponse<FieldMutationResult> {
    let work_item_id = WorkItemId::from(id);
    let operation = mutation.into_operation();

    match run_set(
        &state.config,
        &state.project_root,
        &work_item_id,
        &field,
        operation,
    ) {
        Ok(outcome) => {
            let result = FieldMutationResult::from_outcome(work_item_id, field, &outcome);
            ApiResponse::ok_with(result, outcome.warnings)
        }
        Err(error) => ApiResponse::failed(set_error_status(&error), error.to_string()),
    }
}

/// Map a hard [`SetError`] to its HTTP status. Save-with-warning never
/// reaches here — it's an `Ok` outcome. This is the failure taxonomy:
///
/// - `404` — the item id in the path doesn't exist.
/// - `422` — well-formed but unprocessable: the field doesn't exist, the
///   op is invalid for the field's type, `id` isn't mutable, the current
///   value can't support the op, or the project/target won't parse.
/// - `500` — a genuine server-side I/O failure reading or writing.
fn set_error_status(error: &SetError) -> StatusCode {
    match error {
        SetError::UnknownItem { .. } => StatusCode::NOT_FOUND,

        SetError::SchemaLoad(_)
        | SetError::StoreLoad(_)
        | SetError::UnknownField { .. }
        | SetError::IdNotMutable
        | SetError::ModeNotValidForFieldType { .. }
        | SetError::MutationRequiresExistingValue { .. }
        | SetError::MutationCurrentValueMalformed { .. }
        | SetError::ParseTarget { .. } => StatusCode::UNPROCESSABLE_ENTITY,

        SetError::ReadTarget { .. } | SetError::WriteFile { .. } => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}
