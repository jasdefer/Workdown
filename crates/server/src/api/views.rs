//! Handlers for the `/api/views` family: listing, rendering, and the
//! full view-authoring lifecycle (create, edit, rename, delete, filter).
//!
//! The rendering handlers load the project per request (cold-load, no
//! cache) via `core::load_project`; the authoring seed handlers
//! (`/filter`, `/definition`) read `views.yaml` alone — see
//! `load_one_view` below. Failure mapping follows the three tiers from
//! ADR-013:
//!
//! - `Err(LoadError)` → 422 with the synthesized load diagnostic.
//! - Project loaded, view id not in `views.yaml` → 404 with empty body.
//! - Project loaded, the requested view has a `views_check` diagnostic
//!   pinned to it → 200 with empty `data` and the full diagnostic list
//!   (tier 2). The view can't render; the banner explains.
//! - Project loaded, view is valid → 200 with `ViewData` and the full
//!   project diagnostic list (tier 3). The UI groups primary/secondary.
//!
//! `GET /api/views/{id}` also accepts two optional, non-persisting
//! query params, both validated up front (malformed JSON → 422, even
//! on an unrenderable view): `?filter=` — a URL-encoded JSON array of
//! structured clauses for the filter editor's "for right now" preview,
//! extracted *instead of* the persisted `where:`, with diagnostics
//! computed as if the draft were saved so the preview's banner matches
//! what a save would produce — and `?display=` — a JSON object of
//! display roles applied with highest precedence (see `ViewQuery`).
//! The companion `GET /api/views/{id}/filter` returns the persisted
//! filter decomposed into the editor's clause shape, for seeding the
//! builder.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use workdown_core::model::diagnostic::Diagnostic;
use workdown_core::model::views::{DisplayConfig, View, ViewSummary, Views};
use workdown_core::mutation_data::{
    CreateView, SetViewFilter, UpdateView, ViewDefinition, ViewMutationResult,
};
use workdown_core::operations::view_write::{
    create_view, delete_view, set_view_filter, update_view, ViewWriteError, ViewWriteOutcome,
};
use workdown_core::parser::views::load_views;
use workdown_core::project::Project;
use workdown_core::query::clause::{clauses_to_strings, decompose_clauses, Clause};
use workdown_core::view_data::{self, CheckedView, ViewData};
use workdown_core::views_check;

use crate::envelope::ApiResponse;
use crate::state::{load_state_project, AppState};

/// Router for `/views`, `/views/{id}`, `/views/{id}/filter`, and
/// `/views/{id}/definition` under `/api`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/views", get(list_views).post(create_view_handler))
        .route(
            "/views/{id}",
            get(get_view)
                .patch(update_view_filter)
                .put(update_view_handler)
                .delete(delete_view_handler),
        )
        .route("/views/{id}/filter", get(get_view_filter))
        .route("/views/{id}/definition", get(get_view_definition))
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
    match load_state_project(&state) {
        Err(response) => response,
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
    let project = match load_state_project(&state) {
        Ok(project) => project,
        Err(response) => return response,
    };

    let views = match project.views.as_ref() {
        None => return ApiResponse::not_found(),
        Some(views) => views,
    };
    let view = match views.views.iter().find(|view| view.id == id) {
        None => return ApiResponse::not_found(),
        Some(view) => view,
    };

    // Parse the per-session display override up front, next to the
    // filter parse below — before the tier-2 unrenderable check — so a
    // malformed `?display=` is rejected with 422 exactly like a
    // malformed `?filter=`, whether or not the view itself can render.
    // It is *applied* only at tier 3, after validation.
    let display_override: Option<DisplayConfig> = match query.display.as_deref() {
        None => None,
        Some(display_json) => match serde_json::from_str(display_json) {
            Ok(override_config) => Some(override_config),
            Err(error) => {
                return ApiResponse::failed(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    format!("invalid display parameter: {error}"),
                )
            }
        },
    };

    // Preview path: render with an ad-hoc, non-persisted filter supplied
    // by the editor, instead of the view's saved `where:`. From here
    // both paths share the same tier logic.
    let (render_view, diagnostics) = match query.filter.as_deref() {
        None => (view.clone(), project.diagnostics.clone()),
        Some(filter_json) => {
            let effective = match effective_view(view, filter_json) {
                Ok(effective) => effective,
                Err(error) => {
                    return ApiResponse::failed(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        format!("invalid filter parameter: {error}"),
                    )
                }
            };
            let diagnostics = preview_diagnostics(&effective, views, &project, &state);
            (effective, diagnostics)
        }
    };

    // Display roles resolve here — after validation, so diagnostics keep
    // pointing at what views.yaml says: per-session override › view
    // `display:` › config defaults. Neither step can change the view's
    // id, which is what the tier-2 check below matches on.
    let mut render_view = render_view;
    if let Some(override_config) = display_override {
        render_view.display = override_config.or_inherit(&render_view.display);
    }
    let render_view = render_view.with_display_defaults(&state.config.defaults.display);

    // Tier 2: this specific view has a config *error* pinned to it (e.g.
    // references a missing field, gantt config conflict) — with the
    // effective filter in place. The view can't render; surface the
    // diagnostics instead of data.
    //
    // Severity is what separates the tiers, and `CheckedView` is where
    // that rule lives. A warning pinned to this view (a `where:` operand
    // that can never match, say) describes a view that renders perfectly
    // well; withholding the data over it would hide more than it
    // explains. Such findings ride along in the tier-3 response's
    // `diagnostics` instead.
    let Some(checked) = CheckedView::new(&render_view, &diagnostics) else {
        return ApiResponse::unrenderable(diagnostics);
    };

    // Tier 3: extract and return view data.
    let data = view_data::extract(checked, &project.store, &project.schema, &project.calendar);
    ApiResponse::ok_with(data, diagnostics)
}

/// The view to render, with the editor's draft `where:` in place of the
/// saved one.
///
/// Two ways to fail — unparseable JSON, and an operand that doesn't
/// match its operator's arity — reported as one message for the
/// caller to wrap. Neither is a filter that renders badly; both are
/// malformed requests, which is why the caller answers 422 and the
/// write path rejects them the same way.
fn effective_view(view: &View, filter_json: &str) -> Result<View, String> {
    let clauses: Vec<Clause> =
        serde_json::from_str(filter_json).map_err(|error| error.to_string())?;
    let where_clauses = clauses_to_strings(&clauses).map_err(|error| error.to_string())?;
    Ok(View {
        where_clauses,
        ..view.clone()
    })
}

/// The diagnostics a filter preview should report: what the project
/// would say if the draft filter were saved.
///
/// The whole views file is re-checked with this view's draft
/// substituted in, so stale findings about the persisted filter drop
/// out while findings about every other view stay (the "always show
/// all" convention). Every view-config diagnostic the real load
/// produced came from checking the *persisted* file, so all of them are
/// dropped and replaced by the candidate's. Nothing is written.
fn preview_diagnostics(
    effective: &View,
    views: &Views,
    project: &Project,
    state: &AppState,
) -> Vec<Diagnostic> {
    let candidate = Views {
        output_dir: views.output_dir.clone(),
        views: views
            .views
            .iter()
            .map(|existing| {
                if existing.id == effective.id {
                    effective.clone()
                } else {
                    existing.clone()
                }
            })
            .collect(),
    };

    let mut diagnostics: Vec<Diagnostic> = project
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.view_id().is_none())
        .cloned()
        .collect();
    diagnostics.extend(views_check::evaluate(
        &candidate,
        &project.schema,
        &project.resources,
        &project.store,
        &state.project_root.join(&state.config.paths.views),
    ));
    diagnostics
}

/// Load `views.yaml` alone and find one view by id — the authoring seed
/// endpoints' shared preamble. The rest of the project has no bearing on
/// what the file says: schema, work items, and rule evaluation matter for
/// rendering and for the write path's cross-file warnings, not for reading
/// a definition back — so a broken schema never blocks the editor, and the
/// seed stays cheap (no whole-store parse per form open).
///
/// A missing file means no views are configured (`404`, like an unknown
/// id); an existing file that won't load is `422` with its parse
/// diagnostics, mirroring the write path's `ExistingInvalid`.
fn load_one_view<T: serde::Serialize>(state: &AppState, id: &str) -> Result<View, ApiResponse<T>> {
    let views_path = state.project_root.join(&state.config.paths.views);
    if !views_path.exists() {
        return Err(ApiResponse::not_found());
    }
    let views = match load_views(&views_path) {
        Ok(views) => views,
        Err(error) => {
            return Err(ApiResponse::rejected(
                views_check::parse_errors_to_diagnostics(error, &views_path),
            ))
        }
    };
    views
        .views
        .into_iter()
        .find(|view| view.id == id)
        .ok_or_else(|| ApiResponse::not_found())
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
    match load_one_view(&state, &id) {
        Err(response) => response,
        Ok(view) => ApiResponse::ok(decompose_clauses(&view.where_clauses)),
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
    view_mutation_response(set_view_filter(
        &state.config,
        &state.project_root,
        &id,
        &request.clauses,
    ))
}

/// The shared outcome-to-envelope mapping for the view mutation handlers:
/// a success carries its save-with-warning diagnostics, a hard error maps
/// to its HTTP status. Create stays inline — it differs in returning `201`.
fn view_mutation_response(
    result: Result<ViewWriteOutcome, ViewWriteError>,
) -> ApiResponse<ViewMutationResult> {
    match result {
        Ok(outcome) => {
            let result = ViewMutationResult::from_outcome(&outcome);
            ApiResponse::ok_with(result, outcome.warnings)
        }
        Err(error) => ApiResponse::failed(view_write_error_status(&error), error.to_string()),
    }
}

/// `GET /api/views/{id}/definition` — the persisted view decomposed into
/// the edit form's seed: the flat definition (no `id`, no `where`, and a
/// metric view's rows carrying structured `filter` clauses) plus the
/// view-level filter as structured clauses. Exactly the `PUT` payload
/// shape, so what the form GETs is what it PUTs back.
///
/// Like `/filter`, independent of whether the view renders: a view with a
/// broken slot reference still returns its definition, so the editor can
/// always show and fix what's there.
async fn get_view_definition(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResponse<ViewDefinition> {
    let view = match load_one_view(&state, &id) {
        Err(response) => return response,
        Ok(view) => view,
    };

    match ViewDefinition::from_view(&view) {
        Ok(definition) => ApiResponse::ok(definition),
        // A view that loaded but won't re-serialize is a serializer bug,
        // not caller input — same class as `ProducedInvalid` on the write
        // path.
        Err(error) => ApiResponse::failed(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to serialize view definition: {error}"),
        ),
    }
}

/// `PUT /api/views/{id}` — replace the view's whole definition, and
/// rename it when the request carries a `name`. Save-with-warning applies
/// exactly as on create; the result's `view_id` is the id after the write
/// (the new one on a rename), so the UI navigates by it.
async fn update_view_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateView>,
) -> ApiResponse<ViewMutationResult> {
    view_mutation_response(update_view(
        &state.config,
        &state.project_root,
        &id,
        request.name.as_deref(),
        request.definition,
        &request.filter,
    ))
}

/// `DELETE /api/views/{id}` — remove the view from `views.yaml`, plus its
/// stale rendered output file when one exists. An unknown id is a `404`.
async fn delete_view_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResponse<ViewMutationResult> {
    view_mutation_response(delete_view(&state.config, &state.project_root, &id))
}

/// Map a hard [`ViewWriteError`] to its HTTP status. Save-with-warning
/// never reaches here — it's an `Ok` outcome.
///
/// - `404` — the view id in the path doesn't exist (filter change,
///   update, delete).
/// - `409` — creating a view whose id is already taken, or renaming one
///   onto an id that is.
/// - `422` — well-formed but unprocessable: the project's schema, work
///   items, or existing `views.yaml` won't load, the view definition is
///   invalid (missing/unknown slot), or a filter condition's operand doesn't
///   match its operator's arity.
/// - `500` — a server-side failure: serialization, a produced-invalid
///   invariant violation, or a write I/O error.
fn view_write_error_status(error: &ViewWriteError) -> StatusCode {
    match error {
        ViewWriteError::ViewNotFound { .. } => StatusCode::NOT_FOUND,

        ViewWriteError::DuplicateId { .. } => StatusCode::CONFLICT,

        ViewWriteError::SchemaLoad(_)
        | ViewWriteError::ItemsLoad { .. }
        | ViewWriteError::ExistingInvalid { .. }
        | ViewWriteError::InvalidDefinition { .. }
        | ViewWriteError::InvalidCondition(_)
        | ViewWriteError::InvalidName { .. } => StatusCode::UNPROCESSABLE_ENTITY,

        ViewWriteError::Serialize(_)
        | ViewWriteError::ProducedInvalid { .. }
        | ViewWriteError::WriteFile { .. } => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
