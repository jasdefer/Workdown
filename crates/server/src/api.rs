//! HTTP API surface.
//!
//! This module is **wiring only**. Each resource gets its own child
//! module under `api/` (added when its first endpoint lands); this file
//! declares them and assembles the `/api/*` router. Today the router
//! is empty — handlers arrive in `first-view-end-to-end`.
//!
//! The flat-by-resource layout is deliberate: the view surface is one
//! generic `/api/views/:id` endpoint serving every view kind, so
//! feature-folder organization would either contain almost nothing or
//! fight the schema-driven view system. See the issue body for the
//! full rationale and planned resource files.

use axum::Router;

/// Build the `/api` router. Handlers nest under here once they exist.
pub fn router() -> Router {
    Router::new()
}
