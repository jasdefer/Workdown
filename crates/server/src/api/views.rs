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
//!
//! `GET /api/views/{id}` also accepts an optional `?filter=` param — a
//! URL-encoded JSON array of structured clauses — for the filter editor's
//! "for right now" preview: the view is extracted using those clauses
//! *instead of* the persisted `where:`, without writing anything. Its
//! diagnostics are computed as if the draft were saved, so the preview's
//! banner matches what a save would produce. The
//! companion `GET /api/views/{id}/filter` returns the persisted filter
//! decomposed into the editor's clause shape, for seeding the builder.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use workdown_core::model::diagnostic::Diagnostic;
use workdown_core::model::views::{DisplayConfig, View, ViewSummary, Views};
use workdown_core::mutation_data::{CreateView, SetViewFilter, ViewMutationResult};
use workdown_core::operations::view_write::{create_view, set_view_filter, ViewWriteError};
use workdown_core::project::load_project;
use workdown_core::query::clause::{clauses_to_strings, decompose_clauses, Clause};
use workdown_core::view_data::{self, ViewData};
use workdown_core::views_check;

use crate::envelope::ApiResponse;
use crate::state::AppState;

/// Router for `/views`, `/views/{id}`, and `/views/{id}/filter` under `/api`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/views", get(list_views).post(create_view_handler))
        .route("/views/{id}", get(get_view).patch(update_view_filter))
        .route("/views/{id}/filter", get(get_view_filter))
}

/// Query string for `GET /api/views/{id}`.
#[derive(Deserialize)]
struct ViewQuery {
    /// URL-encoded JSON array of structured clauses for an ad-hoc,
    /// non-persisted preview. Absent → render with the persisted filter.
    filter: Option<String>,
    /// URL-encoded JSON object of display roles (`title`, `subtitle`,
    /// `fields`, `color`) for a per-session override. Set roles take
    /// highest precedence — over the view's `display:` block and the
    /// config defaults; unset roles inherit as usual. `color` accepts a
    /// field name or the sentinel `"none"` (no tint); a stale name (the
    /// field was deleted or retyped since the override was saved) is
    /// skipped at extraction time, never an error. Nothing is persisted.
    display: Option<String>,
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

async fn get_view(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<ViewQuery>,
) -> ApiResponse<ViewData> {
    let project = match load_project(&state.config, &state.project_root) {
        Err(error) => return ApiResponse::rejected(vec![error.to_diagnostic()]),
        Ok(project) => project,
    };

    let views = match project.views.as_ref() {
        None => return ApiResponse::not_found(),
        Some(views) => views,
    };
    let view = match views.views.iter().find(|view| view.id == id) {
        None => return ApiResponse::not_found(),
        Some(view) => view,
    };

    // Preview path: render with an ad-hoc, non-persisted filter supplied
    // by the editor, instead of the view's saved `where:`. The diagnostics
    // are recomputed as if the draft were saved: the whole views file is
    // re-checked with this view's filter substituted, so stale findings
    // about the persisted filter drop out while findings about other views
    // stay (the "always show all" convention) — and nothing is written.
    // From here both paths share the same tier logic.
    let (render_view, diagnostics) = if let Some(filter_json) = query.filter.as_deref() {
        let clauses: Vec<Clause> = match serde_json::from_str(filter_json) {
            Ok(clauses) => clauses,
            Err(error) => {
                return ApiResponse::failed(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    format!("invalid filter parameter: {error}"),
                )
            }
        };
        let effective = View {
            where_clauses: clauses_to_strings(&clauses),
            ..view.clone()
        };
        let views_path = state.project_root.join(&state.config.paths.views);
        let candidate = Views {
            output_dir: views.output_dir.clone(),
            views: views
                .views
                .iter()
                .map(|existing| {
                    if existing.id == view.id {
                        effective.clone()
                    } else {
                        existing.clone()
                    }
                })
                .collect(),
        };
        // Every view-config diagnostic in `project.diagnostics` came from
        // checking the *persisted* file; replace them all with the
        // candidate's (which re-derives the other views' findings too).
        let mut diagnostics: Vec<Diagnostic> = project
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.view_id().is_none())
            .cloned()
            .collect();
        diagnostics.extend(views_check::evaluate(
            &candidate,
            &project.schema,
            &views_path,
        ));
        (effective, diagnostics)
    } else {
        (view.clone(), project.diagnostics.clone())
    };

    // Tier 2: this specific view has a config diagnostic pinned to it
    // (e.g. references a missing field, gantt config conflict) — with the
    // effective filter in place. The view can't render; surface the
    // diagnostics instead of data.
    let has_view_config_issue = diagnostics
        .iter()
        .any(|diagnostic| diagnostic.view_id() == Some(view.id.as_str()));
    if has_view_config_issue {
        return ApiResponse::unrenderable(diagnostics);
    }

    // Tier 3: extract and return view data. Display roles resolve here —
    // after validation, so diagnostics keep pointing at what views.yaml
    // says: per-session override › view `display:` › config defaults.
    let mut render_view = render_view;
    if let Some(display_json) = query.display.as_deref() {
        let override_config: DisplayConfig = match serde_json::from_str(display_json) {
            Ok(config) => config,
            Err(error) => {
                return ApiResponse::failed(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    format!("invalid display parameter: {error}"),
                )
            }
        };
        render_view.display = override_config.or_inherit(&render_view.display);
    }
    let render_view = render_view.with_display_defaults(&state.config.defaults.display);
    let data = view_data::extract(
        &render_view,
        &project.store,
        &project.schema,
        &project.calendar,
    );
    ApiResponse::ok_with(data, diagnostics)
}

/// `GET /api/views/{id}/filter` — the view's persisted `where:` decomposed
/// into the editor's clause shape, for seeding the filter builder.
///
/// Independent of whether the view renders: a view with a broken filter
/// still returns its clauses (unparseable ones come back as `Raw`), so the
/// editor can always show and fix what's there.
async fn get_view_filter(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResponse<Vec<Clause>> {
    let project = match load_project(&state.config, &state.project_root) {
        Err(error) => return ApiResponse::rejected(vec![error.to_diagnostic()]),
        Ok(project) => project,
    };

    match project
        .views
        .as_ref()
        .and_then(|views| views.views.iter().find(|view| view.id == id))
    {
        None => ApiResponse::not_found(),
        Some(view) => ApiResponse::ok(decompose_clauses(&view.where_clauses)),
    }
}

/// `POST /api/views` — create a new view and persist it to `views.yaml`.
///
/// The created view is a normal `views.yaml` entry. Like every mutation,
/// the file is the source of truth and nothing is committed automatically.
/// Save-with-warning applies: a view that persists but fails cross-file
/// validation returns `201` with the problem in `diagnostics`; only a
/// write that would make the file unloadable is a hard failure.
async fn create_view_handler(
    State(state): State<AppState>,
    Json(request): Json<CreateView>,
) -> ApiResponse<ViewMutationResult> {
    match create_view(
        &state.config,
        &state.project_root,
        &request.name,
        request.definition,
        &request.filter,
    ) {
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
    match set_view_filter(&state.config, &state.project_root, &id, &request.clauses) {
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
        | ViewWriteError::InvalidDefinition { .. }
        | ViewWriteError::InvalidName { .. } => StatusCode::UNPROCESSABLE_ENTITY,

        ViewWriteError::Serialize(_)
        | ViewWriteError::ProducedInvalid { .. }
        | ViewWriteError::WriteFile { .. } => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
