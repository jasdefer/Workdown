//! `/api/git` — the git sync surface: status, pull, push.
//!
//! Only present in spirit when `serve.git_controls: true` is set in
//! `config.yaml`: with the flag off, status answers `disabled` (so the
//! UI knows to hide the widget) and the mutating endpoints refuse.
//!
//! Failure vocabulary (the tiers of ADR-013, extended for a surface
//! that talks to a network): `409` for refusals about repository state
//! (uncommitted changes, a rebase in progress, a rejected push or a
//! conflicting pull), `502` when the remote can't be reached by an
//! operation that needs it, `500` when git itself can't be run. A
//! *status* request degrades instead of failing when only the remote
//! is unreachable — the local numbers are still the truth, and the
//! `fetch_error` field says what the remote contact hit.

use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;

use workdown_core::git_data::{GitPullResult, GitPushResult, GitStatus};

use crate::envelope::ApiResponse;
use crate::git::{self, GitError, RepoSnapshot};
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
        .is_some_and(|serve| serve.git_controls)
}

/// The one mapping from "git couldn't answer" to a response — spawn
/// failures, timeouts, and refused plumbing all land here as a `500`
/// with the error's own one-liner.
fn git_failure<T: serde::Serialize>(error: GitError) -> ApiResponse<T> {
    ApiResponse::failed(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}

/// Refusal for a mutating endpoint called without the opt-in flag.
fn refuse_disabled<T: serde::Serialize>() -> ApiResponse<T> {
    ApiResponse::failed(
        StatusCode::NOT_FOUND,
        "git controls are not enabled — set 'serve.git_controls: true' in config.yaml and restart"
            .to_owned(),
    )
}

/// Same-origin check for anything with side effects beyond this
/// machine's repository reads. The server binds to 127.0.0.1, but any
/// website open in the same browser can still fire cross-origin
/// requests at localhost ports — a browser sends the page's `Origin`
/// on such requests, so a foreign one is refused outright. Non-browser
/// clients (curl, scripts) send no `Origin` and pass. Applied to the
/// POSTs and to `GET /api/git?fetch=true`, which contacts the remote
/// (and can invoke a credential helper) even though it is a read.
fn refuse_foreign_origin<T: serde::Serialize>(headers: &HeaderMap) -> Option<ApiResponse<T>> {
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

/// Lift a local snapshot into the wire status.
fn ready(snapshot: RepoSnapshot, fetch_error: Option<String>) -> GitStatus {
    GitStatus::Ready {
        branch: snapshot.branch,
        has_upstream: snapshot.has_upstream,
        ahead: snapshot.ahead,
        behind: snapshot.behind,
        dirty_count: snapshot.dirty_count,
        fetch_error,
    }
}

/// Re-read the status after a mutation. `NotARepo` mid-request means
/// the repository vanished under us — reported as-is rather than
/// guessed around.
async fn fresh_status(state: &AppState) -> Result<GitStatus, GitError> {
    Ok(match git::snapshot(&state.project_root).await? {
        Some(snapshot) => ready(snapshot, None),
        None => GitStatus::NotARepo,
    })
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
    headers: HeaderMap,
    Query(query): Query<StatusQuery>,
) -> ApiResponse<GitStatus> {
    if !git_controls_enabled(&state) {
        return ApiResponse::ok(GitStatus::Disabled);
    }
    if query.fetch {
        if let Some(refusal) = refuse_foreign_origin(&headers) {
            return refusal;
        }
    }
    match status_inner(&state, query.fetch).await {
        Ok(response) => response,
        Err(error) => git_failure(error),
    }
}

async fn status_inner(
    state: &AppState,
    with_fetch: bool,
) -> Result<ApiResponse<GitStatus>, GitError> {
    let root = &state.project_root;
    let Some(local) = git::snapshot(root).await? else {
        return Ok(ApiResponse::ok(GitStatus::NotARepo));
    };
    if !with_fetch {
        return Ok(ApiResponse::ok(ready(local, None)));
    }
    // The lock serializes everything that touches the repository or the
    // remote — a fetch racing a pull's own fetch loses on git's ref
    // locks and would surface as a spurious error.
    let _network = state.git_lock.lock().await;
    let fetch_error = match git::fetch(root).await {
        Ok(output) if output.success => None,
        // An unreachable remote degrades the answer instead of replacing
        // it: the local numbers are still the truth, `behind` is simply
        // as of the last successful fetch, and the field says why.
        Ok(output) => Some(output.stderr.trim().to_owned()),
        Err(error) => Some(error.to_string()),
    };
    Ok(ApiResponse::ok(match git::snapshot(root).await? {
        Some(snapshot) => ready(snapshot, fetch_error),
        None => GitStatus::NotARepo,
    }))
}

async fn git_pull(State(state): State<AppState>, headers: HeaderMap) -> ApiResponse<GitPullResult> {
    if !git_controls_enabled(&state) {
        return refuse_disabled();
    }
    if let Some(refusal) = refuse_foreign_origin(&headers) {
        return refusal;
    }
    let _network = state.git_lock.lock().await;
    match pull_inner(&state).await {
        Ok(response) => response,
        Err(error) => git_failure(error),
    }
}

async fn pull_inner(state: &AppState) -> Result<ApiResponse<GitPullResult>, GitError> {
    let root = &state.project_root;
    let Some(local) = git::snapshot(root).await? else {
        return Ok(ApiResponse::failed(
            StatusCode::CONFLICT,
            "the project is not inside a git repository".to_owned(),
        ));
    };
    // Refusals about repository state, checked before any network:
    // a rebase already underway is the *user's* (possibly mid-conflict
    // in a terminal) and must never be aborted from here; uncommitted
    // changes never get stashed or rebased over from a browser button.
    if git::rebase_in_progress(root).await? {
        return Ok(ApiResponse::failed(
            StatusCode::CONFLICT,
            "a rebase is in progress in this repository — finish or abort it in a terminal first"
                .to_owned(),
        ));
    }
    if local.dirty_count > 0 {
        return Ok(ApiResponse::failed(
            StatusCode::CONFLICT,
            "there are uncommitted changes — pull never touches uncommitted work; commit first"
                .to_owned(),
        ));
    }
    // Fetch first, then read `behind`: after the fetch it is exactly
    // the number of commits the pull will integrate — the same number
    // the pill showed. (Measuring the tracking ref's movement *during*
    // the pull instead would report "already up to date" whenever an
    // earlier status call had already fetched.)
    let fetched = git::fetch(root).await?;
    if !fetched.success {
        return Ok(ApiResponse::failed(
            StatusCode::BAD_GATEWAY,
            format!("fetch failed: {}", fetched.stderr.trim()),
        ));
    }
    let Some(current) = git::snapshot(root).await? else {
        return Ok(ApiResponse::failed(
            StatusCode::CONFLICT,
            "the project is not inside a git repository".to_owned(),
        ));
    };
    if current.behind == 0 {
        return Ok(ApiResponse::ok(GitPullResult {
            pulled_commits: 0,
            status: ready(current, None),
        }));
    }
    let pulled_commits = current.behind;
    let pulled = match git::pull(root).await {
        Ok(output) => output,
        Err(error) => {
            // A timeout kills git mid-operation and can leave its rebase
            // in progress. No rebase was underway before this request
            // (checked above), so an abort here only ever backs out ours.
            let _ = git::abort_rebase(root).await;
            return Err(error);
        }
    };
    if !pulled.success {
        // Same reasoning: this rebase is ours, back it out so the
        // browser never strands the repository mid-rebase.
        let _ = git::abort_rebase(root).await;
        return Ok(ApiResponse::failed(
            StatusCode::CONFLICT,
            format!("pull failed: {}", pulled.stderr.trim()),
        ));
    }
    // The changed files also wake the file watcher, whose ping makes
    // every open tab refetch its view — no manual event needed here.
    Ok(ApiResponse::ok(GitPullResult {
        pulled_commits,
        status: fresh_status(state).await?,
    }))
}

async fn git_push(State(state): State<AppState>, headers: HeaderMap) -> ApiResponse<GitPushResult> {
    if !git_controls_enabled(&state) {
        return refuse_disabled();
    }
    if let Some(refusal) = refuse_foreign_origin(&headers) {
        return refusal;
    }
    let _network = state.git_lock.lock().await;
    match push_inner(&state).await {
        Ok(response) => response,
        Err(error) => git_failure(error),
    }
}

/// Push, or — on a branch with no upstream — *publish*: the same
/// gesture from the user's side ("get my commits onto the remote"),
/// with git's first-time bookkeeping (create the remote branch, record
/// it as upstream) handled here instead of in a terminal. The server
/// decides which from the repository's present state, not from what
/// the pill believed when it was clicked.
async fn push_inner(state: &AppState) -> Result<ApiResponse<GitPushResult>, GitError> {
    let root = &state.project_root;
    let Some(local) = git::snapshot(root).await? else {
        return Ok(ApiResponse::failed(
            StatusCode::CONFLICT,
            "the project is not inside a git repository".to_owned(),
        ));
    };
    let published = !local.has_upstream;
    let pushed = if published {
        // Refusals about what there is to publish, before any network.
        if local.branch == "HEAD" {
            return Ok(ApiResponse::failed(
                StatusCode::CONFLICT,
                "detached HEAD — there is no branch to publish; check one out in a terminal first"
                    .to_owned(),
            ));
        }
        if !local.has_commits {
            return Ok(ApiResponse::failed(
                StatusCode::CONFLICT,
                "the branch has no commits yet — nothing to publish".to_owned(),
            ));
        }
        let Some(remote) = git::publish_remote(root).await? else {
            return Ok(ApiResponse::failed(
                StatusCode::CONFLICT,
                "no remote to publish to — add one (or set remote.pushDefault) in a terminal"
                    .to_owned(),
            ));
        };
        git::publish(root, &remote, &local.branch).await?
    } else {
        git::push(root).await?
    };
    if !pushed.success {
        return Ok(ApiResponse::failed(
            StatusCode::CONFLICT,
            format!(
                "{} failed: {}",
                if published { "publish" } else { "push" },
                pushed.stderr.trim()
            ),
        ));
    }
    Ok(ApiResponse::ok(GitPushResult {
        published,
        status: fresh_status(state).await?,
    }))
}
