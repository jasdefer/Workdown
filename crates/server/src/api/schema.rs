//! `GET /api/schema` — the project's editing vocabulary.
//!
//! Cold-loads the project per request (same as the view endpoints) and
//! projects its [`Schema`](workdown_core::model::schema::Schema) plus the
//! item id index into [`SchemaData`]. The client fetches this once and
//! reuses it to render field editors (detail panel) and the create form.
//!
//! Failure mapping matches the view endpoints' tier 1: if the project
//! can't load, return 422 with the load diagnostic. On success the
//! response carries the schema data and no diagnostics — project health
//! surfaces on the views and on mutations, not on this metadata fetch.

use axum::extract::State;
use axum::routing::get;
use axum::Router;

use workdown_core::project::load_project;
use workdown_core::schema_data::{self, SchemaData};

use crate::envelope::ApiResponse;
use crate::state::AppState;

/// Router for `/schema` under `/api`.
pub fn router() -> Router<AppState> {
    Router::new().route("/schema", get(get_schema))
}

async fn get_schema(State(state): State<AppState>) -> ApiResponse<SchemaData> {
    match load_project(&state.config, &state.project_root) {
        Err(error) => ApiResponse::rejected(vec![error.to_diagnostic()]),
        Ok(project) => ApiResponse::ok(schema_data::build(
            &project.schema,
            &project.store,
            &project.resources,
        )),
    }
}
