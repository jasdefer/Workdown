//! `/api/git` — the git sync surface: status, pull, push.
//!
//! Only present in spirit when `serve.git_controls: true` is set in
//! `config.yaml`: with the flag off, status answers `disabled` (so the
//! UI knows to hide the widget) and the mutating endpoints refuse.

use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;

use workdown_core::git_data::{GitPullResult, GitStatus};

use crate::envelope::ApiResponse;
use crate::git;
use crate::state::AppState;

/// Router for the git endpoints under `/api`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/git", get(git_status))
        .route("/git/pull", post(git_pull))
        .route("/git/push", post(git_push))
}

/// Whether this project opted in to the git controls.
fn git_controls_enabled(state: &AppState) -> bool {
    state
        .config
        .serve
        .as_ref()
        .and_then(|serve| serve.git_controls)
        .unwrap_or(false)
}

/// Query for `GET /api/git`. `fetch=true` contacts the remote first so
/// `behind` is current; the default stays local-only (cheap enough to
/// call on every file-change ping).
#[derive(Debug, Deserialize)]
struct StatusQuery {
    #[serde(default)]
    fetch: bool,
}

async fn git_status(
    State(state): State<AppState>,
    Query(query): Query<StatusQuery>,
) -> ApiResponse<GitStatus> {
    if !git_controls_enabled(&state) {
        return ApiResponse::ok(GitStatus::Disabled);
    }
    match git::is_work_tree(&state.project_root).await {
        Ok(false) => return ApiResponse::ok(GitStatus::NotARepo),
        Err(error) => {
            return ApiResponse::failed(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
        }
        Ok(true) => {}
    }
    if query.fetch {
        match git::fetch(&state.project_root).await {
            Ok(output) if !output.success => {
                return ApiResponse::failed(
                    StatusCode::BAD_GATEWAY,
                    format!("fetch failed: {}", output.stderr.trim()),
                );
            }
            Err(error) => {
                return ApiResponse::failed(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
            }
            Ok(_) => {}
        }
    }
    match git::collect_status(&state.project_root).await {
        Ok(status) => ApiResponse::ok(status),
        Err(error) => ApiResponse::failed(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }
}

/// Refusals shared by the mutating endpoints, checked before any git
/// runs: the opt-in flag, and a same-origin check. The server binds to
/// 127.0.0.1, but any website open in the same browser can still fire
/// cross-origin POSTs at localhost ports — a browser sends the page's
/// `Origin` on such requests, so a foreign one is refused outright.
/// Non-browser clients (curl, scripts) send no `Origin` and pass.
fn refuse_mutation<T: serde::Serialize>(
    state: &AppState,
    headers: &HeaderMap,
) -> Option<ApiResponse<T>> {
    if !git_controls_enabled(state) {
        return Some(ApiResponse::failed(
            StatusCode::NOT_FOUND,
            "git controls are not enabled — set `serve.git_controls: true` in config.yaml and restart".to_owned(),
        ));
    }
    let foreign = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|origin| {
            !matches!(origin_host(origin), Some("127.0.0.1" | "localhost" | "::1"))
        });
    if foreign {
        return Some(ApiResponse::failed(
            StatusCode::FORBIDDEN,
            "cross-origin request refused".to_owned(),
        ));
    }
    None
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

async fn git_push(State(state): State<AppState>, headers: HeaderMap) -> ApiResponse<GitStatus> {
    if let Some(refusal) = refuse_mutation(&state, &headers) {
        return refusal;
    }
    let _network = state.git_lock.lock().await;
    let pushed = match git::push(&state.project_root).await {
        Ok(output) => output,
        Err(error) => {
            return ApiResponse::failed(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
        }
    };
    if !pushed.success {
        return ApiResponse::failed(
            StatusCode::CONFLICT,
            format!("push failed: {}", pushed.stderr.trim()),
        );
    }
    match git::collect_status(&state.project_root).await {
        Ok(status) => ApiResponse::ok(status),
        Err(error) => ApiResponse::failed(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }
}

async fn git_pull(State(state): State<AppState>, headers: HeaderMap) -> ApiResponse<GitPullResult> {
    if let Some(refusal) = refuse_mutation(&state, &headers) {
        return refusal;
    }
    let _network = state.git_lock.lock().await;
    // Where the upstream stood before — afterwards, what it gained is
    // exactly what the pull brought in ("already up to date" when
    // nothing). Measured on the tracking ref, not HEAD: a rebasing pull
    // rewrites local commits, which are not "pulled".
    let upstream_before = match git::upstream_commit(&state.project_root).await {
        Ok(upstream) => upstream,
        Err(error) => {
            return ApiResponse::failed(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
        }
    };
    let pulled = match git::pull(&state.project_root).await {
        Ok(output) => output,
        Err(error) => {
            return ApiResponse::failed(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
        }
    };
    if !pulled.success {
        // A conflicting rebase is left in progress by `git pull`; back
        // out so the browser never strands the repo mid-rebase. Best
        // effort — when the pull failed before rebasing (network down),
        // the abort refuses and that's fine.
        let _ = git::abort_rebase(&state.project_root).await;
        return ApiResponse::failed(
            StatusCode::CONFLICT,
            format!("pull failed: {}", pulled.stderr.trim()),
        );
    }
    let pulled_commits =
        match git::upstream_commits_since(&state.project_root, upstream_before.as_deref()).await {
            Ok(count) => count,
            Err(error) => {
                return ApiResponse::failed(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
            }
        };
    // The changed files also wake the file watcher, whose ping makes
    // every open tab refetch its view — no manual event needed here.
    match git::collect_status(&state.project_root).await {
        Ok(status) => ApiResponse::ok(GitPullResult {
            pulled_commits,
            status,
        }),
        Err(error) => ApiResponse::failed(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }
}
