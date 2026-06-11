//! Workdown HTTP server.
//!
//! Library-shaped: exposes `router`, `bind`, and `serve` for the CLI to
//! wire together. No `main`, no flag parsing, no browser-launching —
//! those concerns live in `workdown-cli`.
//!
//! The router has two layers: the `/api/*` tree (built in
//! [`api::router`]) and a fallback that serves the embedded UI bundle
//! (SPA shell). The API tree is nested under `/api`; everything else
//! falls through to the UI assets.

pub mod api;
pub mod envelope;
pub mod state;
pub mod watcher;

use std::net::SocketAddr;

use anyhow::Result;
use axum::{
    body::Body,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
    Router,
};
use rust_embed::{Embed, EmbeddedFile};
use tokio::net::TcpListener;
use tower_http::catch_panic::CatchPanicLayer;

pub use state::AppState;

/// Embedded UI bundle.
///
/// Release builds bake `ui/dist/` into the binary. Debug builds (with
/// the default `debug-embed = false`) read the same path from disk at
/// runtime, so `cargo run -- serve` picks up fresh bundles without
/// re-linking — provided `ui/dist/` has been populated by `npm run
/// build` (or `cargo xtask build-ui`).
///
/// Under `cargo test`, the folder switches to a committed test fixture
/// so the routing logic can be exercised without the real UI bundle
/// being built first. Keeps plain `cargo test` pure-Rust and free of
/// the Node toolchain.
#[cfg(not(test))]
#[derive(Embed)]
#[folder = "../../ui/dist/"]
struct UiAssets;

#[cfg(test)]
#[derive(Embed)]
#[folder = "tests/fixtures/dist/"]
struct UiAssets;

/// Build the axum router for the workdown UI.
///
/// Routes match in this order:
/// 1. `/api/*` — handled by [`api::router`], state-bound for handlers
///    that need to load the project.
/// 2. Anything else — the SPA fallback (`asset_handler`) serves
///    embedded UI assets, returning `index.html` for unknown paths so
///    the client-side router can resolve them.
pub fn router(state: AppState) -> Router {
    Router::new()
        .nest("/api", api::router())
        .fallback(asset_handler)
        // Convert any handler panic into a 500 instead of dropping the
        // connection. view_data::extract panics on invariants the
        // validator is expected to have caught; this is the safety net.
        .layer(CatchPanicLayer::new())
        .with_state(state)
}

/// Bind a TCP listener on `127.0.0.1:port`. Returns the listener; the
/// caller can read `listener.local_addr()` for the actual bound port
/// (relevant when the caller is implementing scan-on-conflict).
pub async fn bind(port: u16) -> Result<TcpListener> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr).await?;
    Ok(listener)
}

/// Run the axum serve loop on the given listener until it shuts down.
pub async fn serve(listener: TcpListener, router: Router) -> Result<()> {
    axum::serve(listener, router).await?;
    Ok(())
}

// ── Internals ─────────────────────────────────────────────────────────

async fn asset_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    if !path.is_empty() {
        if let Some(file) = UiAssets::get(path) {
            return serve_file(path, file);
        }
        // Path didn't resolve. If it looks like a real asset (has an
        // extension other than `.html`), return 404 rather than masking
        // the miss with the SPA shell — otherwise debugging "why isn't
        // my asset loading" is awful.
        if let Some(extension) = std::path::Path::new(path).extension() {
            if !extension.eq_ignore_ascii_case("html") {
                return StatusCode::NOT_FOUND.into_response();
            }
        }
    }

    // Otherwise: SPA fallback. Unknown route → serve `index.html` and
    // let the client-side router resolve it.
    match UiAssets::get("index.html") {
        Some(file) => serve_file("index.html", file),
        None => (
            StatusCode::NOT_FOUND,
            "UI bundle missing: run `cargo xtask build-ui` (or `npm run build` in `ui/`) and retry.",
        )
            .into_response(),
    }
}

fn serve_file(path: &str, file: EmbeddedFile) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime.as_ref())
        .body(Body::from(file.data.into_owned()))
        .expect("response builder accepts valid headers")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;
    use tower::ServiceExt;

    /// Minimal `AppState` for asset-handler tests — these tests don't
    /// exercise any handler that uses the project loader, so the paths
    /// don't have to resolve to a real project.
    fn test_state() -> AppState {
        AppState::test_stub()
    }

    async fn body_bytes(response: Response) -> Vec<u8> {
        axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec()
    }

    #[tokio::test]
    async fn serves_index_at_root() {
        let app = router(test_state());
        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let ctype = response.headers().get(header::CONTENT_TYPE).cloned();
        let body = body_bytes(response).await;
        assert_eq!(ctype.unwrap(), "text/html");
        assert!(String::from_utf8_lossy(&body).contains("<!doctype html>"));
    }

    #[tokio::test]
    async fn unknown_route_falls_through_to_index() {
        let app = router(test_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/some/client-side/route")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let ctype = response.headers().get(header::CONTENT_TYPE).cloned();
        assert_eq!(ctype.unwrap(), "text/html");
    }

    #[tokio::test]
    async fn missing_asset_with_extension_returns_404() {
        let app = router(test_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/_app/does-not-exist.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn robots_txt_is_served() {
        let app = router(test_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/robots.txt")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let ctype = response.headers().get(header::CONTENT_TYPE).cloned();
        let body = body_bytes(response).await;
        assert_eq!(ctype.unwrap(), "text/plain");
        assert!(String::from_utf8_lossy(&body).contains("Disallow: /"));
    }
}
