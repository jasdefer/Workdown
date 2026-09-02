//! Wire contracts for the git sync endpoints (`GET /api/git`,
//! `POST /api/git/pull`, `POST /api/git/push`).
//!
//! The server shells out to the `git` CLI; these types are only the
//! projection the browser sees. Like the timer contracts, they live in
//! core so `cargo xtask gen-types` can emit the TypeScript bindings.

use serde::Serialize;

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
        /// Files with uncommitted changes (staged, unstaged, or
        /// untracked) — `git status --porcelain` line count.
        dirty_count: u32,
        /// Why the requested remote contact failed, when it did — the
        /// local numbers above are still served (`behind` is then as of
        /// the last successful fetch). `None` when the fetch succeeded
        /// or none was requested; the client keeps the last attempt's
        /// answer across local-only refreshes.
        fetch_error: Option<String>,
    },
}
