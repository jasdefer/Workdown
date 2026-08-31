//! `GET /api/project` — the project's identity: name and description.
//!
//! The one endpoint that answers from [`AppState::config`] alone instead
//! of loading the project per request. That is the point: the web shell
//! needs the project's name to title the browser tab, and it should keep
//! that name while a broken schema or an unparseable item has every
//! other endpoint answering `422`. Config is read once at server start,
//! so an edited `config.yaml` reaches the tab only after a restart
//! (ADR-013) — the same asymmetry the config-hot-reload work will lift.

use axum::extract::State;
use axum::routing::get;
use axum::Router;

use workdown_core::project_data::{self, ProjectIdentity};

use crate::envelope::ApiResponse;
use crate::state::AppState;

/// Router for `/project` under `/api`.
pub fn router() -> Router<AppState> {
    Router::new().route("/project", get(get_project))
}

async fn get_project(State(state): State<AppState>) -> ApiResponse<ProjectIdentity> {
    ApiResponse::ok(project_data::build(&state.config.project))
}
