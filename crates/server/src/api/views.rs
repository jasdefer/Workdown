//! `GET /api/views` and `GET /api/views/:id` handlers.
//!
//! Both load the project per request (cold-load, no cache) via
//! `core::load_project`. Failure mapping follows the three tiers from
//! the `first-view-end-to-end` decisions:
//!
//! - `Err(LoadError)` → 422 with the synthesized load diagnostic.
//! - Project loaded, view id not in `views.yaml` → 404 with empty body.
//! - Project loaded, the requested view has a `views_check` diagnostic
//!   pinned to it → 200 with empty `data` and the full diagnostic list
//!   (tier 2). The view can't render; the banner explains.
//! - Project loaded, view is valid → 200 with `ViewData` and the full
//!   project diagnostic list (tier 3). The UI groups primary/secondary.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};

use workdown_core::model::views::ViewSummary;
use workdown_core::mutation_data::{CreateView, SetViewFilter, ViewMutationResult};
use workdown_core::operations::view_write::{add_view, set_view_filter, ViewWriteError};
use workdown_core::project::load_project;
use workdown_core::view_data::{self, ViewData};

use crate::envelope::ApiResponse;
use crate::state::AppState;

/// Router for `/views` and `/views/{id}` under `/api`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/views", get(list_views).post(create_view))
        .route("/views/{id}", get(get_view).patch(update_view_filter))
}

async fn list_views(State(state): State<AppState>) -> ApiResponse<Vec<ViewSummary>> {
    match load_project(&state.config, &state.project_root) {
        Err(error) => ApiResponse::rejected(vec![error.to_diagnostic()]),
        Ok(project) => {
            let summaries: Vec<ViewSummary> = project
                .views
                .as_ref()
                .map(|views| views.views.iter().map(|view| view.summary()).collect())
                .unwrap_or_default();
            ApiResponse::ok_with(summaries, project.diagnostics)
        }
    }
}

async fn get_view(State(state): State<AppState>, Path(id): Path<String>) -> ApiResponse<ViewData> {
    let project = match load_project(&state.config, &state.project_root) {
        Err(error) => return ApiResponse::rejected(vec![error.to_diagnostic()]),
        Ok(project) => project,
    };

    let view = match project
        .views
        .as_ref()
        .and_then(|views| views.views.iter().find(|view| view.id == id))
    {
        None => return ApiResponse::not_found(),
        Some(view) => view,
    };

    // Tier 2: this specific view has a config diagnostic pinned to it
    // (e.g. references a missing field, gantt config conflict). The
    // view can't render; surface the diagnostics instead of data.
    let has_view_config_issue = project
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.view_id() == Some(view.id.as_str()));
    if has_view_config_issue {
        return ApiResponse::unrenderable(project.diagnostics);
    }

    // Tier 3: extract and return view data.
    let data = view_data::extract(view, &project.store, &project.schema, &project.calendar);
    ApiResponse::ok_with(data, project.diagnostics)
}

/// `POST /api/views` — create a new view and persist it to `views.yaml`.
///
/// The created view is a normal `views.yaml` entry. Like every mutation,
/// the file is the source of truth and nothing is committed automatically.
/// Save-with-warning applies: a view that persists but fails cross-file
/// validation returns `201` with the problem in `diagnostics`; only a
/// write that would make the file unloadable is a hard failure.
async fn create_view(
    State(state): State<AppState>,
    Json(request): Json<CreateView>,
) -> ApiResponse<ViewMutationResult> {
    match add_view(&state.config, &state.project_root, request.definition) {
        Ok(outcome) => {
            let result = ViewMutationResult::from_outcome(&outcome);
            ApiResponse::created(result, outcome.warnings)
        }
        Err(error) => ApiResponse::failed(view_write_error_status(&error), error.to_string()),
    }
}

/// `PATCH /api/views/{id}` — replace a view's `where:` filter.
///
/// The milestone's scope: this adjusts the filter only, not the view's
/// kind or other slots. A `200` carries any save-with-warning diagnostics;
/// an unknown id is a `404`.
async fn update_view_filter(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<SetViewFilter>,
) -> ApiResponse<ViewMutationResult> {
    match set_view_filter(
        &state.config,
        &state.project_root,
        &id,
        request.where_clauses,
    ) {
        Ok(outcome) => {
            let result = ViewMutationResult::from_outcome(&outcome);
            ApiResponse::ok_with(result, outcome.warnings)
        }
        Err(error) => ApiResponse::failed(view_write_error_status(&error), error.to_string()),
    }
}

/// Map a hard [`ViewWriteError`] to its HTTP status. Save-with-warning
/// never reaches here — it's an `Ok` outcome.
///
/// - `404` — the view id in the path doesn't exist (filter change).
/// - `409` — creating a view whose id is already taken.
/// - `422` — well-formed but unprocessable: the project's schema or the
///   existing `views.yaml` won't load, or the view definition is invalid
///   (missing/unknown slot).
/// - `500` — a server-side failure: serialization, a produced-invalid
///   invariant violation, or a write I/O error.
fn view_write_error_status(error: &ViewWriteError) -> StatusCode {
    match error {
        ViewWriteError::ViewNotFound { .. } => StatusCode::NOT_FOUND,

        ViewWriteError::DuplicateId { .. } => StatusCode::CONFLICT,

        ViewWriteError::SchemaLoad(_)
        | ViewWriteError::ExistingInvalid { .. }
        | ViewWriteError::InvalidDefinition { .. } => StatusCode::UNPROCESSABLE_ENTITY,

        ViewWriteError::Serialize(_)
        | ViewWriteError::ProducedInvalid { .. }
        | ViewWriteError::WriteFile { .. } => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
