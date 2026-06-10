//! HTTP API surface.
//!
//! This module is **wiring only**. Each resource gets its own child
//! module under `api/` (added when its first endpoint lands); this file
//! declares them and assembles the `/api/*` router.
//!
//! The flat-by-resource layout is deliberate: the view surface is one
//! generic `/api/views/:id` endpoint serving every view kind, so
//! feature-folder organization would either contain almost nothing or
//! fight the schema-driven view system. See the issue body for the
//! full rationale and planned resource files.

use axum::Router;

use crate::state::AppState;

pub mod items;
pub mod schema;
pub mod views;

/// Build the `/api` router. State-typed `Router<AppState>` so child
/// handlers can extract `State<AppState>` and call `core::load_project`.
pub fn router() -> Router<AppState> {
    Router::new()
        .merge(views::router())
        .merge(schema::router())
        .merge(items::router())
}
