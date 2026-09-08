//! Workdown's git layer: the user's own `git`, run as a child process,
//! limited to the workdown paths.
//!
//! Deliberately the CLI and not a git library: the user's own `git`
//! carries their credential setup (Git Credential Manager on Windows,
//! the keychain on macOS), so pull and push authenticate exactly like
//! the terminal does, with nothing to configure. Every invocation is
//! non-interactive — `GIT_TERMINAL_PROMPT=0` and `GCM_INTERACTIVE=never`
//! make a missing credential fail fast instead of hanging a request on
//! a prompt nobody can see — and bounded by a timeout.
//!
//! Synchronous on purpose. Waiting on an external process is blocking
//! work however it is dressed; the CLI calls these functions directly,
//! and the server runs each git action on a blocking thread. This crate
//! is the one place git is spawned — the server's `/api/git` handlers,
//! `workdown changes` and `workdown install-hooks` all come here.
//!
//! The crate root holds the git commands themselves. [`scope`] decides
//! which paths the git controls may touch; [`preview`] turns the changes
//! inside that scope into the file list and commit message the dialog
//! and `workdown changes` show.

pub mod preview;
pub mod scope;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Bound for purely local plumbing (`rev-parse`, `status`).
const LOCAL_TIMEOUT: Duration = Duration::from_secs(10);

/// Bound for anything that talks to the remote (`fetch`, `pull`,
/// `push`) — generous, but a hung credential helper or a dead VPN
/// must not pin a request forever.
const NETWORK_TIMEOUT: Duration = Duration::from_secs(120);

/// Bound for a commit: hooks run inside it (a pre-commit hook that
/// re-renders views takes a moment), so longer than plumbing, shorter
/// than the network.
const COMMIT_TIMEOUT: Duration = Duration::from_secs(60);

/// How often [`run`] checks whether the child has exited while waiting
/// for it. Short enough that plumbing calls add no noticeable latency.
const EXIT_POLL_INTERVAL: Duration = Duration::from_millis(5);

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

impl std::error::Error for GitError {}

/// Run `git -C <root> <args…>` to completion, capturing output.
///
/// `LC_ALL=C` pins git's messages to English so the few places that
/// match on them (not-a-repository detection) hold on localized
/// systems.
///
/// The child is killed when `timeout` passes. Both pipes are drained on
/// their own threads from the start, because a child that fills one pipe
/// while this thread waits on the other would never exit.
pub fn run(root: &Path, args: &[&str], timeout: Duration) -> Result<GitOutput, GitError> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(GitError::Spawn)?;
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(EXIT_POLL_INTERVAL),
            Ok(None) => {
                // Killing closes the pipes, which ends the drain threads.
                let _ = child.kill();
                let _ = child.wait();
                return Err(GitError::TimedOut);
            }
            Err(error) => return Err(GitError::Spawn(error)),
        }
    };

    Ok(GitOutput {
        success: status.success(),
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

/// Read a child's pipe to its end on a thread of its own, as text.
fn drain<Pipe: Read + Send + 'static>(pipe: Option<Pipe>) -> JoinHandle<String> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

/// The one git message this crate matches on — pinned to English by
/// `LC_ALL=C` in [`run`].
fn is_not_a_repository(stderr: &str) -> bool {
    stderr.contains("not a git repository")
}

/// One `git rev-parse <flag>` answer, trimmed. `None` when `root` is not
/// inside a git work tree; any other refusal is an error, since
/// `rev-parse` has no failure mode the caller can interpret.
fn rev_parse(root: &Path, flag: &str) -> Result<Option<String>, GitError> {
    let output = run(root, &["rev-parse", flag], LOCAL_TIMEOUT)?;
    if !output.success {
        if is_not_a_repository(&output.stderr) {
            return Ok(None);
        }
        return Err(GitError::Failed {
            command: format!("rev-parse {flag}"),
            stderr: output.stderr,
        });
    }
    Ok(Some(output.stdout.trim().to_owned()))
}

// ── Remote ──────────────────────────────────────────────────────────

/// Update remote-tracking refs so ahead/behind reflect the remote's
/// present, not its past. Changes no local files.
pub fn fetch(root: &Path) -> Result<GitOutput, GitError> {
    run(root, &["fetch", "--quiet"], NETWORK_TIMEOUT)
}

/// Integrate remote commits into the local branch. `--rebase` keeps
/// the item history linear. No `--autostash`: the pull endpoint
/// refuses to run over uncommitted changes, so there is never a stash
/// whose failed reapply could scatter conflict markers into item files
/// behind a "success" answer.
pub fn pull(root: &Path) -> Result<GitOutput, GitError> {
    run(root, &["pull", "--rebase"], NETWORK_TIMEOUT)
}

/// Publish local commits to the upstream. Pushes only what is already
/// committed — uncommitted edits never leave the machine, so the
/// review gate (look at the diff, then commit) stays where it is.
pub fn push(root: &Path) -> Result<GitOutput, GitError> {
    run(root, &["push"], NETWORK_TIMEOUT)
}

/// First push of a branch that has no upstream yet: create it on
/// `remote` and record that as the upstream (`-u`), so the next status
/// has real ahead/behind numbers and later pushes are plain `push`.
pub fn publish(root: &Path, remote: &str, branch: &str) -> Result<GitOutput, GitError> {
    run(root, &["push", "-u", remote, branch], NETWORK_TIMEOUT)
}

/// The remote a first publish goes to, by a rule one step friendlier
/// than bare `git push`: `remote.pushDefault` when it names an existing
/// remote; otherwise the only remote when there is exactly one (the
/// step git itself never takes, editors do); otherwise `origin` if it
/// exists. `None` when nothing applies — several remotes and no
/// default is the terminal's job.
///
/// One spawn: `git config --get-regexp '^remote\.'` lists every
/// remote's settings and the push default together.
pub fn publish_remote(root: &Path) -> Result<Option<String>, GitError> {
    let output = run(
        root,
        &["config", "--get-regexp", "^remote\\."],
        LOCAL_TIMEOUT,
    )?;
    // `--get-regexp` exits 1 when nothing matches — a repository with
    // no remotes at all, which is an answer, not a failure.
    if !output.success && !output.stderr.trim().is_empty() {
        return Err(GitError::Failed {
            command: "config --get-regexp ^remote.".to_owned(),
            stderr: output.stderr,
        });
    }
    Ok(resolve_publish_remote(&output.stdout))
}

/// The resolution rule behind [`publish_remote`], on the `key value`
/// lines `git config --get-regexp` prints.
fn resolve_publish_remote(config_lines: &str) -> Option<String> {
    let mut push_default = None;
    let mut remotes: Vec<String> = Vec::new();
    for line in config_lines.lines() {
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        if key == "remote.pushDefault" || key == "remote.pushdefault" {
            push_default = Some(value.trim().to_owned());
            continue;
        }
        // `remote.<name>.url` — the one key every remote has. Names may
        // contain dots, so peel the fixed prefix and suffix rather than
        // splitting on every dot.
        if let Some(name) = key
            .strip_prefix("remote.")
            .and_then(|rest| rest.strip_suffix(".url"))
        {
            if !remotes.iter().any(|known| known.as_str() == name) {
                remotes.push(name.to_owned());
            }
        }
    }
    if let Some(default) = push_default {
        if remotes.contains(&default) {
            return Some(default);
        }
    }
    if remotes.len() == 1 {
        return remotes.pop();
    }
    remotes.into_iter().find(|name| name == "origin")
}

// ── Repository facts ────────────────────────────────────────────────

/// Back out of a rebase the *caller* started. Callers must know the
/// rebase is their own — the pull endpoint refuses to run while one is
/// already in progress precisely so this can never destroy a rebase
/// the user is resolving in a terminal.
pub fn abort_rebase(root: &Path) -> Result<GitOutput, GitError> {
    run(root, &["rebase", "--abort"], LOCAL_TIMEOUT)
}

/// The repository's git directory (usually `<repo>/.git`), or `None`
/// when `root` is not inside a git work tree.
pub fn git_directory(root: &Path) -> Result<Option<PathBuf>, GitError> {
    Ok(rev_parse(root, "--absolute-git-dir")?.map(PathBuf::from))
}

/// Where git runs hooks from — `core.hooksPath` honoured, gitfile work
/// trees resolved, which is why this asks git instead of assuming
/// `.git/hooks`. Absolute. `None` when `root` is not inside a git work
/// tree.
pub fn hooks_directory(root: &Path) -> Result<Option<PathBuf>, GitError> {
    let output = run(root, &["rev-parse", "--git-path", "hooks"], LOCAL_TIMEOUT)?;
    if !output.success {
        if is_not_a_repository(&output.stderr) {
            return Ok(None);
        }
        return Err(GitError::Failed {
            command: "rev-parse --git-path hooks".to_owned(),
            stderr: output.stderr,
        });
    }
    // `--git-path` answers relative to the directory git ran in when
    // the hooks live inside the repository, absolute otherwise.
    let path = PathBuf::from(output.stdout.trim());
    Ok(Some(if path.is_absolute() {
        path
    } else {
        root.join(path)
    }))
}

/// Whether a rebase is underway in this repository — regardless of who
/// started it. Checks the two marker directories git itself uses
/// (merge-backend and apply-backend rebases).
pub fn rebase_in_progress(root: &Path) -> Result<bool, GitError> {
    let Some(git_directory) = git_directory(root)? else {
        return Ok(false);
    };
    Ok(git_directory.join("rebase-merge").exists() || git_directory.join("rebase-apply").exists())
}

/// Where the project root sits inside the repository, as a
/// repository-relative directory: empty at the top, `sub/dir/` (with
/// git's trailing slash) below. `None` when `root` is not inside a git
/// work tree.
pub fn repository_prefix(root: &Path) -> Result<Option<String>, GitError> {
    rev_parse(root, "--show-prefix")
}

/// The absolute path of the repository's working tree root — where the
/// repository-relative paths `git status` reports are resolved against.
/// `None` when `root` is not inside a git work tree.
pub fn top_level(root: &Path) -> Result<Option<PathBuf>, GitError> {
    Ok(rev_parse(root, "--show-toplevel")?.map(PathBuf::from))
}

/// A file's text as committed at `HEAD`, by repository-relative path.
/// `None` when `HEAD` has no such file — the file is new, or the branch
/// is unborn and there is no `HEAD` at all. Read as the blob is stored,
/// so line endings are the repository's, not the working tree's; the
/// summary normalizes both sides before comparing.
pub fn show_at_head(root: &Path, repository_path: &str) -> Result<Option<String>, GitError> {
    let spec = format!("HEAD:{repository_path}");
    let output = run(root, &["show", &spec], LOCAL_TIMEOUT)?;
    if !output.success {
        return Ok(None);
    }
    Ok(Some(output.stdout))
}

/// The identity commits would be recorded under — `user.name` and
/// `user.email` — or `None` when either is unset. Never guessed: git
/// itself may invent one from the hostname, and a browser button must
/// not put a made-up author into shared history.
pub fn identity(root: &Path) -> Result<Option<(String, String)>, GitError> {
    let name = config_value(root, "user.name")?;
    let email = config_value(root, "user.email")?;
    Ok(name.zip(email))
}

/// One git config value, `None` when unset (`--get` exits 1 then).
fn config_value(root: &Path, key: &str) -> Result<Option<String>, GitError> {
    let output = run(root, &["config", "--get", key], LOCAL_TIMEOUT)?;
    if !output.success {
        return Ok(None);
    }
    let value = output.stdout.trim();
    Ok((!value.is_empty()).then(|| value.to_owned()))
}

/// The abbreviated hash of `HEAD`.
pub fn head_short_hash(root: &Path) -> Result<String, GitError> {
    let output = run(root, &["rev-parse", "--short", "HEAD"], LOCAL_TIMEOUT)?;
    if !output.success {
        return Err(GitError::Failed {
            command: "rev-parse --short HEAD".to_owned(),
            stderr: output.stderr,
        });
    }
    Ok(output.stdout.trim().to_owned())
}

// ── Staging and committing ──────────────────────────────────────────

/// A pathspec naming exactly one repository-relative path: anchored at
/// the top, no glob interpretation, so `[` or `*` in a filename cannot
/// widen it.
pub fn literal_pathspec(repository_path: &str) -> String {
    format!(":(top,literal){repository_path}")
}

/// Stage every change — modified, added, deleted — matching `pathspecs`,
/// and nothing else. Whatever the user staged outside them stays as it
/// was. Callers pass the changed files themselves (see
/// [`literal_pathspec`]): `add` refuses a pathspec that matches nothing,
/// so a directory or file the project does not have cannot be listed.
pub fn stage(root: &Path, pathspecs: &[String]) -> Result<GitOutput, GitError> {
    let mut args: Vec<&str> = vec!["add", "--all", "--"];
    args.extend(pathspecs.iter().map(String::as_str));
    run(root, &args, LOCAL_TIMEOUT)
}

/// Put the index back to `HEAD` for `pathspecs` — the undo of [`stage`]
/// after a commit that did not happen, so the button leaves no
/// half-staged state behind. Working-tree files are untouched.
pub fn unstage(root: &Path, pathspecs: &[String]) -> Result<GitOutput, GitError> {
    let mut args: Vec<&str> = vec!["reset", "--quiet", "--"];
    args.extend(pathspecs.iter().map(String::as_str));
    run(root, &args, LOCAL_TIMEOUT)
}

/// Commit the working-tree state of `pathspecs` with `message`.
/// Path-scoped on purpose: a plain `git commit` would sweep up anything
/// the user staged in a terminal; with pathspecs, git records exactly
/// these paths and leaves the rest of the index alone. Hooks run as
/// they always do and may add to the commit (a pre-commit hook that
/// re-renders and stages views, for one).
pub fn commit(root: &Path, message: &str, pathspecs: &[String]) -> Result<GitOutput, GitError> {
    let mut args: Vec<&str> = vec!["commit", "--quiet", "-m", message, "--"];
    args.extend(pathspecs.iter().map(String::as_str));
    run(root, &args, COMMIT_TIMEOUT)
}

// ── Status ──────────────────────────────────────────────────────────

/// What happened to one path in the working tree, relative to `HEAD`,
/// as `git status` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeState {
    /// Untracked, or staged as new.
    Added,
    Modified,
    Deleted,
    /// Staged rename or copy: `from` is the path at `HEAD`.
    Renamed {
        from: String,
    },
    /// Conflicted — a merge or rebase left it unresolved.
    Unmerged,
}

/// One entry of `git status`: the repository-relative path and what
/// happened to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedPath {
    pub path: String,
    pub state: ChangeState,
}

/// The purely local half of the `Ready` status, read in one spawn.
#[derive(Debug, PartialEq, Eq)]
pub struct RepoSnapshot {
    /// Current branch name (`HEAD` when detached).
    pub branch: String,
    /// Whether the branch's upstream ref resolves; without one, push
    /// can't succeed and ahead/behind are meaningless zeros.
    pub has_upstream: bool,
    /// `false` on an unborn branch (fresh `git init`, nothing committed
    /// yet) — there is nothing to publish then.
    pub has_commits: bool,
    pub ahead: u32,
    pub behind: u32,
    /// Every uncommitted change in the repository — staged, unstaged,
    /// untracked — wherever it is. The pull refusal needs the whole
    /// picture; the pill shows only the in-scope part (see
    /// [`scoped_changes`]).
    pub dirty: Vec<ChangedPath>,
}

/// Read branch, upstream, ahead/behind and the dirty files — one
/// `git status --porcelain=v2 --branch -z` invocation answers all of it,
/// including "not a repository" (`None`). Ahead/behind are as of the
/// last fetch; only a fetch contacts the remote.
pub fn snapshot(root: &Path) -> Result<Option<RepoSnapshot>, GitError> {
    let output = run(
        root,
        &[
            "status",
            "--porcelain=v2",
            "--branch",
            "-z",
            "--untracked-files=all",
        ],
        LOCAL_TIMEOUT,
    )?;
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

/// The uncommitted changes matching `pathspecs` — the git-controls
/// scope, anchored at the repository root. Git decides membership by
/// its own pathspec rules, the same ones a later `add`/`commit` with
/// the same list will apply; nothing is filtered afterwards.
/// `--untracked-files=all` lists a new directory's files one by one
/// instead of collapsing them into the directory, so every new item
/// counts as one.
pub fn scoped_changes(root: &Path, pathspecs: &[String]) -> Result<Vec<ChangedPath>, GitError> {
    if pathspecs.is_empty() {
        return Ok(Vec::new());
    }
    let mut args: Vec<&str> = vec![
        "status",
        "--porcelain=v2",
        "-z",
        "--untracked-files=all",
        "--",
    ];
    args.extend(pathspecs.iter().map(String::as_str));
    let output = run(root, &args, LOCAL_TIMEOUT)?;
    if !output.success {
        return Err(GitError::Failed {
            command: "status -- <scope>".to_owned(),
            stderr: output.stderr,
        });
    }
    Ok(parse_porcelain_entries(&output.stdout).1)
}

/// Parse `git status --porcelain=v2 --branch -z` output. Header lines
/// (`# branch.…`) carry the branch facts; every other entry is one
/// changed, untracked, or unmerged file.
///
/// - `# branch.head <name>` — `(detached)` maps to `HEAD`, matching
///   the wire contract for a detached head. On an unborn branch (fresh
///   `git init`, nothing committed) this still names the real branch.
/// - `# branch.oid (initial)` — the branch is unborn; anything else is
///   a commit hash, so `has_commits` is true.
/// - `# branch.ab +<ahead> -<behind>` — present exactly when the
///   upstream ref resolves, which is what `has_upstream` means.
fn parse_porcelain_status(stdout: &str) -> RepoSnapshot {
    let mut branch = String::from("HEAD");
    let mut has_upstream = false;
    let mut has_commits = true;
    let mut ahead = 0;
    let mut behind = 0;

    let (headers, dirty) = parse_porcelain_entries(stdout);
    for line in headers {
        if let Some(name) = line.strip_prefix("# branch.head ") {
            if name != "(detached)" {
                branch = name.to_owned();
            }
        } else if let Some(oid) = line.strip_prefix("# branch.oid ") {
            has_commits = oid != "(initial)";
        } else if let Some(counts) = line.strip_prefix("# branch.ab ") {
            has_upstream = true;
            for count in counts.split_whitespace() {
                if let Some(value) = count.strip_prefix('+') {
                    ahead = value.parse().unwrap_or(0);
                } else if let Some(value) = count.strip_prefix('-') {
                    behind = value.parse().unwrap_or(0);
                }
            }
        }
    }

    RepoSnapshot {
        branch,
        has_upstream,
        has_commits,
        ahead,
        behind,
        dirty,
    }
}

/// Split NUL-terminated porcelain v2 output into its header lines and
/// its file entries. `-z` is what makes paths safe to read back: without
/// it, git quotes paths with spaces or non-ASCII characters, and a
/// rename's two paths share one line separated by a tab.
///
/// Entry shapes (fields are space-separated, the path is last):
/// - `1 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>` — an ordinary change;
/// - `2 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <Xscore> <path>` followed
///   by the original path as its own NUL-terminated token;
/// - `u <XY> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>` — unmerged;
/// - `? <path>` — untracked; `! <path>` — ignored (not requested here).
fn parse_porcelain_entries(stdout: &str) -> (Vec<&str>, Vec<ChangedPath>) {
    let mut headers = Vec::new();
    let mut changes = Vec::new();
    let mut tokens = stdout.split('\0').filter(|token| !token.is_empty());
    while let Some(token) = tokens.next() {
        if token.starts_with('#') {
            headers.push(token);
        } else if let Some(path) = token.strip_prefix("? ") {
            changes.push(ChangedPath {
                path: path.to_owned(),
                state: ChangeState::Added,
            });
        } else if token.starts_with("1 ") {
            if let Some((status, path)) = split_fields(token, 8) {
                changes.push(ChangedPath {
                    path: path.to_owned(),
                    state: ordinary_state(status),
                });
            }
        } else if token.starts_with("2 ") {
            let from = tokens.next().unwrap_or_default();
            if let Some((_status, path)) = split_fields(token, 9) {
                changes.push(ChangedPath {
                    path: path.to_owned(),
                    state: ChangeState::Renamed {
                        from: from.to_owned(),
                    },
                });
            }
        } else if token.starts_with("u ") {
            if let Some((_status, path)) = split_fields(token, 10) {
                changes.push(ChangedPath {
                    path: path.to_owned(),
                    state: ChangeState::Unmerged,
                });
            }
        }
        // `!` (ignored) and anything unknown: nothing to report.
    }
    (headers, changes)
}

/// The `<XY>` field and the path of an entry whose path follows
/// `fixed_fields` space-separated fields (the entry type included).
fn split_fields(token: &str, fixed_fields: usize) -> Option<(&str, &str)> {
    let mut rest = token;
    let mut status = "";
    for index in 0..fixed_fields {
        let (field, remainder) = rest.split_once(' ')?;
        if index == 1 {
            status = field;
        }
        rest = remainder;
    }
    Some((status, rest))
}

/// What an ordinary entry's `<XY>` (index, work tree) means for the
/// file as a whole: gone from the work tree is deleted, new in the
/// index is added, everything else is modified.
fn ordinary_state(status: &str) -> ChangeState {
    let mut letters = status.chars();
    let index = letters.next().unwrap_or('.');
    let work_tree = letters.next().unwrap_or('.');
    if work_tree == 'D' || (index == 'D' && work_tree == '.') {
        ChangeState::Deleted
    } else if index == 'A' {
        ChangeState::Added
    } else {
        ChangeState::Modified
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn joined(entries: &[&str]) -> String {
        entries.iter().map(|entry| format!("{entry}\0")).collect()
    }

    #[test]
    fn parses_synced_branch_with_upstream() {
        let stdout = joined(&[
            "# branch.oid 1234567890abcdef1234567890abcdef12345678",
            "# branch.head main",
            "# branch.upstream origin/main",
            "# branch.ab +0 -0",
        ]);
        assert_eq!(
            parse_porcelain_status(&stdout),
            RepoSnapshot {
                branch: "main".into(),
                has_upstream: true,
                has_commits: true,
                ahead: 0,
                behind: 0,
                dirty: vec![],
            }
        );
    }

    #[test]
    fn parses_ahead_behind_and_dirty_files() {
        let stdout = joined(&[
            "# branch.oid 1234567890abcdef1234567890abcdef12345678",
            "# branch.head feature",
            "# branch.upstream origin/feature",
            "# branch.ab +2 -3",
            "1 .M N... 100644 100644 100644 0123456 0123456 workdown-items/item-a.md",
            "? workdown-items/item d.md",
        ]);
        assert_eq!(
            parse_porcelain_status(&stdout),
            RepoSnapshot {
                branch: "feature".into(),
                has_upstream: true,
                has_commits: true,
                ahead: 2,
                behind: 3,
                dirty: vec![
                    ChangedPath {
                        path: "workdown-items/item-a.md".into(),
                        state: ChangeState::Modified,
                    },
                    ChangedPath {
                        path: "workdown-items/item d.md".into(),
                        state: ChangeState::Added,
                    },
                ],
            }
        );
    }

    #[test]
    fn classifies_every_entry_shape() {
        let stdout = joined(&[
            "1 A. N... 000000 100644 100644 0000000 0123456 new-staged.md",
            "1 AM N... 000000 100644 100644 0000000 0123456 new-staged-then-edited.md",
            "1 .D N... 100644 100644 000000 0123456 0123456 gone-from-tree.md",
            "1 D. N... 100644 000000 000000 0123456 0000000 staged-delete.md",
            "1 MM N... 100644 100644 100644 0123456 0123456 edited-twice.md",
            "2 R. N... 100644 100644 100644 0123456 0123456 R100 renamed-to.md",
            "renamed-from.md",
            "u UU N... 100644 100644 100644 100644 0123456 0123456 0123456 conflicted.md",
            "? untracked.md",
        ]);
        let (_headers, changes) = parse_porcelain_entries(&stdout);
        let states: Vec<(&str, &ChangeState)> = changes
            .iter()
            .map(|change| (change.path.as_str(), &change.state))
            .collect();
        assert_eq!(
            states,
            vec![
                ("new-staged.md", &ChangeState::Added),
                ("new-staged-then-edited.md", &ChangeState::Added),
                ("gone-from-tree.md", &ChangeState::Deleted),
                ("staged-delete.md", &ChangeState::Deleted),
                ("edited-twice.md", &ChangeState::Modified),
                (
                    "renamed-to.md",
                    &ChangeState::Renamed {
                        from: "renamed-from.md".into()
                    }
                ),
                ("conflicted.md", &ChangeState::Unmerged),
                ("untracked.md", &ChangeState::Added),
            ]
        );
    }

    #[test]
    fn no_upstream_line_means_no_upstream() {
        // An unborn branch (fresh init) and a branch with an unresolvable
        // upstream both omit `branch.ab` — either way push has nowhere to
        // go and ahead/behind carry no information.
        let stdout = joined(&[
            "# branch.oid (initial)",
            "# branch.head main",
            "? workdown-items/item-a.md",
        ]);
        assert_eq!(
            parse_porcelain_status(&stdout),
            RepoSnapshot {
                branch: "main".into(),
                has_upstream: false,
                has_commits: false,
                ahead: 0,
                behind: 0,
                dirty: vec![ChangedPath {
                    path: "workdown-items/item-a.md".into(),
                    state: ChangeState::Added,
                }],
            }
        );
    }

    #[test]
    fn resolves_publish_remote_by_default_then_sole_then_origin() {
        // pushDefault wins when it names a real remote…
        assert_eq!(
            resolve_publish_remote(
                "remote.pushDefault fork\nremote.origin.url a\nremote.fork.url b\n"
            ),
            Some("fork".into())
        );
        // …and is ignored when it names one that no longer exists.
        assert_eq!(
            resolve_publish_remote("remote.pushDefault gone\nremote.origin.url a\n"),
            Some("origin".into())
        );
        // The only remote is used whatever it is called.
        assert_eq!(
            resolve_publish_remote("remote.upstream.url a\nremote.upstream.fetch x\n"),
            Some("upstream".into())
        );
        // Several remotes: origin if present, otherwise no answer.
        assert_eq!(
            resolve_publish_remote("remote.fork.url b\nremote.origin.url a\n"),
            Some("origin".into())
        );
        assert_eq!(
            resolve_publish_remote("remote.fork.url b\nremote.upstream.url a\n"),
            None
        );
        assert_eq!(resolve_publish_remote(""), None);
    }

    #[test]
    fn detached_head_reads_as_head() {
        let stdout = joined(&[
            "# branch.oid 1234567890abcdef1234567890abcdef12345678",
            "# branch.head (detached)",
        ]);
        assert_eq!(parse_porcelain_status(&stdout).branch, "HEAD");
    }

    #[test]
    fn a_command_that_outlives_its_timeout_is_killed() {
        // `git -C <root> --version` needs no repository; a tiny timeout
        // still has to end in `TimedOut` rather than a hang. Skipped
        // silently when git is not installed — the other tests cover the
        // parsing, and this one is about the deadline.
        let Ok(result) = std::panic::catch_unwind(|| {
            run(Path::new("."), &["--version"], Duration::from_nanos(1))
        }) else {
            return;
        };
        match result {
            Err(GitError::TimedOut) | Err(GitError::Spawn(_)) => {}
            Ok(output) => assert!(output.success, "git ran to completion inside the deadline"),
            Err(other) => panic!("unexpected error: {other}"),
        }
    }
}
