//! Shelling out to the `git` CLI for the sync endpoints.
//!
//! Deliberately the CLI and not a git library: the user's own `git`
//! carries their credential setup (Git Credential Manager on Windows,
//! the keychain on macOS), so pull and push authenticate exactly like
//! the terminal does, with nothing to configure. Every invocation is
//! non-interactive — `GIT_TERMINAL_PROMPT=0` and `GCM_INTERACTIVE=never`
//! make a missing credential fail fast instead of hanging a request on
//! a prompt nobody can see — and bounded by a timeout.

use std::path::{Path, PathBuf};
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
/// an error — errors are reserved for git being unrunnable, or for
/// commands that have no meaningful failure mode for the caller.
pub struct GitOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Why a git invocation produced no usable answer.
#[derive(Debug)]
pub enum GitError {
    /// `git` couldn't be spawned — not installed, not on PATH.
    Spawn(std::io::Error),
    /// The command outlived its timeout and was killed.
    TimedOut,
    /// git ran but refused, on a command whose failure the caller has
    /// no better answer for than reporting it (`status` on a repo that
    /// exists, `rev-parse` for the git directory). Pull and push
    /// interpret their own non-zero exits instead — those are results.
    Failed { command: String, stderr: String },
}

impl std::fmt::Display for GitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GitError::Spawn(error) => write!(formatter, "could not run git: {error}"),
            GitError::TimedOut => write!(formatter, "git took too long and was stopped"),
            GitError::Failed { command, stderr } => {
                write!(formatter, "git {command} failed: {}", stderr.trim())
            }
        }
    }
}

/// Run `git -C <root> <args…>` to completion, capturing output.
///
/// `LC_ALL=C` pins git's messages to English so the few places that
/// match on them (not-a-repository detection) hold on localized
/// systems.
pub async fn run(root: &Path, args: &[&str], timeout: Duration) -> Result<GitOutput, GitError> {
    let child = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never")
        .env("LC_ALL", "C")
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

/// Update remote-tracking refs so ahead/behind reflect the remote's
/// present, not its past. Changes no local files.
pub async fn fetch(root: &Path) -> Result<GitOutput, GitError> {
    run(root, &["fetch", "--quiet"], NETWORK_TIMEOUT).await
}

/// Integrate remote commits into the local branch. `--rebase` keeps
/// the item history linear. No `--autostash`: the pull endpoint
/// refuses to run over uncommitted changes, so there is never a stash
/// whose failed reapply could scatter conflict markers into item files
/// behind a "success" answer.
pub async fn pull(root: &Path) -> Result<GitOutput, GitError> {
    run(root, &["pull", "--rebase"], NETWORK_TIMEOUT).await
}

/// Publish local commits to the upstream. Pushes only what is already
/// committed — uncommitted edits never leave the machine, so the
/// review gate (look at the diff, then commit) stays where it is.
pub async fn push(root: &Path) -> Result<GitOutput, GitError> {
    run(root, &["push"], NETWORK_TIMEOUT).await
}

/// Back out of a rebase the *endpoint* started. Callers must know the
/// rebase is their own — the pull endpoint refuses to run while one is
/// already in progress precisely so this can never destroy a rebase
/// the user is resolving in a terminal.
pub async fn abort_rebase(root: &Path) -> Result<GitOutput, GitError> {
    run(root, &["rebase", "--abort"], LOCAL_TIMEOUT).await
}

/// The repository's git directory (usually `<repo>/.git`), or `None`
/// when `root` is not inside a git work tree.
pub async fn git_directory(root: &Path) -> Result<Option<PathBuf>, GitError> {
    let output = run(root, &["rev-parse", "--absolute-git-dir"], LOCAL_TIMEOUT).await?;
    if !output.success {
        if is_not_a_repository(&output.stderr) {
            return Ok(None);
        }
        return Err(GitError::Failed {
            command: "rev-parse --absolute-git-dir".to_owned(),
            stderr: output.stderr,
        });
    }
    Ok(Some(PathBuf::from(output.stdout.trim())))
}

/// Whether a rebase is underway in this repository — regardless of who
/// started it. Checks the two marker directories git itself uses
/// (merge-backend and apply-backend rebases).
pub async fn rebase_in_progress(root: &Path) -> Result<bool, GitError> {
    let Some(git_directory) = git_directory(root).await? else {
        return Ok(false);
    };
    Ok(git_directory.join("rebase-merge").exists() || git_directory.join("rebase-apply").exists())
}

/// The purely local half of the `Ready` status, read in one spawn.
#[derive(Debug, PartialEq, Eq)]
pub struct RepoSnapshot {
    /// Current branch name (`HEAD` when detached).
    pub branch: String,
    /// Whether the branch's upstream ref resolves; without one, push
    /// can't succeed and ahead/behind are meaningless zeros.
    pub has_upstream: bool,
    pub ahead: u32,
    pub behind: u32,
    pub dirty_count: u32,
}

/// Read branch, upstream, ahead/behind and the dirty count — one
/// `git status --porcelain=v2 --branch` invocation answers all of it,
/// including "not a repository" (`None`). Ahead/behind are as of the
/// last fetch; only a fetch contacts the remote.
pub async fn snapshot(root: &Path) -> Result<Option<RepoSnapshot>, GitError> {
    let output = run(
        root,
        &["status", "--porcelain=v2", "--branch"],
        LOCAL_TIMEOUT,
    )
    .await?;
    if !output.success {
        if is_not_a_repository(&output.stderr) {
            return Ok(None);
        }
        return Err(GitError::Failed {
            command: "status".to_owned(),
            stderr: output.stderr,
        });
    }
    Ok(Some(parse_porcelain_status(&output.stdout)))
}

/// The one git message this module matches on — pinned to English by
/// `LC_ALL=C` in [`run`].
fn is_not_a_repository(stderr: &str) -> bool {
    stderr.contains("not a git repository")
}

/// Parse `git status --porcelain=v2 --branch` output. Header lines
/// (`# branch.…`) carry the branch facts; every non-header line is one
/// changed, untracked, or unmerged file.
///
/// - `# branch.head <name>` — `(detached)` maps to `HEAD`, matching
///   the wire contract for a detached head. On an unborn branch (fresh
///   `git init`, nothing committed) this still names the real branch.
/// - `# branch.ab +<ahead> -<behind>` — present exactly when the
///   upstream ref resolves, which is what `has_upstream` means.
fn parse_porcelain_status(stdout: &str) -> RepoSnapshot {
    let mut branch = String::from("HEAD");
    let mut has_upstream = false;
    let mut ahead = 0;
    let mut behind = 0;
    let mut dirty_count = 0;

    for line in stdout.lines() {
        if let Some(name) = line.strip_prefix("# branch.head ") {
            if name != "(detached)" {
                branch = name.to_owned();
            }
        } else if let Some(counts) = line.strip_prefix("# branch.ab ") {
            has_upstream = true;
            for count in counts.split_whitespace() {
                if let Some(value) = count.strip_prefix('+') {
                    ahead = value.parse().unwrap_or(0);
                } else if let Some(value) = count.strip_prefix('-') {
                    behind = value.parse().unwrap_or(0);
                }
            }
        } else if !line.starts_with('#') && !line.is_empty() {
            dirty_count += 1;
        }
    }

    RepoSnapshot {
        branch,
        has_upstream,
        ahead,
        behind,
        dirty_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_synced_branch_with_upstream() {
        let stdout = "\
# branch.oid 1234567890abcdef1234567890abcdef12345678
# branch.head main
# branch.upstream origin/main
# branch.ab +0 -0
";
        assert_eq!(
            parse_porcelain_status(stdout),
            RepoSnapshot {
                branch: "main".into(),
                has_upstream: true,
                ahead: 0,
                behind: 0,
                dirty_count: 0,
            }
        );
    }

    #[test]
    fn parses_ahead_behind_and_dirty_files() {
        let stdout = "\
# branch.oid 1234567890abcdef1234567890abcdef12345678
# branch.head feature
# branch.upstream origin/feature
# branch.ab +2 -3
1 .M N... 100644 100644 100644 0123456 0123456 workdown-items/item-a.md
? workdown-items/item-d.md
";
        assert_eq!(
            parse_porcelain_status(stdout),
            RepoSnapshot {
                branch: "feature".into(),
                has_upstream: true,
                ahead: 2,
                behind: 3,
                dirty_count: 2,
            }
        );
    }

    #[test]
    fn no_upstream_line_means_no_upstream() {
        // An unborn branch (fresh init) and a branch with an unresolvable
        // upstream both omit `branch.ab` — either way push has nowhere to
        // go and ahead/behind carry no information.
        let stdout = "\
# branch.oid (initial)
# branch.head main
? workdown-items/item-a.md
";
        assert_eq!(
            parse_porcelain_status(stdout),
            RepoSnapshot {
                branch: "main".into(),
                has_upstream: false,
                ahead: 0,
                behind: 0,
                dirty_count: 1,
            }
        );
    }

    #[test]
    fn detached_head_reads_as_head() {
        let stdout = "\
# branch.oid 1234567890abcdef1234567890abcdef12345678
# branch.head (detached)
";
        assert_eq!(parse_porcelain_status(stdout).branch, "HEAD");
    }
}
