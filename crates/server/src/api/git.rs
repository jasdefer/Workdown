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
use workdown_git::{self as git, ChangeState, GitError, RepoSnapshot};

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
    /// and keeps the rest behind a details toggle; see [`with_details`].
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

/// The one mapping from an action's outcome to its response.
fn respond<T: serde::Serialize>(result: Result<T, GitActionError>) -> ApiResponse<T> {
    match result {
        Ok(data) => ApiResponse::ok(data),
        Err(GitActionError::Refused(message)) => ApiResponse::failed(StatusCode::CONFLICT, message),
        Err(GitActionError::RemoteUnreachable(message)) => {
            ApiResponse::failed(StatusCode::BAD_GATEWAY, message)
        }
        Err(GitActionError::Git(error)) => {
            ApiResponse::failed(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
        }
        Err(GitActionError::ThreadFailed(message)) => ApiResponse::failed(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("the git action did not finish: {message}"),
        ),
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
    ApiResponse::failed(
        StatusCode::NOT_FOUND,
        "git controls are not enabled — set 'serve.git_controls: true' in config.yaml and restart"
            .to_owned(),
    )
}

/// The POSTs here are covered by the same-origin layer over the whole
/// API ([`crate::origin::guard_mutations`]). Two *reads* need the same
/// guard and apply it themselves: `GET /api/git?fetch=true` contacts the
/// remote (and can invoke a credential helper), and the commit preview
/// returns file contents.
fn refuse_foreign_origin<T: serde::Serialize>(headers: &HeaderMap) -> Option<ApiResponse<T>> {
    crate::origin::is_foreign_origin(headers).then(crate::origin::refusal)
}

const NOT_A_REPOSITORY: &str = "the project is not inside a git repository";

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
    read_local(state)?.ok_or_else(|| refuse(NOT_A_REPOSITORY))
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
    respond(blocking(move || Ok(status_inner(&state, query.fetch)?)).await)
}

fn status_inner(state: &AppState, with_fetch: bool) -> Result<GitStatus, GitError> {
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

fn pull_inner(state: &AppState) -> Result<GitPullResult, GitActionError> {
    let root = &state.project_root;
    let local = require_local(state)?;
    // Refusals about repository state, checked before any network:
    // a rebase already underway is the *user's* (possibly mid-conflict
    // in a terminal) and must never be aborted from here; uncommitted
    // changes never get stashed or rebased over from a browser button.
    if git::rebase_in_progress(root)? {
        return Err(refuse(
            "a rebase is in progress in this repository — finish or abort it in a terminal first",
        ));
    }
    if !local.snapshot.dirty.is_empty() {
        // Uncommitted work anywhere blocks the pull — but the pill only
        // shows the in-scope part, so when everything dirty is *outside*
        // the workdown paths the message has to name it: the user sees
        // a clean pill and needs to know what the repository sees.
        let message = if local.in_scope.is_empty() {
            format!(
                "there are uncommitted changes outside the workdown paths ({}) — pull never touches uncommitted work; commit or stash them in a terminal first",
                local.outside_scope().join(", ")
            )
        } else {
            "there are uncommitted changes — pull never touches uncommitted work; commit first"
                .to_owned()
        };
        return Err(refuse(message));
    }
    // Fetch first, then read `behind`: after the fetch it is exactly
    // the number of commits the pull will integrate — the same number
    // the pill showed. (Measuring the tracking ref's movement *during*
    // the pull instead would report "already up to date" whenever an
    // earlier status call had already fetched.)
    let fetched = git::fetch(root)?;
    if !fetched.success {
        return Err(GitActionError::RemoteUnreachable(format!(
            "fetch failed: {}",
            fetched.stderr.trim()
        )));
    }
    let current = require_local(state)?;
    if current.snapshot.behind == 0 {
        return Ok(GitPullResult {
            pulled_commits: 0,
            status: current.into_status(None),
        });
    }
    let pulled_commits = current.snapshot.behind;
    let pulled = match git::pull(root) {
        Ok(output) => output,
        Err(error) => {
            // A timeout kills git mid-operation and can leave its rebase
            // in progress. No rebase was underway before this request
            // (checked above), so an abort here only ever backs out ours.
            let _ = git::abort_rebase(root);
            return Err(error.into());
        }
    };
    if !pulled.success {
        // Same reasoning: this rebase is ours, back it out so the
        // browser never strands the repository mid-rebase.
        let _ = git::abort_rebase(root);
        return Err(refuse(format!("pull failed: {}", pulled.stderr.trim())));
    }
    // The changed files also wake the file watcher, whose ping makes
    // every open tab refetch its view — no manual event needed here.
    Ok(GitPullResult {
        pulled_commits,
        status: fresh_status(state)?,
    })
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

/// Push, or — on a branch with no upstream — *publish*: the same
/// gesture from the user's side ("get my commits onto the remote"),
/// with git's first-time bookkeeping (create the remote branch, record
/// it as upstream) handled here instead of in a terminal. The server
/// decides which from the repository's present state, not from what
/// the pill believed when it was clicked.
fn push_inner(state: &AppState) -> Result<GitPushResult, GitActionError> {
    let root = &state.project_root;
    let Some(snapshot) = git::snapshot(root)? else {
        return Err(refuse(NOT_A_REPOSITORY));
    };
    match attempt_push(root, &snapshot)? {
        PushAttempt::Pushed { published } => Ok(GitPushResult {
            published,
            status: fresh_status(state)?,
        }),
        PushAttempt::Refused(reason) => Err(refuse(reason)),
        PushAttempt::Rejected { reason, details } => Err(refuse(format!("{reason}: {details}"))),
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
fn attempt_push(root: &Path, local: &RepoSnapshot) -> Result<PushAttempt, GitError> {
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
        let Some(remote) = git::publish_remote(root)? else {
            return Ok(PushAttempt::Refused(
                "no remote to publish to — add one (or set remote.pushDefault) in a terminal"
                    .to_owned(),
            ));
        };
        git::publish(root, &remote, &local.branch)?
    } else {
        git::push(root)?
    };
    if !pushed.success {
        return Ok(PushAttempt::Rejected {
            reason: format!("{} failed", if published { "publish" } else { "push" }),
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
        return Err(refuse(NOT_A_REPOSITORY));
    };
    preview::build_preview(&state.project_root, &scope, &state.config, schema)?
        .ok_or_else(|| refuse(NOT_A_REPOSITORY))
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
        return Err(refuse(
            "detached HEAD — a commit here has no branch to push; check one out in a terminal first",
        ));
    }
    if git::rebase_in_progress(root)? {
        return Err(refuse(
            "a rebase is in progress in this repository — finish or abort it in a terminal first",
        ));
    }
    let message = request.message.trim();
    if message.is_empty() {
        return Err(refuse("the commit message is empty"));
    }
    if local.in_scope.is_empty() {
        return Err(refuse("nothing to commit — no workdown files have changed"));
    }
    ensure_confirmed_set(&local, &request.files)?;
    ensure_nothing_conflicted(&local)?;
    if git::identity(root)?.is_none() {
        return Err(refuse(
            "git has no identity to commit as — set user.name and user.email in a terminal first",
        ));
    }

    let pathspecs = confirmed_pathspecs(&local);
    let staged = git::stage(root, &pathspecs)?;
    if !staged.success {
        return Err(refuse(with_details(
            "the changes could not be staged",
            &staged.stderr,
        )));
    }
    let committed = git::commit(root, message, &pathspecs)?;
    if !committed.success {
        // Leave the index as the user had it: our staging is undone,
        // their working tree was never touched.
        let _ = git::unstage(root, &pathspecs);
        return Err(refuse(commit_failure(&committed)));
    }
    let commit = git::head_short_hash(root)?;

    let pull = pull_step(state, local.snapshot.has_upstream)?;
    let push = push_step(root, &pull)?;
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
        return Err(refuse(
            "the set of changed files has moved since the preview — review the new list before committing",
        ));
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
    Err(refuse(format!(
        "{} still carr{} conflict markers — resolve in a terminal first",
        conflicted.join(", "),
        if conflicted.len() == 1 { "ies" } else { "y" }
    )))
}

/// The pathspecs to stage and commit: exactly the files git listed for
/// the scope — the set the user just confirmed — as literal paths.
/// `git add` (unlike `git status`) refuses a pathspec that matches
/// nothing, which a scope entry for a file the project does not have
/// (no resources.yaml, say) would be; the changed files themselves
/// always exist on one side. A rename contributes both of its paths.
fn confirmed_pathspecs(local: &LocalState) -> Vec<String> {
    local
        .in_scope
        .iter()
        .flat_map(|change| {
            let mut paths = vec![git::literal_pathspec(&change.path)];
            if let ChangeState::Renamed { from } = &change.state {
                paths.push(git::literal_pathspec(from));
            }
            paths
        })
        .collect()
}

/// The pull step of a commit: skipped without an upstream (publish
/// follows) or when not behind after a fetch; otherwise `pull --rebase`,
/// which the whole-repository dirty rule still guards — a failed rebase
/// is aborted so the repository is never left mid-rebase from here.
fn pull_step(state: &AppState, has_upstream: bool) -> Result<GitPullStep, GitError> {
    let root = &state.project_root;
    if !has_upstream {
        return Ok(GitPullStep::Skipped {
            reason: "not published yet — the push step publishes the branch".to_owned(),
        });
    }
    let stopped =
        |reason: String, details: Option<String>| Ok(GitPullStep::Stopped { reason, details });
    let fetched = git::fetch(root)?;
    if !fetched.success {
        return stopped(
            "the remote could not be reached, so the branch could not be brought up to date — the commit is safe and local; press Push once the remote is back".to_owned(),
            Some(fetched.stderr.trim().to_owned()),
        );
    }
    let Some(current) = read_local(state)? else {
        return stopped(
            "the project is no longer inside a git repository".to_owned(),
            None,
        );
    };
    if current.snapshot.behind == 0 {
        return Ok(GitPullStep::Skipped {
            reason: "nothing to integrate — the branch is not behind".to_owned(),
        });
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
    let pulled = match git::pull(root) {
        Ok(output) => output,
        Err(error) => {
            // Same reasoning as the pull endpoint: no rebase was underway
            // before this request, so an abort only backs out ours.
            let _ = git::abort_rebase(root);
            return stopped(
                "the pull was interrupted — the commit is safe and local; pull in a terminal, then press Push".to_owned(),
                Some(error.to_string()),
            );
        }
    };
    if !pulled.success {
        let _ = git::abort_rebase(root);
        let raw = format!("{}\n{}", pulled.stdout.trim(), pulled.stderr.trim());
        return stopped(conflict_wording(&raw), Some(raw.trim().to_owned()));
    }
    Ok(GitPullStep::Pulled { commits: behind })
}

/// The push step of a commit: not attempted when the pull stopped;
/// otherwise the same push-or-publish the Push button runs, worded for
/// the checklist.
fn push_step(root: &Path, pull: &GitPullStep) -> Result<GitPushStep, GitError> {
    if let GitPullStep::Stopped { .. } = pull {
        return Ok(GitPushStep::Skipped {
            reason: "not attempted — the pull did not complete".to_owned(),
        });
    }
    let Some(snapshot) = git::snapshot(root)? else {
        return Ok(GitPushStep::Stopped {
            reason: "the project is no longer inside a git repository".to_owned(),
            details: None,
        });
    };
    Ok(match attempt_push(root, &snapshot)? {
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
    })
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
/// A missing identity never reaches here: it is checked before staging.
fn commit_failure(output: &git::GitOutput) -> String {
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
