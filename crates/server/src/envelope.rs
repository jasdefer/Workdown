//! Uniform response envelope shared across every API endpoint.
//!
//! Shape:
//!
//! ```json
//! { "data": <T>, "diagnostics": [<Diagnostic>...], "error": "<string>" }
//! ```
//!
//! - `data` is omitted (not `null`) when the response carries no
//!   payload — an outright rejected write, or a delete that succeeded.
//!   Absent-vs-null disambiguates "rejected" from "explicit null".
//! - `diagnostics` is **always present**, often `[]`. The UI never
//!   needs to optional-chain it: `response.diagnostics.length` always
//!   works. Same shape as `workdown check --json` produces. These are
//!   *project-validation findings* — including the save-with-warning
//!   warnings a successful mutation returns on a `200`.
//! - `error` is omitted unless a *hard operational failure* occurred —
//!   the request was understood but couldn't be carried out (unknown
//!   item, an op invalid for the field's type, a write I/O error) and
//!   nothing was written. One human-readable line. Kept distinct from
//!   `diagnostics` because a request-level failure is not a project
//!   finding and doesn't fit the structured per-item diagnostic model.
//!
//! HTTP status answers "did the thing happen?"; `diagnostics` answers
//! "what should the user know about the project?"; `error` answers "why
//! did this request fail?". `200` for success (warnings still possible),
//! `422` for well-formed-but-rejected, `404` for unknown routes/IDs,
//! `500` for I/O failures and panics.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use workdown_core::model::diagnostic::Diagnostic;

/// HTTP response envelope.
///
/// `data` is `Option<T>` so the `skip_serializing_if` attribute can
/// omit the field entirely on `None`. `diagnostics` is always serialized
/// — except when `omit_body` is set, which sends no body at all (404).
#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    pub diagnostics: Vec<Diagnostic>,
    /// A single human-readable line for a hard operational failure;
    /// omitted (not `null`) on success. See the module docs for how this
    /// differs from `diagnostics`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip)]
    pub status: StatusCode,
    /// When `true`, the response sends just the status code with no
    /// body — not even `{ diagnostics: [] }`. Used for 404 (unknown
    /// view id) where the empty body is intentional and a synthesized
    /// "you asked for the wrong URL" diagnostic would dilute the
    /// diagnostic vocabulary.
    #[serde(skip)]
    pub omit_body: bool,
}

impl<T: Serialize> ApiResponse<T> {
    /// 200 OK with a payload. Diagnostics may still carry warnings.
    pub fn ok(data: T) -> Self {
        Self {
            data: Some(data),
            diagnostics: Vec::new(),
            error: None,
            status: StatusCode::OK,
            omit_body: false,
        }
    }

    /// 200 OK with a payload plus diagnostics (typically warnings).
    pub fn ok_with(data: T, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            data: Some(data),
            diagnostics,
            error: None,
            status: StatusCode::OK,
            omit_body: false,
        }
    }

    /// 201 Created with the new resource's payload, plus any
    /// save-with-warning diagnostics. Used by item creation — the
    /// resource came into existence, so the status differs from a plain
    /// `200` field mutation.
    pub fn created(data: T, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            data: Some(data),
            diagnostics,
            error: None,
            status: StatusCode::CREATED,
            omit_body: false,
        }
    }

    /// 200 OK with empty data plus diagnostics — tier 2 of the failure
    /// model: the request was understood, the project loaded, but the
    /// requested view can't render because its own config is broken.
    /// The accompanying diagnostics explain what's wrong with the view.
    pub fn unrenderable(diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            data: None,
            diagnostics,
            error: None,
            status: StatusCode::OK,
            omit_body: false,
        }
    }

    /// 422 Unprocessable Entity — tier 1 of the failure model: the
    /// project itself can't be loaded (missing schema, unparseable
    /// views.yaml). Caller passes the diagnostics that explain why.
    /// Body present (with diagnostics, no `data`) so the UI can render
    /// the full diagnostic list on the error page.
    pub fn rejected(diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            data: None,
            diagnostics,
            error: None,
            status: StatusCode::UNPROCESSABLE_ENTITY,
            omit_body: false,
        }
    }

    /// A hard operational failure: the request was understood but the
    /// action couldn't be carried out and nothing was written. `error`
    /// carries one human-readable line; `data` and `diagnostics` are
    /// empty. The caller picks the status (`404` unknown item, `422`
    /// semantic rejection, `500` I/O failure).
    pub fn failed(status: StatusCode, error: String) -> Self {
        Self {
            data: None,
            diagnostics: Vec::new(),
            error: Some(error),
            status,
            omit_body: false,
        }
    }

    /// 404 Not Found with no body — the URL doesn't resolve to anything
    /// the API knows about (typically: unknown view id). The UI builds
    /// any friendly "did you mean…" surface from layout-loaded data;
    /// the server doesn't synthesize a diagnostic for a routing miss.
    pub fn not_found() -> Self {
        Self {
            data: None,
            diagnostics: Vec::new(),
            error: None,
            status: StatusCode::NOT_FOUND,
            omit_body: true,
        }
    }
}

impl<T: Serialize> IntoResponse for ApiResponse<T> {
    fn into_response(self) -> Response {
        if self.omit_body {
            return self.status.into_response();
        }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a str>,
}

impl<'a, T: Serialize> From<&'a ApiResponse<T>> for EnvelopeBody<'a, T> {
    fn from(response: &'a ApiResponse<T>) -> Self {
        Self {
            data: response.data.as_ref(),
            diagnostics: &response.diagnostics,
            error: response.error.as_deref(),
        }
    }
}
