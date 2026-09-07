//! `/api/git` — the git sync surface: status, pull, push, and the
//! commit-and-push behind the "Commit & push" dialog (preview, commit).
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
use axum::{Json, Router};
use serde::Deserialize;

use workdown_core::git_data::{
    GitCommitPreview, GitCommitRequest, GitCommitResult, GitPullResult, GitPullStep, GitPushResult,
    GitPushStep, GitStatus,
};

use crate::envelope::ApiResponse;
use crate::git::{self, ChangeState, GitError, RepoSnapshot};
use crate::git_preview::{self, LocalState};
use crate::state::{load_state_project, AppState};

/// Router for the git endpoints under `/api`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/git", get(git_status))
        .route("/git/pull", post(git_pull))
        .route("/git/push", post(git_push))
        .route("/git/commit-preview", get(git_commit_preview))
        .route("/git/commit", post(git_commit))
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

/// Read the repository for this server's project — snapshot, scope and
/// in-scope changes; see [`git_preview::read_local`].
async fn read_local(state: &AppState) -> Result<Option<LocalState>, GitError> {
    git_preview::read_local(&state.project_root, &state.config, &state.config_path).await
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
    match attempt_push(root, &local).await? {
        PushAttempt::Pushed { published } => Ok(ApiResponse::ok(GitPushResult {
            published,
            status: fresh_status(state).await?,
        })),
        PushAttempt::Refused(reason) => Ok(ApiResponse::failed(StatusCode::CONFLICT, reason)),
        PushAttempt::Rejected { reason, details } => Ok(ApiResponse::failed(
            StatusCode::CONFLICT,
            format!("{reason}: {details}"),
        )),
    }
}

/// How a push or publish went — shared by the Push button and the
/// push step of a commit.
enum PushAttempt {
    Pushed {
        published: bool,
    },
    /// Not attempted: something about the branch rules it out, worded.
    Refused(String),
    /// Attempted, and git said no; `details` is its output.
    Rejected {
        reason: String,
        details: String,
    },
}

/// Push, or on a branch with no upstream publish it (create the remote
/// branch, record it as upstream). Refusals about what there is to
/// publish come before any network.
async fn attempt_push(
    root: &std::path::Path,
    local: &RepoSnapshot,
) -> Result<PushAttempt, GitError> {
    let published = !local.has_upstream;
    let pushed = if published {
        if local.branch == "HEAD" {
            return Ok(PushAttempt::Refused(
                "detached HEAD — there is no branch to publish; check one out in a terminal first"
                    .to_owned(),
            ));
        }
        if !local.has_commits {
            return Ok(PushAttempt::Refused(
                "the branch has no commits yet — nothing to publish".to_owned(),
            ));
        }
        let Some(remote) = git::publish_remote(root).await? else {
            return Ok(PushAttempt::Refused(
                "no remote to publish to — add one (or set remote.pushDefault) in a terminal"
                    .to_owned(),
            ));
        };
        git::publish(root, &remote, &local.branch).await?
    } else {
        git::push(root).await?
    };
    if !pushed.success {
        return Ok(PushAttempt::Rejected {
            reason: format!("{} failed", if published { "publish" } else { "push" }),
            details: pushed.stderr.trim().to_owned(),
        });
    }
    Ok(PushAttempt::Pushed { published })
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
    match git_preview::build_preview(
        &state.project_root,
        &state.config,
        &state.config_path,
        &project.schema,
    )
    .await
    {
        Ok(Some(preview)) => ApiResponse::ok(preview),
        Ok(None) => ApiResponse::failed(
            StatusCode::CONFLICT,
            "the project is not inside a git repository".to_owned(),
        ),
        Err(error) => git_failure(error),
    }
}

// ── Commit & push ───────────────────────────────────────────────────

/// `POST /api/git/commit` — the confirmed gesture: stage the workdown
/// paths, commit with the confirmed message, pull with rebase if the
/// branch is behind, push. One server action under the git lock, so
/// nothing can slip between the steps, with a per-step report.
///
/// Refusals before the commit are `409` with a worded error. Where git's
/// own output matters, the error is the sentence, a blank line, then the
/// raw output — the dialog shows the sentence and keeps the rest behind
/// a details toggle. A stale file list is also `409`: the dialog reloads
/// the preview on any `409` from here, which is right for every case.
///
/// Once the commit exists the answer is `200` whatever pull and push
/// did: the commit is real, and the report says how far it got.
async fn git_commit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<GitCommitRequest>,
) -> ApiResponse<GitCommitResult> {
    if !git_controls_enabled(&state) {
        return refuse_disabled();
    }
    if let Some(refusal) = refuse_foreign_origin(&headers) {
        return refusal;
    }
    let _network = state.git_lock.lock().await;
    match commit_inner(&state, request).await {
        Ok(response) => response,
        Err(error) => git_failure(error),
    }
}

async fn commit_inner(
    state: &AppState,
    request: GitCommitRequest,
) -> Result<ApiResponse<GitCommitResult>, GitError> {
    let root = &state.project_root;
    let refuse = |message: String| Ok(ApiResponse::failed(StatusCode::CONFLICT, message));

    let Some(local) = read_local(state).await? else {
        return refuse("the project is not inside a git repository".to_owned());
    };
    if local.snapshot.branch == "HEAD" {
        return refuse(
            "detached HEAD — a commit here has no branch to push; check one out in a terminal first"
                .to_owned(),
        );
    }
    if git::rebase_in_progress(root).await? {
        return refuse(
            "a rebase is in progress in this repository — finish or abort it in a terminal first"
                .to_owned(),
        );
    }
    let message = request.message.trim();
    if message.is_empty() {
        return refuse("the commit message is empty".to_owned());
    }
    if local.in_scope.is_empty() {
        return refuse("nothing to commit — no workdown files have changed".to_owned());
    }

    // The review moment: the set the user confirmed must be the set the
    // commit will cover. Another tab or an editor may have changed it.
    let mut shown = request.files.clone();
    shown.sort();
    shown.dedup();
    let mut current: Vec<String> = local
        .in_scope
        .iter()
        .map(|change| local.scope.project_relative(&change.path))
        .collect();
    current.sort();
    current.dedup();
    if shown != current {
        return refuse(
            "the set of changed files has moved since the preview — review the new list before committing"
                .to_owned(),
        );
    }

    let conflicted: Vec<&str> = local
        .in_scope
        .iter()
        .filter(|change| change.state == ChangeState::Unmerged)
        .map(|change| change.path.as_str())
        .collect();
    if !conflicted.is_empty() {
        return refuse(format!(
            "{} still carr{} conflict markers — resolve in a terminal first",
            conflicted.join(", "),
            if conflicted.len() == 1 { "ies" } else { "y" }
        ));
    }
    if git::identity(root).await?.is_none() {
        return refuse(
            "git has no identity to commit as — set user.name and user.email in a terminal first"
                .to_owned(),
        );
    }

    // Stage and commit exactly the files git listed for the scope — the
    // set the user just confirmed. `git add` (unlike `git status`)
    // refuses a pathspec that matches nothing, which a scope entry for a
    // file the project does not have (no resources.yaml, say) would be;
    // the changed files themselves always exist on one side.
    let pathspecs: Vec<String> = local
        .in_scope
        .iter()
        .flat_map(|change| {
            let mut paths = vec![git::literal_pathspec(&change.path)];
            if let ChangeState::Renamed { from } = &change.state {
                paths.push(git::literal_pathspec(from));
            }
            paths
        })
        .collect();
    let staged = git::stage(root, &pathspecs).await?;
    if !staged.success {
        return refuse(with_details(
            "the changes could not be staged",
            &staged.stderr,
        ));
    }
    let committed = git::commit(root, message, &pathspecs).await?;
    if !committed.success {
        // Leave the index as the user had it: our staging is undone,
        // their working tree was never touched.
        let _ = git::unstage(root, &pathspecs).await;
        return refuse(commit_failure(&committed));
    }
    let commit = git::head_short_hash(root).await?;

    let pull = pull_step(state, local.snapshot.has_upstream).await?;
    let push = match &pull {
        GitPullStep::Stopped { .. } => GitPushStep::Skipped {
            reason: "not attempted — the pull did not complete".to_owned(),
        },
        GitPullStep::Skipped | GitPullStep::Pulled { .. } => match git::snapshot(root).await? {
            None => GitPushStep::Stopped {
                reason: "the project is no longer inside a git repository".to_owned(),
                details: None,
            },
            Some(snapshot) => match attempt_push(root, &snapshot).await? {
                PushAttempt::Pushed { published } => GitPushStep::Pushed { published },
                PushAttempt::Refused(reason) => GitPushStep::Stopped {
                    reason,
                    details: None,
                },
                PushAttempt::Rejected { reason, details } => GitPushStep::Stopped {
                    reason: format!(
                        "{reason} — the commit is safe and local; pull, resolve, and press Push"
                    ),
                    details: Some(details),
                },
            },
        },
    };

    Ok(ApiResponse::ok(GitCommitResult {
        commit,
        pull,
        push,
        status: fresh_status(state).await?,
    }))
}

/// The pull step of a commit: skipped without an upstream (publish
/// follows) or when not behind after a fetch; otherwise `pull --rebase`,
/// which the whole-repository dirty rule still guards — a failed rebase
/// is aborted so the repository is never left mid-rebase from here.
async fn pull_step(state: &AppState, has_upstream: bool) -> Result<GitPullStep, GitError> {
    let root = &state.project_root;
    if !has_upstream {
        return Ok(GitPullStep::Skipped);
    }
    let stopped =
        |reason: String, details: Option<String>| Ok(GitPullStep::Stopped { reason, details });
    let fetched = git::fetch(root).await?;
    if !fetched.success {
        return stopped(
            "the remote could not be reached, so the branch could not be brought up to date — the commit is safe and local; press Push once the remote is back".to_owned(),
            Some(fetched.stderr.trim().to_owned()),
        );
    }
    let Some(current) = read_local(state).await? else {
        return stopped(
            "the project is no longer inside a git repository".to_owned(),
            None,
        );
    };
    if current.snapshot.behind == 0 {
        return Ok(GitPullStep::Skipped);
    }
    if !current.snapshot.dirty.is_empty() {
        // After the commit, anything still dirty is outside the scope by
        // construction — the files the pill never showed.
        let outside: Vec<&str> = current
            .snapshot
            .dirty
            .iter()
            .map(|change| change.path.as_str())
            .collect();
        return stopped(
            format!(
                "the branch is {} behind, but uncommitted changes outside the workdown paths ({}) block the pull — the commit is safe and local; commit or stash them in a terminal, then press Pull and Push",
                plural_commits(current.snapshot.behind),
                outside.join(", ")
            ),
            None,
        );
    }
    let behind = current.snapshot.behind;
    let pulled = match git::pull(root).await {
        Ok(output) => output,
        Err(error) => {
            // Same reasoning as the pull endpoint: no rebase was underway
            // before this request, so an abort only backs out ours.
            let _ = git::abort_rebase(root).await;
            return stopped(
                "the pull was interrupted — the commit is safe and local; pull in a terminal, then press Push".to_owned(),
                Some(error.to_string()),
            );
        }
    };
    if !pulled.success {
        let _ = git::abort_rebase(root).await;
        let raw = format!("{}\n{}", pulled.stdout.trim(), pulled.stderr.trim());
        return stopped(conflict_wording(&raw), Some(raw.trim().to_owned()));
    }
    Ok(GitPullStep::Pulled { commits: behind })
}

/// The three things a stuck pull must say: the commit is safe, what it
/// could not be combined with, and the way out. Conflicted paths are
/// read from git's `CONFLICT (…): Merge conflict in <path>` lines.
fn conflict_wording(raw: &str) -> String {
    let conflicted: Vec<&str> = raw
        .lines()
        .filter_map(|line| line.split("Merge conflict in ").nth(1))
        .map(str::trim)
        .collect();
    if conflicted.is_empty() {
        "the commit is safe and local, but the pull did not complete — see the details, pull in a terminal, then press Push".to_owned()
    } else {
        format!(
            "the commit is safe and local, but it could not be combined with a teammate's change to {} — resolve the conflict in a terminal, then press Push",
            conflicted.join(", ")
        )
    }
}

/// A commit that did not happen, worded from git's output where the
/// cause is recognizable, with the raw output kept behind the sentence.
fn commit_failure(output: &git::GitOutput) -> String {
    let raw = format!("{}\n{}", output.stdout.trim(), output.stderr.trim());
    let lower = raw.to_lowercase();
    let sentence = if lower.contains("gpg")
        || lower.contains("signing")
        || lower.contains("ssh-keygen")
    {
        "the commit could not be signed — the signing key or agent is not available to the server; commit from a terminal, or turn off commit.gpgsign"
    } else if lower.contains("please tell me who you are") {
        "git has no identity to commit as — set user.name and user.email in a terminal first"
    } else if lower.contains("hook") {
        "a git hook rejected the commit"
    } else {
        "the commit failed — a hook may have rejected it; see the details"
    };
    with_details(sentence, &raw)
}

/// `sentence`, then git's raw output after a blank line when there is
/// any — the convention the dialog splits on.
fn with_details(sentence: &str, raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() {
        sentence.to_owned()
    } else {
        format!("{sentence}\n\n{raw}")
    }
}

fn plural_commits(count: u32) -> String {
    if count == 1 {
        "1 commit".to_owned()
    } else {
        format!("{count} commits")
    }
}
