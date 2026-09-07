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

use workdown_core::change_summary::{summarize_changes, ChangedFile, ChangedFileKind};
use workdown_core::git_data::{
    GitChangeKind, GitChangedFile, GitCommitPreview, GitPullResult, GitPushResult, GitStatus,
};
use workdown_core::model::schema::Schema;

use crate::envelope::ApiResponse;
use crate::git::{self, ChangeState, ChangedPath, GitError, RepoSnapshot};
use crate::git_scope::{GitScope, ScopeRole};
use crate::state::{load_state_project, AppState};

/// Router for the git endpoints under `/api`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/git", get(git_status))
        .route("/git/pull", post(git_pull))
        .route("/git/push", post(git_push))
        .route("/git/commit-preview", get(git_commit_preview))
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

/// Everything a status answer is built from: the whole-repository
/// snapshot, the git-controls scope, and the uncommitted changes inside
/// that scope.
struct LocalState {
    snapshot: RepoSnapshot,
    scope: GitScope,
    /// The dirty files the commit button would commit — git's own
    /// answer for the scope's pathspecs, not a filter over `snapshot`.
    in_scope: Vec<ChangedPath>,
}

impl LocalState {
    /// Lift into the wire status. The dirty numbers come from the
    /// in-scope changes only, counted by role: work items as a number,
    /// definition files by name.
    fn into_status(self, fetch_error: Option<String>) -> GitStatus {
        let roles: Vec<ScopeRole> = self
            .in_scope
            .iter()
            .filter_map(|change| self.scope.classify(&change.path))
            .collect();
        let dirty_items = roles
            .iter()
            .filter(|role| **role == ScopeRole::WorkItems)
            .count() as u32;
        let dirty_definitions = ScopeRole::DEFINITIONS
            .iter()
            .filter(|role| roles.contains(role))
            .map(|role| role.label().to_owned())
            .collect();
        GitStatus::Ready {
            branch: self.snapshot.branch,
            has_upstream: self.snapshot.has_upstream,
            ahead: self.snapshot.ahead,
            behind: self.snapshot.behind,
            dirty_items,
            dirty_definitions,
            fetch_error,
        }
    }

    /// Uncommitted changes the button would *not* commit — everything
    /// dirty in the repository that is outside the workdown paths.
    fn outside_scope(&self) -> Vec<&str> {
        self.snapshot
            .dirty
            .iter()
            .map(|change| change.path.as_str())
            .filter(|path| !self.in_scope.iter().any(|change| change.path == *path))
            .collect()
    }
}

/// Read the repository: snapshot, scope, in-scope changes. `None` when
/// the project is not inside a git work tree. The scoped status is
/// skipped when the whole repository is clean — nothing in scope can
/// be dirty then.
async fn read_local(state: &AppState) -> Result<Option<LocalState>, GitError> {
    let root = &state.project_root;
    let Some(snapshot) = git::snapshot(root).await? else {
        return Ok(None);
    };
    let Some(prefix) = git::repository_prefix(root).await? else {
        return Ok(None);
    };
    let scope = GitScope::from_config(&prefix, &state.config, &state.config_path, root);
    let in_scope = if snapshot.dirty.is_empty() {
        Vec::new()
    } else {
        git::scoped_changes(root, &scope.pathspecs()).await?
    };
    Ok(Some(LocalState {
        snapshot,
        scope,
        in_scope,
    }))
}

/// Re-read the status after a mutation. `NotARepo` mid-request means
/// the repository vanished under us — reported as-is rather than
/// guessed around.
async fn fresh_status(state: &AppState) -> Result<GitStatus, GitError> {
    Ok(match read_local(state).await? {
        Some(local) => local.into_status(None),
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
    let Some(local) = read_local(state).await? else {
        return Ok(ApiResponse::ok(GitStatus::NotARepo));
    };
    if !with_fetch {
        return Ok(ApiResponse::ok(local.into_status(None)));
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
    Ok(ApiResponse::ok(match read_local(state).await? {
        Some(refreshed) => refreshed.into_status(fetch_error),
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
    let Some(local) = read_local(state).await? else {
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
    if !local.snapshot.dirty.is_empty() {
        // Uncommitted work anywhere blocks the pull — but the pill only
        // shows the in-scope part, so when everything dirty is *outside*
        // the workdown paths the message has to name it: the user sees
        // a clean pill and needs to know what the repository sees.
        let outside = local.outside_scope();
        let message = if local.in_scope.is_empty() {
            format!(
                "there are uncommitted changes outside the workdown paths ({}) — pull never touches uncommitted work; commit or stash them in a terminal first",
                outside.join(", ")
            )
        } else {
            "there are uncommitted changes — pull never touches uncommitted work; commit first"
                .to_owned()
        };
        return Ok(ApiResponse::failed(StatusCode::CONFLICT, message));
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
    let Some(current) = read_local(state).await? else {
        return Ok(ApiResponse::failed(
            StatusCode::CONFLICT,
            "the project is not inside a git repository".to_owned(),
        ));
    };
    if current.snapshot.behind == 0 {
        return Ok(ApiResponse::ok(GitPullResult {
            pulled_commits: 0,
            status: current.into_status(None),
        }));
    }
    let pulled_commits = current.snapshot.behind;
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

/// `GET /api/git/commit-preview` — what a commit from here would cover
/// and say. Read-only and repeatable: it stages nothing. Origin-guarded
/// like the other git surfaces because it reads file contents out of
/// the repository, which is more than the status reveals.
async fn git_commit_preview(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResponse<GitCommitPreview> {
    if !git_controls_enabled(&state) {
        return refuse_disabled();
    }
    if let Some(refusal) = refuse_foreign_origin(&headers) {
        return refusal;
    }
    // The schema words the message (field names, choice values); a
    // project that cannot load has no schema to word it with, and
    // answers the way every other project read does.
    let project = match load_state_project(&state) {
        Ok(project) => project,
        Err(response) => return response,
    };
    match preview_inner(&state, &project.schema).await {
        Ok(response) => response,
        Err(error) => git_failure(error),
    }
}

async fn preview_inner(
    state: &AppState,
    schema: &Schema,
) -> Result<ApiResponse<GitCommitPreview>, GitError> {
    let root = &state.project_root;
    let (Some(local), Some(top_level)) = (read_local(state).await?, git::top_level(root).await?)
    else {
        return Ok(ApiResponse::failed(
            StatusCode::CONFLICT,
            "the project is not inside a git repository".to_owned(),
        ));
    };

    let mut files = Vec::with_capacity(local.in_scope.len());
    let mut inputs = Vec::with_capacity(local.in_scope.len());
    for change in &local.in_scope {
        // Git matched the path against the scope's own pathspecs, so a
        // role is always found; the fallback only keeps an unexpected
        // disagreement from dropping a file the commit would include.
        let (role, kind) = local
            .scope
            .classify(&change.path)
            .map_or(("files", ChangedFileKind::Definition), |role| {
                (role.label(), role.kind())
            });
        let project_path = local.scope.project_relative(&change.path);
        let (old_text, new_text, change_kind) = match &change.state {
            ChangeState::Added => (
                None,
                read_work_tree(&top_level, &change.path),
                GitChangeKind::Added,
            ),
            ChangeState::Deleted => (
                git::show_at_head(root, &change.path).await?,
                None,
                GitChangeKind::Deleted,
            ),
            ChangeState::Modified => (
                git::show_at_head(root, &change.path).await?,
                read_work_tree(&top_level, &change.path),
                GitChangeKind::Modified,
            ),
            ChangeState::Renamed { from } => (
                git::show_at_head(root, from).await?,
                read_work_tree(&top_level, &change.path),
                GitChangeKind::Renamed,
            ),
            ChangeState::Unmerged => (
                git::show_at_head(root, &change.path).await?,
                read_work_tree(&top_level, &change.path),
                GitChangeKind::Unmerged,
            ),
        };
        files.push(GitChangedFile {
            path: project_path.clone(),
            role: role.to_owned(),
            change: change_kind,
        });
        inputs.push(ChangedFile {
            path: std::path::PathBuf::from(project_path),
            kind,
            old_text,
            new_text,
        });
    }

    let title_field = state.config.defaults.display.title.as_deref();
    let message = summarize_changes(&inputs, schema, title_field).to_message();
    let outside = local
        .outside_scope()
        .into_iter()
        .map(str::to_owned)
        .collect();
    Ok(ApiResponse::ok(GitCommitPreview {
        files,
        message,
        outside,
    }))
}

/// The working-tree text of a repository-relative path, or `None` when
/// it cannot be read (deleted between the status and now, or not text).
fn read_work_tree(top_level: &std::path::Path, repository_path: &str) -> Option<String> {
    std::fs::read(top_level.join(repository_path))
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}
