//! What a commit from workdown would cover and say — shared by the
//! server's `GET /api/git/commit-preview` endpoint and the `workdown
//! changes` command, so a terminal and the dialog describe the same
//! change with the same words.
//!
//! Reads the repository through the crate root, limits every question
//! to the [`GitScope`] the config defines, and hands the changed files'
//! texts at `HEAD` and in the working tree to
//! `workdown_core::change_summary` for wording. No caching, no state:
//! every call re-reads the repository, like every other project read.

use std::path::{Path, PathBuf};

use workdown_core::change_summary::{summarize_changes, ChangedFile};
use workdown_core::git_data::{GitChangeKind, GitChangedFile, GitCommitPreview, GitStatus};
use workdown_core::model::config::{Config, PathRole};
use workdown_core::model::schema::Schema;

use crate::scope::GitScope;
use crate::{scoped_changes, show_at_head, snapshot, top_level};
use crate::{ChangeState, ChangedPath, GitError, RepoSnapshot};

/// Everything a status answer is built from: the whole-repository
/// snapshot, the git-controls scope, and the uncommitted changes inside
/// that scope.
pub struct LocalState {
    pub snapshot: RepoSnapshot,
    pub scope: GitScope,
    /// The dirty files the commit button would commit — git's own
    /// answer for the scope's pathspecs, not a filter over `snapshot`.
    pub in_scope: Vec<ChangedPath>,
}

impl LocalState {
    /// Lift into the wire status. The dirty numbers come from the
    /// in-scope changes only, counted by role: work items as a number,
    /// definition files by name.
    pub fn into_status(self, fetch_error: Option<String>) -> GitStatus {
        let roles: Vec<PathRole> = self
            .in_scope
            .iter()
            .filter_map(|change| self.scope.classify(&change.path))
            .collect();
        let dirty_items = roles.iter().filter(|role| role.is_work_item()).count() as u32;
        let dirty_definitions = PathRole::DEFINITIONS
            .into_iter()
            .filter(|role| roles.contains(role))
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
    pub fn outside_scope(&self) -> Vec<&str> {
        self.snapshot
            .dirty
            .iter()
            .map(|change| change.path.as_str())
            .filter(|path| !self.in_scope.iter().any(|change| change.path == *path))
            .collect()
    }
}

/// Read the repository: snapshot and the changes inside `scope`. `None`
/// when the project is not inside a git work tree. The scoped status is
/// skipped when the whole repository is clean — nothing in scope can be
/// dirty then.
pub fn read_local(project_root: &Path, scope: &GitScope) -> Result<Option<LocalState>, GitError> {
    let Some(snapshot) = snapshot(project_root)? else {
        return Ok(None);
    };
    let in_scope = if snapshot.dirty.is_empty() {
        Vec::new()
    } else {
        scoped_changes(project_root, &scope.pathspecs())?
    };
    Ok(Some(LocalState {
        snapshot,
        scope: scope.clone(),
        in_scope,
    }))
}

/// What a commit would cover and say right now: the in-scope files with
/// their change kind, the generated message, and the dirty files outside
/// the scope. `None` when the project is not inside a git repository.
/// Read-only and repeatable.
pub fn build_preview(
    project_root: &Path,
    scope: &GitScope,
    config: &Config,
    schema: &Schema,
) -> Result<Option<GitCommitPreview>, GitError> {
    let (Some(local), Some(top_level)) =
        (read_local(project_root, scope)?, top_level(project_root)?)
    else {
        return Ok(None);
    };

    let mut files = Vec::with_capacity(local.in_scope.len());
    let mut inputs = Vec::with_capacity(local.in_scope.len());
    for change in &local.in_scope {
        // Git matched the path against the scope's own pathspecs, so the
        // scope must be able to name its role; if it cannot, the two
        // rules have drifted apart, and that is a bug to surface rather
        // than a file to guess about.
        let role = local
            .scope
            .classify(&change.path)
            .ok_or_else(|| GitError::ScopeMismatch {
                path: change.path.clone(),
            })?;
        let project_path = local.scope.project_relative(&change.path);
        let (old_text, new_text, change_kind) = match &change.state {
            ChangeState::Added => (
                None,
                read_work_tree(&top_level, &change.path),
                GitChangeKind::Added,
            ),
            ChangeState::Deleted => (
                show_at_head(project_root, &change.path)?,
                None,
                GitChangeKind::Deleted,
            ),
            ChangeState::Modified => (
                show_at_head(project_root, &change.path)?,
                read_work_tree(&top_level, &change.path),
                GitChangeKind::Modified,
            ),
            ChangeState::Renamed { from } => (
                show_at_head(project_root, from)?,
                read_work_tree(&top_level, &change.path),
                GitChangeKind::Renamed,
            ),
            ChangeState::Unmerged => (
                show_at_head(project_root, &change.path)?,
                read_work_tree(&top_level, &change.path),
                GitChangeKind::Unmerged,
            ),
        };
        files.push(GitChangedFile {
            path: project_path.clone(),
            role,
            change: change_kind,
            label: change_kind.label().to_owned(),
        });
        inputs.push(ChangedFile {
            path: PathBuf::from(project_path),
            role,
            old_text,
            new_text,
        });
    }

    let title_field = config.defaults.display.title.as_deref();
    let message = summarize_changes(&inputs, schema, title_field).to_message();
    let outside = local
        .outside_scope()
        .into_iter()
        .map(str::to_owned)
        .collect();
    Ok(Some(GitCommitPreview {
        files,
        message,
        outside,
    }))
}

/// The working-tree text of a repository-relative path, or `None` when
/// it cannot be read (deleted between the status and now, or not text).
fn read_work_tree(top_level: &Path, repository_path: &str) -> Option<String> {
    std::fs::read(top_level.join(repository_path))
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}
