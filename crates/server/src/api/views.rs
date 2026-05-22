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
use axum::routing::get;
use axum::Router;

use workdown_core::model::diagnostic::{Diagnostic, FileDiagnosticKind};
use workdown_core::model::schema::Severity;
use workdown_core::model::views::ViewSummary;
use workdown_core::project::{load_project, LoadError};
use workdown_core::view_data::{self, ViewData};

use crate::envelope::ApiResponse;
use crate::state::AppState;

/// Router for `/views` and `/views/{id}` under `/api`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/views", get(list_views))
        .route("/views/{id}", get(get_view))
}

async fn list_views(State(state): State<AppState>) -> ApiResponse<Vec<ViewSummary>> {
    match load_project(&state.config, &state.project_root) {
        Err(error) => ApiResponse::rejected(vec![load_error_to_diagnostic(&error)]),
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

async fn get_view(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResponse<ViewData> {
    let project = match load_project(&state.config, &state.project_root) {
        Err(error) => return ApiResponse::rejected(vec![load_error_to_diagnostic(&error)]),
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

/// Wrap a [`LoadError`] as a `File`-scope diagnostic with the failing
/// path as `source_path` and the underlying error message as `detail`.
/// Reuses the existing `ReadError` variant rather than inventing a new
/// kind for HTTP-side concerns.
fn load_error_to_diagnostic(error: &LoadError) -> Diagnostic {
    let (path, detail) = match error {
        LoadError::Schema { path, detail } | LoadError::Items { path, detail } => {
            (path.clone(), detail.clone())
        }
    };
    Diagnostic::file(
        Severity::Error,
        path,
        FileDiagnosticKind::ReadError { detail },
    )
}
