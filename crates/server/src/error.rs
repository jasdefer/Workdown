//! Server-wide error mapping.
//!
//! Catches anything that escapes a handler — panics, unexpected
//! `Result::Err` bubbling up the tower stack — and converts it into the
//! standard envelope shape so the UI never sees a free-form text body.
//!
//! Today this is a thin shell; richer mapping lands as soon as the
//! first real handler reports something other than success. The point
//! of having it from the start is that every endpoint can rely on the
//! envelope contract without each handler having to think about how
//! errors leave the building.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

/// Catch-all 500 response body shape.
///
/// Mirrors the [`ApiResponse`](crate::envelope::ApiResponse) wire shape
/// (`data` omitted, empty `diagnostics`) so the UI can decode every
/// response — even panics — with the same parser.
#[derive(Serialize)]
struct ServerErrorBody {
    diagnostics: [(); 0],
}

/// Generic 500-internal-server-error response. Diagnostics array is
/// empty today; a structured `ServerPanic` diagnostic variant can be
/// added once the diagnostic taxonomy needs it.
pub fn internal_server_error() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ServerErrorBody { diagnostics: [] }),
    )
        .into_response()
}
