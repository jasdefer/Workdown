//! Wire contracts for the git sync endpoints (`GET /api/git`,
//! `GET /api/git/commit-preview`, `POST /api/git/commit`,
//! `POST /api/git/pull`, `POST /api/git/push`).
//!
//! The server shells out to the `git` CLI; these types are only the
//! projection the browser sees. Like the timer contracts, they live in
//! core so `cargo xtask gen-types` can emit the TypeScript bindings.

use serde::{Deserialize, Serialize};

/// `POST /api/git/commit` — the confirmed gesture behind the dialog:
/// stage the workdown paths, commit, pull if behind, push.
#[derive(Debug, Clone, Deserialize, ts_rs::TS)]
pub struct GitCommitRequest {
    /// The commit message as confirmed — the generated one, edited or
    /// not. Subject line, then a blank line and the body.
    pub message: String,
    /// The project-relative paths the dialog showed. The server refuses
    /// when the in-scope set differs by the time the request arrives:
    /// the dialog is the review moment, and committing files the user
    /// did not see defeats it.
    pub files: Vec<String>,
}

/// What one press of "Commit & push" did, step by step. The commit
/// itself succeeded whenever this is returned; pull and push report
/// their own outcome so the dialog can render the three as a checklist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
pub struct GitCommitResult {
    /// Short hash of the new commit.
    pub commit: String,
    pub pull: GitPullStep,
    pub push: GitPushStep,
    pub status: GitStatus,
}

/// The pull step of a commit-and-push.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum GitPullStep {
    /// Nothing to integrate: the branch was not behind, or has no
    /// upstream yet (the push step publishes it).
    Skipped,
    Pulled {
        commits: u32,
    },
    /// The pull could not run or did not complete. The commit is intact
    /// and local; `reason` says so and what to do next, `details` is
    /// git's own output when there is any.
    Stopped {
        reason: String,
        details: Option<String>,
    },
}

/// The push step of a commit-and-push.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum GitPushStep {
    /// Pushed to the upstream, or — `published` — created the remote
    /// branch and recorded it as upstream.
    Pushed { published: bool },
    /// Not attempted, because the pull step stopped.
    Skipped { reason: String },
    /// Attempted and refused or rejected; the commit stays local.
    Stopped {
        reason: String,
        details: Option<String>,
    },
}

/// What a pull accomplished: how many commits came in (0 means the
/// branch was already up to date — the toast says so instead of
/// claiming a pull that changed nothing), plus the fresh status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
pub struct GitPullResult {
    pub pulled_commits: u32,
    pub status: GitStatus,
}

/// What a push accomplished: whether it *published* the branch (first
/// push, upstream created and recorded) or pushed to an existing
/// upstream — the toast says which — plus the fresh status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
pub struct GitPushResult {
    pub published: bool,
    pub status: GitStatus,
}

/// What a commit from the web app would do right now — the review the
/// confirmation dialog shows before anything is staged. Read-only and
/// repeatable; the confirmation sends `files` back so the server can
/// refuse when the set has moved in the meantime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
pub struct GitCommitPreview {
    /// The in-scope files the commit would cover, in git's order.
    pub files: Vec<GitChangedFile>,
    /// The generated commit message — subject, and a body after a blank
    /// line when the subject cannot carry it all. The dialog's editable
    /// starting point.
    pub message: String,
    /// Uncommitted files outside the workdown paths, repository-relative.
    /// Never committed from here, but named: a pull after the commit
    /// refuses over them, and the pill does not show them.
    pub outside: Vec<String>,
}

/// One file a commit would cover.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
pub struct GitChangedFile {
    /// Project-relative path, forward slashes.
    pub path: String,
    /// The config key the file falls under, as the pill names it:
    /// `items`, `schema`, `views`, `resources`, `templates`, `config`.
    pub role: String,
    pub change: GitChangeKind,
}

/// What happened to a file, as `git status` sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum GitChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    /// Left conflicted by a merge or rebase — git will not commit it.
    Unmerged,
}

/// What the git controls should show — one tagged state per situation
/// the widget must distinguish. `Disabled` (the default) keeps the
/// widget entirely hidden; nothing else about the repository is
/// revealed unless a project opted in via `serve.git_controls`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum GitStatus {
    /// `serve.git_controls` is not enabled for this project.
    Disabled,
    /// Enabled, but the project directory is not inside a git work
    /// tree — nothing to pull or push.
    NotARepo,
    /// Enabled and inside a repository: the numbers the widget shows.
    Ready {
        /// Current branch name (`HEAD` when detached).
        branch: String,
        /// Whether the branch tracks an upstream. Without one the
        /// branch is unpublished: ahead/behind are meaningless zeros,
        /// pull has nowhere to pull from, and push *publishes* (creates
        /// the remote branch and records it as upstream).
        has_upstream: bool,
        /// Commits on the branch that the upstream doesn't have.
        ahead: u32,
        /// Commits on the upstream that the branch doesn't have — as of
        /// the last fetch; only `?fetch=true` contacts the remote.
        behind: u32,
        /// Work items with uncommitted changes (edited, added, or
        /// deleted) — files under the config's `paths.work_items`.
        /// Counted over the workdown paths only: a source change sitting
        /// next to the items in a code repository is not the pill's
        /// business and does not appear here.
        dirty_items: u32,
        /// Definition files with uncommitted changes, named by role in
        /// a fixed order (`schema`, `views`, `resources`, `templates`,
        /// `config`) rather than by filename. Empty when none changed.
        dirty_definitions: Vec<String>,
        /// Why the requested remote contact failed, when it did — the
        /// local numbers above are still served (`behind` is then as of
        /// the last successful fetch). `None` when the fetch succeeded
        /// or none was requested; the client keeps the last attempt's
        /// answer across local-only refreshes.
        fetch_error: Option<String>,
    },
}
