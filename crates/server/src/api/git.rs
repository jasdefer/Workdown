//! `/api/git` — the git sync surface: status, pull, push, and the
//! commit-and-push behind the "Commit & push" dialog (preview, commit).
//!
//! Only present in spirit when `serve.git_controls: true` is set in
//! `config.yaml`: with the flag off, status answers `disabled` (so the
//! UI knows to hide the widget) and the mutating endpoints refuse.
//!
//! The git work itself is `workdown_git`, which is synchronous: each
//! handler takes the git lock where it needs it, then runs the whole
//! action — checks, git calls, report — as plain code on a blocking
//! thread (`blocking`). Nothing in the action bodies is async.
//!
//! Failure vocabulary (the tiers of ADR-013, extended for a surface
//! that talks to a network): `409` for refusals about repository state
//! (uncommitted changes, a rebase in progress, a rejected push or a
//! conflicting pull), `502` when the remote can't be reached by an
//! operation that needs it, `500` when git itself can't be run. Every
//! action returns `GitActionError` for those three and `respond` maps
//! it to the response in one place. A *status* request degrades instead
//! of failing when only the remote is unreachable — the local numbers
//! are still the truth, and the `fetch_error` field says what the remote
//! contact hit.
//!
//! Facts and words are kept apart. The sections below produce facts —
//! a `PullAttempt`, a `PushAttempt`, git's output — and never a
//! sentence; every sentence a person reads comes from the `wording`
//! module at the end of the file, so the voice of the git controls can
//! be read and changed in one place.

use std::path::Path;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use workdown_core::git_data::{
    GitCommitPreview, GitCommitRequest, GitCommitResult, GitPullResult, GitPullStep, GitPushResult,
    GitPushStep, GitStatus,
};
use workdown_core::model::schema::Schema;
use workdown_git::preview::{self, LocalState};
use workdown_git::scope::GitScope;
use workdown_git::{self as git, ChangeState, GitError};

use crate::envelope::ApiResponse;
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

/// Why a git action did not do what was asked — the failure tiers of
/// this surface, mapped to a response by [`respond`]. Actions return
/// this so each check is one line and every refusal is worded the same
/// way.
enum GitActionError {
    /// A refusal about repository state, worded for the user (`409`).
    /// Where git's own output matters, the message is the sentence, a
    /// blank line, then the raw output — the dialog shows the sentence
    /// and keeps the rest behind a details toggle; see
    /// `wording::with_details`.
    Refused(String),
    /// The remote could not be reached by an operation that needs it
    /// (`502`).
    RemoteUnreachable(String),
    /// git could not answer at all — spawn failure, timeout, refused
    /// plumbing (`500`).
    Git(GitError),
    /// The blocking thread running the action died (`500`) — a panic
    /// somewhere below, reported rather than swallowed.
    ThreadFailed(String),
}

impl From<GitError> for GitActionError {
    fn from(error: GitError) -> Self {
        GitActionError::Git(error)
    }
}

/// A refusal about repository state, ready to `return Err(...)`.
fn refuse(message: impl Into<String>) -> GitActionError {
    GitActionError::Refused(message.into())
}

impl GitActionError {
    /// The sentence a person reads for this failure — the response's
    /// `error`, or the details of a commit step that could not run.
    fn message(&self) -> String {
        match self {
            GitActionError::Refused(message) | GitActionError::RemoteUnreachable(message) => {
                message.clone()
            }
            GitActionError::Git(error) => error.to_string(),
            GitActionError::ThreadFailed(cause) => wording::action_did_not_finish(cause),
        }
    }

    fn status_code(&self) -> StatusCode {
        match self {
            GitActionError::Refused(_) => StatusCode::CONFLICT,
            GitActionError::RemoteUnreachable(_) => StatusCode::BAD_GATEWAY,
            GitActionError::Git(_) | GitActionError::ThreadFailed(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }
}

/// The one mapping from an action's outcome to its response.
fn respond<T: serde::Serialize>(result: Result<T, GitActionError>) -> ApiResponse<T> {
    match result {
        Ok(data) => ApiResponse::ok(data),
        Err(error) => ApiResponse::failed(error.status_code(), error.message()),
    }
}

/// Run one git action on a blocking thread. The git layer is
/// synchronous — it waits on child processes — so it must not run on
/// the async worker threads that serve every other request.
async fn blocking<T, Work>(work: Work) -> Result<T, GitActionError>
where
    T: Send + 'static,
    Work: FnOnce() -> Result<T, GitActionError> + Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .unwrap_or_else(|join_error| Err(GitActionError::ThreadFailed(join_error.to_string())))
}

/// Refusal for a mutating endpoint called without the opt-in flag.
fn refuse_disabled<T: serde::Serialize>() -> ApiResponse<T> {
    ApiResponse::failed(StatusCode::NOT_FOUND, wording::controls_disabled())
}

/// The POSTs here are covered by the same-origin layer over the whole
/// API ([`crate::origin::guard_mutations`]). Two *reads* need the same
/// guard and apply it themselves: `GET /api/git?fetch=true` contacts the
/// remote (and can invoke a credential helper), and the commit preview
/// returns file contents.
fn refuse_foreign_origin<T: serde::Serialize>(headers: &HeaderMap) -> Option<ApiResponse<T>> {
    crate::origin::is_foreign_origin(headers).then(crate::origin::refusal)
}

/// The git-controls scope for this server's project, computed on the
/// first request that finds a repository and kept in the state from then
/// on (see `AppState::git_scope`). `None` while the project is not
/// inside a git work tree.
fn scope(state: &AppState) -> Result<Option<GitScope>, GitError> {
    if let Some(scope) = state.git_scope.get() {
        return Ok(Some(scope.clone()));
    }
    let Some(scope) =
        GitScope::for_project(&state.project_root, &state.config, &state.config_path)?
    else {
        return Ok(None);
    };
    // Two first requests may race to fill the cell; they computed the
    // same value, so whichever landed first is the one to keep.
    let _ = state.git_scope.set(scope);
    Ok(state.git_scope.get().cloned())
}

/// Read the repository for this server's project — snapshot, scope and
/// in-scope changes; see [`preview::read_local`].
fn read_local(state: &AppState) -> Result<Option<LocalState>, GitError> {
    let Some(scope) = scope(state)? else {
        return Ok(None);
    };
    preview::read_local(&state.project_root, &scope)
}

/// [`read_local`] for an action: not being in a repository is a refusal.
fn require_local(state: &AppState) -> Result<LocalState, GitActionError> {
    read_local(state)?.ok_or_else(|| refuse(wording::NOT_A_REPOSITORY))
}

/// Re-read the status after a mutation. `NotARepo` mid-request means
/// the repository vanished under us — reported as-is rather than
/// guessed around.
fn fresh_status(state: &AppState) -> Result<GitStatus, GitError> {
    Ok(match read_local(state)? {
        Some(local) => local.into_status(None),
        None => GitStatus::NotARepo,
    })
}

// ── Status ──────────────────────────────────────────────────────────

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
    // A fetch touches the remote and the tracking refs, so it takes the
    // lock like every other operation that does — a fetch racing a
    // pull's own fetch loses on git's ref locks and would surface as a
    // spurious error. A local-only status needs no lock.
    let lock = state.git_lock.clone();
    let _network = if query.fetch {
        Some(lock.lock().await)
    } else {
        None
    };
    respond(blocking(move || status_inner(&state, query.fetch)).await)
}

fn status_inner(state: &AppState, with_fetch: bool) -> Result<GitStatus, GitActionError> {
    let root = &state.project_root;
    let Some(local) = read_local(state)? else {
        return Ok(GitStatus::NotARepo);
    };
    if !with_fetch {
        return Ok(local.into_status(None));
    }
    let fetch_error = match git::fetch(root) {
        Ok(output) if output.success => None,
        // An unreachable remote degrades the answer instead of replacing
        // it: the local numbers are still the truth, `behind` is simply
        // as of the last successful fetch, and the field says why.
        Ok(output) => Some(output.stderr.trim().to_owned()),
        Err(error) => Some(error.to_string()),
    };
    Ok(match read_local(state)? {
        Some(refreshed) => refreshed.into_status(fetch_error),
        None => GitStatus::NotARepo,
    })
}

// ── Pull ────────────────────────────────────────────────────────────

async fn git_pull(State(state): State<AppState>) -> ApiResponse<GitPullResult> {
    if !git_controls_enabled(&state) {
        return refuse_disabled();
    }
    let lock = state.git_lock.clone();
    let _network = lock.lock().await;
    respond(blocking(move || pull_inner(&state)).await)
}

/// The Pull button: one [`attempt_pull`], framed as a response. A pull
/// that did not happen is the request's error — nothing else happened,
/// so there is nothing else to report.
fn pull_inner(state: &AppState) -> Result<GitPullResult, GitActionError> {
    let pulled_commits = wording::pull_response(attempt_pull(state)?)?;
    // The changed files also wake the file watcher, whose ping makes
    // every open tab refetch its view — no manual event needed here.
    Ok(GitPullResult {
        pulled_commits,
        status: fresh_status(state)?,
    })
}

/// How a pull went — one sequence for the Pull button and the pull step
/// of a commit, which only frame the outcome differently (as the
/// response, or as one row of the commit's checklist). Every variant is
/// a fact about the repository; the wording is the caller's.
enum PullAttempt {
    /// The branch tracks no upstream — nowhere to pull from.
    NoUpstream,
    /// A rebase is underway — the user's, possibly mid-conflict in a
    /// terminal — and must never be disturbed from here.
    RebaseInProgress,
    /// The fetch could not reach the remote; `details` is git's output.
    RemoteUnreachable {
        details: String,
    },
    /// Nothing to integrate after the fetch.
    NotBehind,
    /// `behind` commits are waiting, but uncommitted work blocks a
    /// rebase over them — pull never stashes. `outside` names the dirty
    /// files outside the workdown paths, the ones the pill never shows.
    Blocked {
        behind: u32,
        in_scope_dirty: bool,
        outside: Vec<String>,
    },
    Pulled {
        commits: u32,
    },
    /// git ran the pull and it did not complete — a conflict, usually.
    /// Our rebase has been aborted, so the repository is as it was;
    /// `conflicted` are the paths git named, `details` its output.
    Failed {
        conflicted: Vec<String>,
        details: String,
    },
}

/// Fetch, then `pull --rebase` if — and only if — the branch is behind.
///
/// The refusals that need no network come first, the rebase check
/// before all others: mid-rebase git reports a detached head with no
/// upstream, and "not published" would be the wrong thing to say about
/// a repository the user is resolving in a terminal. Uncommitted work is
/// judged *after* the fetch, because it only matters when there is
/// something to rebase over — and nothing is ever stashed or rebased
/// over from a browser button. A pull that fails is backed out: no
/// rebase was underway before this call, so the abort only ever undoes
/// ours, never one the user is resolving in a terminal.
fn attempt_pull(state: &AppState) -> Result<PullAttempt, GitActionError> {
    let root = &state.project_root;
    if git::rebase_in_progress(root)? {
        return Ok(PullAttempt::RebaseInProgress);
    }
    let local = require_local(state)?;
    if !local.snapshot.has_upstream {
        return Ok(PullAttempt::NoUpstream);
    }
    // Fetch first, then read `behind`: after the fetch it is exactly
    // the number of commits the pull will integrate — the same number
    // the pill showed. (Measuring the tracking ref's movement *during*
    // the pull instead would report "already up to date" whenever an
    // earlier status call had already fetched.)
    let fetched = git::fetch(root)?;
    if !fetched.success {
        return Ok(PullAttempt::RemoteUnreachable {
            details: fetched.stderr.trim().to_owned(),
        });
    }
    let current = require_local(state)?;
    let behind = current.snapshot.behind;
    if behind == 0 {
        return Ok(PullAttempt::NotBehind);
    }
    if !current.snapshot.dirty.is_empty() {
        return Ok(PullAttempt::Blocked {
            behind,
            in_scope_dirty: !current.in_scope.is_empty(),
            outside: current
                .outside_scope()
                .into_iter()
                .map(str::to_owned)
                .collect(),
        });
    }
    let pulled = match git::pull(root) {
        Ok(output) => output,
        Err(error) => {
            // A timeout kills git mid-operation and can leave its rebase
            // in progress.
            let _ = git::abort_rebase(root);
            return Err(error.into());
        }
    };
    if !pulled.success {
        let _ = git::abort_rebase(root);
        let details = format!("{}\n{}", pulled.stdout.trim(), pulled.stderr.trim())
            .trim()
            .to_owned();
        return Ok(PullAttempt::Failed {
            conflicted: conflicted_paths(&details),
            details,
        });
    }
    Ok(PullAttempt::Pulled { commits: behind })
}

/// The paths git names in its `CONFLICT (…): Merge conflict in <path>`
/// lines.
fn conflicted_paths(raw: &str) -> Vec<String> {
    raw.lines()
        .filter_map(|line| line.split("Merge conflict in ").nth(1))
        .map(|path| path.trim().to_owned())
        .collect()
}

// ── Push ────────────────────────────────────────────────────────────

async fn git_push(State(state): State<AppState>) -> ApiResponse<GitPushResult> {
    if !git_controls_enabled(&state) {
        return refuse_disabled();
    }
    let lock = state.git_lock.clone();
    let _network = lock.lock().await;
    respond(blocking(move || push_inner(&state)).await)
}

/// The Push button: one [`attempt_push`], framed as a response.
fn push_inner(state: &AppState) -> Result<GitPushResult, GitActionError> {
    let published = wording::push_response(attempt_push(&state.project_root)?)?;
    Ok(GitPushResult {
        published,
        status: fresh_status(state)?,
    })
}

/// How a push or publish went — one sequence for the Push button and
/// the push step of a commit. Every variant is a fact; the wording is
/// the caller's.
enum PushAttempt {
    /// Pushed to the upstream, or — `published` — created the remote
    /// branch and recorded it as upstream.
    Pushed { published: bool },
    /// Not attempted: the branch has no upstream and cannot be
    /// published either.
    Blocked(PublishBlocker),
    /// git ran the push (`published`: as a first publish) and said no;
    /// `details` is its output.
    Rejected { published: bool, details: String },
}

/// Why a branch with no upstream cannot be published.
enum PublishBlocker {
    /// `HEAD` is detached — no branch to publish.
    DetachedHead,
    /// The branch is unborn (fresh `git init`) — nothing to publish.
    NoCommits,
    /// No remote to publish to by the rule in [`git::publish_remote`].
    NoRemote,
}

/// Push, or — on a branch with no upstream — *publish*: the same
/// gesture from the user's side ("get my commits onto the remote"),
/// with git's first-time bookkeeping (create the remote branch, record
/// it as upstream) handled here instead of in a terminal. Which of the
/// two is decided from the repository's present state, not from what
/// the pill believed when it was clicked. Refusals about what there is
/// to publish come before any network.
fn attempt_push(root: &Path) -> Result<PushAttempt, GitActionError> {
    let Some(local) = git::snapshot(root)? else {
        return Err(refuse(wording::NOT_A_REPOSITORY));
    };
    let published = !local.has_upstream;
    let pushed = if published {
        if local.branch == "HEAD" {
            return Ok(PushAttempt::Blocked(PublishBlocker::DetachedHead));
        }
        if !local.has_commits {
            return Ok(PushAttempt::Blocked(PublishBlocker::NoCommits));
        }
        let Some(remote) = git::publish_remote(root)? else {
            return Ok(PushAttempt::Blocked(PublishBlocker::NoRemote));
        };
        git::publish(root, &remote, &local.branch)?
    } else {
        git::push(root)?
    };
    if !pushed.success {
        return Ok(PushAttempt::Rejected {
            published,
            details: pushed.stderr.trim().to_owned(),
        });
    }
    Ok(PushAttempt::Pushed { published })
}

// ── Commit preview ──────────────────────────────────────────────────

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
    respond(blocking(move || preview_inner(&state, &project.schema)).await)
}

fn preview_inner(state: &AppState, schema: &Schema) -> Result<GitCommitPreview, GitActionError> {
    let Some(scope) = scope(state)? else {
        return Err(refuse(wording::NOT_A_REPOSITORY));
    };
    preview::build_preview(&state.project_root, &scope, &state.config, schema)?
        .ok_or_else(|| refuse(wording::NOT_A_REPOSITORY))
}

// ── Commit & push ───────────────────────────────────────────────────

/// `POST /api/git/commit` — the confirmed gesture: stage the workdown
/// paths, commit with the confirmed message, pull with rebase if the
/// branch is behind, push. One server action under the git lock, so
/// nothing can slip between the steps, with a per-step report.
///
/// Refusals before the commit are `409` with a worded error. A stale
/// file list is also `409`: the dialog reloads the preview on any `409`
/// from here, which is right for every case.
///
/// Once the commit exists the answer is `200` whatever pull and push
/// did: the commit is real, and the report says how far it got.
async fn git_commit(
    State(state): State<AppState>,
    Json(request): Json<GitCommitRequest>,
) -> ApiResponse<GitCommitResult> {
    if !git_controls_enabled(&state) {
        return refuse_disabled();
    }
    let lock = state.git_lock.clone();
    let _network = lock.lock().await;
    respond(blocking(move || commit_inner(&state, request)).await)
}

fn commit_inner(
    state: &AppState,
    request: GitCommitRequest,
) -> Result<GitCommitResult, GitActionError> {
    let root = &state.project_root;
    let local = require_local(state)?;
    if local.snapshot.branch == "HEAD" {
        return Err(refuse(wording::COMMIT_ON_DETACHED_HEAD));
    }
    if git::rebase_in_progress(root)? {
        return Err(refuse(wording::REBASE_IN_PROGRESS));
    }
    let message = request.message.trim();
    if message.is_empty() {
        return Err(refuse(wording::EMPTY_MESSAGE));
    }
    if local.in_scope.is_empty() {
        return Err(refuse(wording::NOTHING_TO_COMMIT));
    }
    ensure_confirmed_set(&local, &request.files)?;
    ensure_nothing_conflicted(&local)?;
    if git::identity(root)?.is_none() {
        return Err(refuse(wording::NO_IDENTITY));
    }

    let paths = confirmed_paths(&local);
    let pathspecs: Vec<String> = paths
        .iter()
        .map(|path| git::literal_pathspec(path))
        .collect();
    let staged = git::stage(root, &pathspecs)?;
    if !staged.success {
        return Err(refuse(wording::staging_failed(&staged.stderr)));
    }
    let committed = git::commit(root, message, &pathspecs)?;
    if !committed.success {
        // Leave the index as the user had it: our staging is undone,
        // their working tree was never touched.
        let _ = git::unstage(root, &pathspecs);
        return Err(refuse(wording::commit_failed(&committed)));
    }
    // A pre-commit hook may have added files to the commit (a
    // re-rendered views directory, for one); the real index still holds
    // them as they were, which would block the next pull.
    git::absorb_hook_additions(root, &paths)?;
    let commit = git::head_short_hash(root)?;

    // From here on the commit exists, so nothing is an error response
    // any more: the same pull and push the buttons run, each framed as
    // one row of the checklist — even git failing to run becomes a row
    // that says the commit is safe and what to do next.
    let pull = wording::pull_row(attempt_pull(state));
    let push = push_step(root, &pull);
    Ok(GitCommitResult {
        commit,
        pull,
        push,
        status: fresh_status(state)?,
    })
}

/// The review moment: the set the user confirmed must be the set the
/// commit will cover. Another tab or an editor may have changed it.
fn ensure_confirmed_set(local: &LocalState, confirmed: &[String]) -> Result<(), GitActionError> {
    let mut shown = confirmed.to_vec();
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
        return Err(refuse(wording::STALE_FILE_LIST));
    }
    Ok(())
}

/// Git will not commit a file that still carries conflict markers.
fn ensure_nothing_conflicted(local: &LocalState) -> Result<(), GitActionError> {
    let conflicted: Vec<&str> = local
        .in_scope
        .iter()
        .filter(|change| change.state == ChangeState::Unmerged)
        .map(|change| change.path.as_str())
        .collect();
    if conflicted.is_empty() {
        return Ok(());
    }
    Err(refuse(wording::conflicted_files(&conflicted)))
}

/// The repository-relative paths to stage and commit: exactly the files
/// git listed for the scope — the set the user just confirmed. `git add`
/// (unlike `git status`) refuses a pathspec that matches nothing, which
/// a scope entry for a file the project does not have (no
/// resources.yaml, say) would be; the changed files themselves always
/// exist on one side. A rename contributes both of its paths.
fn confirmed_paths(local: &LocalState) -> Vec<String> {
    local
        .in_scope
        .iter()
        .flat_map(|change| {
            let mut paths = vec![change.path.clone()];
            if let ChangeState::Renamed { from } = &change.state {
                paths.push(from.clone());
            }
            paths
        })
        .collect()
}

/// The push step of a commit: not attempted when the pull stopped;
/// otherwise the same [`attempt_push`] the Push button runs, framed as
/// a checklist row.
fn push_step(root: &Path, pull: &GitPullStep) -> GitPushStep {
    if let GitPullStep::Stopped { .. } = pull {
        return wording::push_not_attempted();
    }
    wording::push_row(attempt_push(root))
}

// ── Wording ─────────────────────────────────────────────────────────

/// Every sentence a person reads from this surface.
///
/// The sections above produce facts — a [`PullAttempt`], a
/// [`PushAttempt`], git's output — and come here to turn them into a
/// refusal, a checklist row or an error message. The same fact is
/// worded twice where the frame differs: as the answer to a button
/// press, where a pull that did not happen is the whole story, and as
/// one row of the commit's checklist, where the commit already exists
/// and the row must say so and name the way out.
mod wording {
    use super::{refuse, GitActionError, PublishBlocker, PullAttempt, PushAttempt};
    use workdown_core::git_data::{GitPullStep, GitPushStep};
    use workdown_git::GitOutput;

    pub(super) const NOT_A_REPOSITORY: &str = "the project is not inside a git repository";
    pub(super) const REBASE_IN_PROGRESS: &str =
        "a rebase is in progress in this repository — finish or abort it in a terminal first";
    pub(super) const COMMIT_ON_DETACHED_HEAD: &str =
        "detached HEAD — a commit here has no branch to push; check one out in a terminal first";
    pub(super) const EMPTY_MESSAGE: &str = "the commit message is empty";
    pub(super) const NOTHING_TO_COMMIT: &str = "nothing to commit — no workdown files have changed";
    pub(super) const STALE_FILE_LIST: &str =
        "the set of changed files has moved since the preview — review the new list before committing";
    pub(super) const NO_IDENTITY: &str =
        "git has no identity to commit as — set user.name and user.email in a terminal first";

    pub(super) fn controls_disabled() -> String {
        "git controls are not enabled — set 'serve.git_controls: true' in config.yaml and restart"
            .to_owned()
    }

    pub(super) fn action_did_not_finish(cause: &str) -> String {
        format!("the git action did not finish: {cause}")
    }

    // ── The Pull and Push buttons ────────────────────────────────────

    /// The Pull button's answer: how many commits came in, or the
    /// refusal.
    pub(super) fn pull_response(attempt: PullAttempt) -> Result<u32, GitActionError> {
        match attempt {
            PullAttempt::Pulled { commits } => Ok(commits),
            PullAttempt::NotBehind => Ok(0),
            PullAttempt::NoUpstream => Err(refuse(
                "the branch is not published yet — there is nothing to pull from; publish it first",
            )),
            PullAttempt::RebaseInProgress => Err(refuse(REBASE_IN_PROGRESS)),
            PullAttempt::RemoteUnreachable { details } => Err(GitActionError::RemoteUnreachable(
                format!("fetch failed: {details}"),
            )),
            PullAttempt::Blocked {
                in_scope_dirty: true,
                ..
            } => Err(refuse(
                "there are uncommitted changes — pull never touches uncommitted work; commit first",
            )),
            // Everything dirty is *outside* the workdown paths: the pill
            // reads clean, so the message has to say what the repository
            // sees.
            PullAttempt::Blocked { outside, .. } => Err(refuse(format!(
                "there are uncommitted changes outside the workdown paths ({}) — pull never touches uncommitted work; commit or stash them in a terminal first",
                outside.join(", ")
            ))),
            PullAttempt::Failed {
                conflicted,
                details,
            } => {
                let sentence = if conflicted.is_empty() {
                    "the pull did not complete — see the details, then pull in a terminal".to_owned()
                } else {
                    format!(
                        "the pull could not combine a teammate's change to {} with yours — resolve the conflict in a terminal",
                        conflicted.join(", ")
                    )
                };
                Err(refuse(with_details(&sentence, &details)))
            }
        }
    }

    /// The Push button's answer: whether the branch was published (as
    /// opposed to pushed), or the refusal.
    pub(super) fn push_response(attempt: PushAttempt) -> Result<bool, GitActionError> {
        match attempt {
            PushAttempt::Pushed { published } => Ok(published),
            PushAttempt::Blocked(blocker) => Err(refuse(publish_blocker(blocker))),
            PushAttempt::Rejected { published, details } => {
                Err(refuse(format!("{}: {details}", push_failed(published))))
            }
        }
    }

    // ── The commit's checklist ───────────────────────────────────────

    /// The pull step of a commit as a checklist row. The commit exists
    /// by now, so every outcome — even git failing to run — becomes a
    /// row that says the commit is safe and what to do next.
    pub(super) fn pull_row(attempt: Result<PullAttempt, GitActionError>) -> GitPullStep {
        let skipped = |reason: &str| GitPullStep::Skipped {
            reason: reason.to_owned(),
        };
        let stopped =
            |reason: String, details: Option<String>| GitPullStep::Stopped { reason, details };
        match attempt {
            Ok(PullAttempt::Pulled { commits }) => GitPullStep::Pulled { commits },
            Ok(PullAttempt::NoUpstream) => {
                skipped("not published yet — the push step publishes the branch")
            }
            Ok(PullAttempt::NotBehind) => {
                skipped("nothing to integrate — the branch is not behind")
            }
            Ok(PullAttempt::RebaseInProgress) => stopped(
                "a rebase started in this repository meanwhile — the commit is safe and local; finish or abort it in a terminal, then press Pull and Push".to_owned(),
                None,
            ),
            Ok(PullAttempt::RemoteUnreachable { details }) => stopped(
                "the remote could not be reached, so the branch could not be brought up to date — the commit is safe and local; press Push once the remote is back".to_owned(),
                Some(details),
            ),
            // After the commit, anything still dirty is outside the scope
            // by construction — unless an editor wrote a workdown file in
            // the meantime, which the first sentence covers.
            Ok(PullAttempt::Blocked {
                behind,
                in_scope_dirty,
                outside,
            }) => stopped(
                if in_scope_dirty {
                    format!(
                        "the branch is {} behind, but workdown files changed again since the commit — the commit is safe and local; press Commit & push again",
                        plural_commits(behind)
                    )
                } else {
                    format!(
                        "the branch is {} behind, but uncommitted changes outside the workdown paths ({}) block the pull — the commit is safe and local; commit or stash them in a terminal, then press Pull and Push",
                        plural_commits(behind),
                        outside.join(", ")
                    )
                },
                None,
            ),
            // The three things a stuck pull must say: the commit is safe,
            // what it could not be combined with, and the way out.
            Ok(PullAttempt::Failed {
                conflicted,
                details,
            }) => stopped(
                if conflicted.is_empty() {
                    "the commit is safe and local, but the pull did not complete — see the details, pull in a terminal, then press Push".to_owned()
                } else {
                    format!(
                        "the commit is safe and local, but it could not be combined with a teammate's change to {} — resolve the conflict in a terminal, then press Push",
                        conflicted.join(", ")
                    )
                },
                Some(details),
            ),
            Err(error) => stopped(
                "the pull could not run — the commit is safe and local; pull in a terminal, then press Push".to_owned(),
                Some(error.message()),
            ),
        }
    }

    /// The push row when the pull row stopped: nothing was tried.
    pub(super) fn push_not_attempted() -> GitPushStep {
        GitPushStep::Skipped {
            reason: "not attempted — the pull did not complete".to_owned(),
        }
    }

    /// The push step of a commit as a checklist row; infallible for the
    /// same reason as [`pull_row`].
    pub(super) fn push_row(attempt: Result<PushAttempt, GitActionError>) -> GitPushStep {
        let stopped =
            |reason: String, details: Option<String>| GitPushStep::Stopped { reason, details };
        match attempt {
            Ok(PushAttempt::Pushed { published }) => GitPushStep::Pushed { published },
            Ok(PushAttempt::Blocked(blocker)) => stopped(publish_blocker(blocker).to_owned(), None),
            Ok(PushAttempt::Rejected { published, details }) => stopped(
                format!(
                    "{} — the commit is safe and local; pull, resolve, and press Push",
                    push_failed(published)
                ),
                Some(details),
            ),
            Err(error) => stopped(
                "the push could not run — the commit is safe and local; press Push".to_owned(),
                Some(error.message()),
            ),
        }
    }

    // ── Commit refusals ──────────────────────────────────────────────

    pub(super) fn conflicted_files(paths: &[&str]) -> String {
        format!(
            "{} still carr{} conflict markers — resolve in a terminal first",
            paths.join(", "),
            if paths.len() == 1 { "ies" } else { "y" }
        )
    }

    pub(super) fn staging_failed(stderr: &str) -> String {
        with_details("the changes could not be staged", stderr)
    }

    /// A commit that did not happen, worded from git's output where the
    /// cause is recognizable, with the raw output kept behind the
    /// sentence. A missing identity never reaches here: it is checked
    /// before staging.
    pub(super) fn commit_failed(output: &GitOutput) -> String {
        let raw = format!("{}\n{}", output.stdout.trim(), output.stderr.trim());
        let lower = raw.to_lowercase();
        let sentence = if lower.contains("gpg")
            || lower.contains("signing")
            || lower.contains("ssh-keygen")
        {
            "the commit could not be signed — the signing key or agent is not available to the server; commit from a terminal, or turn off commit.gpgsign"
        } else if lower.contains("hook") {
            "a git hook rejected the commit"
        } else {
            "the commit failed — a hook may have rejected it; see the details"
        };
        with_details(sentence, &raw)
    }

    // ── Shared pieces ────────────────────────────────────────────────

    /// Why a branch cannot be published — the same sentence for the
    /// button and the checklist, since neither has anything to add.
    fn publish_blocker(blocker: PublishBlocker) -> &'static str {
        match blocker {
            PublishBlocker::DetachedHead => {
                "detached HEAD — there is no branch to publish; check one out in a terminal first"
            }
            PublishBlocker::NoCommits => "the branch has no commits yet — nothing to publish",
            PublishBlocker::NoRemote => {
                "no remote to publish to — add one (or set remote.pushDefault) in a terminal"
            }
        }
    }

    fn push_failed(published: bool) -> String {
        format!("{} failed", if published { "publish" } else { "push" })
    }

    /// `sentence`, then git's raw output after a blank line when there
    /// is any — the convention the dialog splits on.
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
}
