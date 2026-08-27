//! HTTP API surface.
//!
//! This module is **wiring only**. Each resource gets its own child
//! module under `api/` (added when its first endpoint lands); this file
//! declares them and assembles the `/api/*` router.
//!
//! The flat-by-resource layout is deliberate: the view surface is one
//! generic `/api/views/:id` endpoint serving every view kind, so
//! feature-folder organization would either contain almost nothing or
//! fight the schema-driven view system.

use axum::Router;

use crate::state::AppState;

pub mod events;
pub mod git;
pub mod items;
pub mod schema;
pub mod timer;
pub mod views;

/// Build the `/api` router. State-typed `Router<AppState>` so child
/// handlers can extract `State<AppState>` and call `core::load_project`.
pub fn router() -> Router<AppState> {
    Router::new()
        .merge(views::router())
        .merge(schema::router())
        .merge(items::router())
        .merge(events::router())
        .merge(timer::router())
        .merge(git::router())
}
