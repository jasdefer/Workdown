//! `GET /api/schema` — the project's editing vocabulary — and
//! `GET /api/schema/definition` — the schema editor's payload.
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
//! Failure mapping matches the view endpoints' tier 1 for both: if the
//! project (or, for `/definition`, the schema file) can't load, return
//! 422 with the load diagnostic. On success the response carries the
//! data and no diagnostics — project health surfaces on the views and
//! on mutations, not on these metadata fetches.

use axum::extract::State;
use axum::routing::get;
use axum::Router;

use workdown_core::schema_data::{self, SchemaData};
use workdown_core::schema_definition_data::{self, SchemaDefinitionData};

use crate::envelope::ApiResponse;
use crate::state::{load_state_project, AppState};

/// Router for `/schema` under `/api`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/schema", get(get_schema))
        .route("/schema/definition", get(get_schema_definition))
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
