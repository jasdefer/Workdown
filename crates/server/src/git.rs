//! Shelling out to the `git` CLI for the sync endpoints.
//!
//! Deliberately the CLI and not a git library: the user's own `git`
//! carries their credential setup (Git Credential Manager on Windows,
//! the keychain on macOS), so pull and push authenticate exactly like
//! the terminal does, with nothing to configure. Every invocation is
//! non-interactive — `GIT_TERMINAL_PROMPT=0` and `GCM_INTERACTIVE=never`
//! make a missing credential fail fast instead of hanging a request on
//! a prompt nobody can see — and bounded by a timeout.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;

/// Bound for purely local plumbing (`rev-parse`, `status`).
const LOCAL_TIMEOUT: Duration = Duration::from_secs(10);

/// Bound for anything that talks to the remote (`fetch`, `pull`,
/// `push`) — generous, but a hung credential helper or a dead VPN
/// must not pin a request forever.
const NETWORK_TIMEOUT: Duration = Duration::from_secs(120);

/// A finished git invocation: exit success plus captured output. A
/// non-zero exit is a *result* (the caller decides what it means), not
/// an error — errors are reserved for git being unrunnable.
pub struct GitOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Why a git invocation produced no result at all.
#[derive(Debug)]
pub enum GitError {
    /// `git` couldn't be spawned — not installed, not on PATH.
    Spawn(std::io::Error),
    /// The command outlived its timeout and was killed.
    TimedOut,
}

impl std::fmt::Display for GitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GitError::Spawn(error) => write!(formatter, "could not run git: {error}"),
            GitError::TimedOut => write!(formatter, "git took too long and was stopped"),
        }
    }
}

/// Run `git -C <root> <args…>` to completion, capturing output.
pub async fn run(root: &Path, args: &[&str], timeout: Duration) -> Result<GitOutput, GitError> {
    let child = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output();

    let output = match tokio::time::timeout(timeout, child).await {
        Err(_elapsed) => return Err(GitError::TimedOut),
        Ok(Err(error)) => return Err(GitError::Spawn(error)),
        Ok(Ok(output)) => output,
    };

    Ok(GitOutput {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

/// Whether `root` sits inside a git work tree.
pub async fn is_work_tree(root: &Path) -> Result<bool, GitError> {
    let output = run(root, &["rev-parse", "--is-inside-work-tree"], LOCAL_TIMEOUT).await?;
    Ok(output.success && output.stdout.trim() == "true")
}

/// Update remote-tracking refs so ahead/behind reflect the remote's
/// present, not its past. Changes no local files.
pub async fn fetch(root: &Path) -> Result<GitOutput, GitError> {
    run(root, &["fetch", "--quiet"], NETWORK_TIMEOUT).await
}

/// Integrate remote commits into the local branch. `--rebase` keeps
/// the item history linear; `--autostash` lets a pull proceed over
/// uncommitted local edits (they're reapplied afterwards). On conflict
/// the caller is expected to abort the rebase — see [`abort_rebase`].
pub async fn pull(root: &Path) -> Result<GitOutput, GitError> {
    run(root, &["pull", "--rebase", "--autostash"], NETWORK_TIMEOUT).await
}

/// Publish local commits to the upstream. Pushes only what is already
/// committed — uncommitted edits never leave the machine, so the
/// review gate (look at the diff, then commit) stays where it is.
pub async fn push(root: &Path) -> Result<GitOutput, GitError> {
    run(root, &["push"], NETWORK_TIMEOUT).await
}

/// The commit the upstream tracking ref points at, or `None` when the
/// branch has no upstream (or it doesn't resolve yet).
pub async fn upstream_commit(root: &Path) -> Result<Option<String>, GitError> {
    let output = run(root, &["rev-parse", "@{upstream}"], LOCAL_TIMEOUT).await?;
    Ok(output
        .success
        .then(|| output.stdout.trim().to_owned())
        .filter(|hash| !hash.is_empty()))
}

/// How many commits the upstream gained since `old_upstream` — what a
/// pull actually brought in. Deliberately measured on the tracking ref,
/// not on `HEAD`: a rebasing pull rewrites local commits, and an
/// old-HEAD..HEAD count would include those rewrites as if they had
/// been pulled. `None` for the old tip means the upstream is new, so
/// everything reachable from it counts.
pub async fn upstream_commits_since(
    root: &Path,
    old_upstream: Option<&str>,
) -> Result<u32, GitError> {
    let range = match old_upstream {
        Some(old) => format!("{old}..@{{upstream}}"),
        None => "@{upstream}".to_owned(),
    };
    let output = run(root, &["rev-list", "--count", &range], LOCAL_TIMEOUT).await?;
    Ok(output.stdout.trim().parse().unwrap_or(0))
}

/// Back out of a failed rebase, restoring the pre-pull state. Callers
/// treat this as best-effort: when the pull failed before the rebase
/// even started (network down, no upstream) there is nothing to abort
/// and git's refusal is expected.
pub async fn abort_rebase(root: &Path) -> Result<GitOutput, GitError> {
    run(root, &["rebase", "--abort"], LOCAL_TIMEOUT).await
}

/// Collect the `Ready` projection for a repository at `root` — purely
/// local reads; a fetch (when requested) happens before this.
pub async fn collect_status(root: &Path) -> Result<workdown_core::git_data::GitStatus, GitError> {
    let branch = run(root, &["rev-parse", "--abbrev-ref", "HEAD"], LOCAL_TIMEOUT)
        .await?
        .stdout
        .trim()
        .to_owned();

    let upstream = run(
        root,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ],
        LOCAL_TIMEOUT,
    )
    .await?;
    let has_upstream = upstream.success;

    let (behind, ahead) = if has_upstream {
        let counts = run(
            root,
            &["rev-list", "--left-right", "--count", "@{upstream}...HEAD"],
            LOCAL_TIMEOUT,
        )
        .await?;
        parse_left_right_count(&counts.stdout).unwrap_or((0, 0))
    } else {
        (0, 0)
    };

    let porcelain = run(root, &["status", "--porcelain"], LOCAL_TIMEOUT).await?;
    let dirty_count = porcelain.stdout.lines().filter(|l| !l.is_empty()).count() as u32;

    Ok(workdown_core::git_data::GitStatus::Ready {
        branch,
        has_upstream,
        ahead,
        behind,
        dirty_count,
    })
}

/// Parse `git rev-list --left-right --count A...B` output — two
/// tab-separated numbers: commits only in A (left), commits only in B
/// (right).
fn parse_left_right_count(stdout: &str) -> Option<(u32, u32)> {
    let mut parts = stdout.split_whitespace();
    let left = parts.next()?.parse().ok()?;
    let right = parts.next()?.parse().ok()?;
    Some((left, right))
}
