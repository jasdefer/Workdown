//! Uniform response envelope shared across every API endpoint.
//!
//! Shape:
//!
//! ```json
//! { "data": <T>, "diagnostics": [<Diagnostic>...] }
//! ```
//!
//! - `data` is omitted (not `null`) when the response carries no
//!   payload — an outright rejected write, or a delete that succeeded.
//!   Absent-vs-null disambiguates "rejected" from "explicit null".
//! - `diagnostics` is **always present**, often `[]`. The UI never
//!   needs to optional-chain it: `response.diagnostics.length` always
//!   works. Same shape as `workdown check --json` produces.
//!
//! HTTP status answers "did the thing happen?"; `diagnostics` answers
//! "what should the user know?". See the issue body for the full
//! status-code table — `200` for success (warnings still possible),
//! `422` for well-formed-but-rejected with diagnostics explaining why,
//! `400` for malformed request shapes, `404` for unknown routes/IDs,
//! `500` for panics.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use workdown_core::model::diagnostic::Diagnostic;

/// HTTP response envelope.
///
/// `data` is `Option<T>` so the `skip_serializing_if` attribute can
/// omit the field entirely on `None`. `diagnostics` is always serialized.
#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    pub diagnostics: Vec<Diagnostic>,
    #[serde(skip)]
    pub status: StatusCode,
}

impl<T: Serialize> ApiResponse<T> {
    /// 200 OK with a payload. Diagnostics may still carry warnings.
    pub fn ok(data: T) -> Self {
        Self {
            data: Some(data),
            diagnostics: Vec::new(),
            status: StatusCode::OK,
        }
    }

    /// 200 OK with a payload plus diagnostics (typically warnings).
    pub fn ok_with(data: T, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            data: Some(data),
            diagnostics,
            status: StatusCode::OK,
        }
    }

    /// 422 Unprocessable Entity — request well-formed but rejected.
    /// Caller passes the diagnostics that explain why.
    pub fn rejected(diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            data: None,
            diagnostics,
            status: StatusCode::UNPROCESSABLE_ENTITY,
        }
    }
}

impl<T: Serialize> IntoResponse for ApiResponse<T> {
    fn into_response(self) -> Response {
        (self.status, Json(EnvelopeBody::from(&self))).into_response()
    }
}

/// Wire shape — what actually serializes. Decouples the public
/// `ApiResponse` (which carries the status out-of-band) from the JSON
/// body the client receives.
#[derive(Serialize)]
struct EnvelopeBody<'a, T: Serialize> {
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<&'a T>,
    diagnostics: &'a [Diagnostic],
}

impl<'a, T: Serialize> From<&'a ApiResponse<T>> for EnvelopeBody<'a, T> {
    fn from(response: &'a ApiResponse<T>) -> Self {
        Self {
            data: response.data.as_ref(),
            diagnostics: &response.diagnostics,
        }
    }
}
