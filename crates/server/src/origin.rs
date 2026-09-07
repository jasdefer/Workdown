//! Same-origin guard for the API.
//!
//! The server binds to 127.0.0.1, but any website open in the same
//! browser can still fire requests at localhost ports. For requests
//! that carry JSON the browser asks the server for permission first
//! (a CORS preflight) and, getting none, never sends them; for
//! "ordinary" requests — a bodiless POST such as `/api/timer/stop` — it
//! does not ask, it sends. The browser attaches the page's `Origin` to
//! every such cross-site request, so refusing a foreign `Origin` closes
//! the gap. Non-browser clients (curl, scripts) send no `Origin` and
//! pass.
//!
//! Applied as one layer over the whole `/api` router for every method
//! that mutates, so a future endpoint is covered the day it lands
//! rather than when someone remembers. Two *reads* need the guard as
//! well and check it themselves: `GET /api/git?fetch=true` contacts the
//! remote and may invoke a credential helper, and
//! `GET /api/git/commit-preview` returns file contents.

use axum::extract::Request;
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::envelope::ApiResponse;

/// Whether the request came from a browser page that is not ours: an
/// `Origin` header naming anything but localhost.
pub fn is_foreign_origin(headers: &HeaderMap) -> bool {
    headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|origin| {
            !matches!(origin_host(origin), Some("127.0.0.1" | "localhost" | "::1"))
        })
}

/// The refusal every guarded surface answers with.
pub fn refusal<T: serde::Serialize>() -> ApiResponse<T> {
    ApiResponse::failed(
        StatusCode::FORBIDDEN,
        "cross-origin request refused".to_owned(),
    )
}

/// Middleware: refuse every mutating request (anything but GET, HEAD
/// and OPTIONS) that carries a foreign `Origin`. Reads pass untouched
/// — the browser already withholds their responses from a foreign page.
pub async fn guard_mutations(request: Request, next: Next) -> Response {
    let safe = matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    );
    if !safe && is_foreign_origin(request.headers()) {
        return refusal::<()>().into_response();
    }
    next.run(request).await
}

/// The host part of an `Origin` header value (`scheme://host[:port]`).
fn origin_host(origin: &str) -> Option<&str> {
    let rest = origin.split_once("//")?.1;
    if let Some(bracketed) = rest.strip_prefix('[') {
        // IPv6 literal: `[::1]:3141`.
        return bracketed.split(']').next();
    }
    rest.split(':').next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(origin: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(origin) = origin {
            headers.insert(header::ORIGIN, HeaderValue::from_str(origin).unwrap());
        }
        headers
    }

    #[test]
    fn localhost_origins_and_no_origin_pass() {
        assert!(!is_foreign_origin(&headers(None)));
        assert!(!is_foreign_origin(&headers(Some("http://localhost:3141"))));
        assert!(!is_foreign_origin(&headers(Some("http://127.0.0.1:3141"))));
        assert!(!is_foreign_origin(&headers(Some("http://[::1]:3141"))));
    }

    #[test]
    fn foreign_origins_are_refused() {
        assert!(is_foreign_origin(&headers(Some("https://evil.example"))));
        assert!(is_foreign_origin(&headers(Some(
            "http://localhost.evil.example"
        ))));
        assert!(is_foreign_origin(&headers(Some("null"))));
    }
}
