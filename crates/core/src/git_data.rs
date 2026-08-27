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
        /// Whether the branch tracks an upstream; without one, push
        /// can't succeed and ahead/behind are meaningless zeros.
        has_upstream: bool,
        /// Commits on the branch that the upstream doesn't have.
        ahead: u32,
        /// Commits on the upstream that the branch doesn't have — as of
        /// the last fetch; only `?fetch=true` contacts the remote.
        behind: u32,
        /// Files with uncommitted changes (staged, unstaged, or
        /// untracked) — `git status --porcelain` line count.
        dirty_count: u32,
    },
}
