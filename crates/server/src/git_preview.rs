//! What a commit from workdown would cover and say — shared by the
//! `GET /api/git/commit-preview` endpoint and the `workdown changes`
//! command, so a terminal and the dialog describe the same change with
//! the same words.
//!
//! Reads the repository through [`crate::git`], limits every question to
//! the [`crate::git_scope::GitScope`] the config defines, and hands the
//! changed files' texts at `HEAD` and in the working tree to
//! `workdown_core::change_summary` for wording. No caching, no state:
//! every call re-reads the repository, like every other project read.

use std::path::Path;

use workdown_core::change_summary::{summarize_changes, ChangedFile, ChangedFileKind};
use workdown_core::git_data::{GitChangeKind, GitChangedFile, GitCommitPreview, GitStatus};
use workdown_core::model::config::Config;
use workdown_core::model::schema::Schema;

use crate::git::{self, ChangeState, ChangedPath, GitError, RepoSnapshot};
use crate::git_scope::{GitScope, ScopeRole};

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
        let roles: Vec<ScopeRole> = self
            .in_scope
            .iter()
            .filter_map(|change| self.scope.classify(&change.path))
            .collect();
        let dirty_items = roles
            .iter()
            .filter(|role| **role == ScopeRole::WorkItems)
            .count() as u32;
        let dirty_definitions = ScopeRole::DEFINITIONS
            .iter()
            .filter(|role| roles.contains(role))
            .map(|role| role.label().to_owned())
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

/// Read the repository: snapshot, scope, in-scope changes. `None` when
/// the project is not inside a git work tree. The scoped status is
/// skipped when the whole repository is clean — nothing in scope can
/// be dirty then.
pub async fn read_local(
    project_root: &Path,
    config: &Config,
    config_path: &Path,
) -> Result<Option<LocalState>, GitError> {
    let Some(snapshot) = git::snapshot(project_root).await? else {
        return Ok(None);
    };
    let Some(prefix) = git::repository_prefix(project_root).await? else {
        return Ok(None);
    };
    let scope = GitScope::from_config(&prefix, config, config_path, project_root);
    let in_scope = if snapshot.dirty.is_empty() {
        Vec::new()
    } else {
        git::scoped_changes(project_root, &scope.pathspecs()).await?
    };
    Ok(Some(LocalState {
        snapshot,
        scope,
        in_scope,
    }))
}

/// What a commit would cover and say right now: the in-scope files with
/// their change kind, the generated message, and the dirty files outside
/// the scope. `None` when the project is not inside a git repository.
/// Read-only and repeatable.
pub async fn build_preview(
    project_root: &Path,
    config: &Config,
    config_path: &Path,
    schema: &Schema,
) -> Result<Option<GitCommitPreview>, GitError> {
    let (Some(local), Some(top_level)) = (
        read_local(project_root, config, config_path).await?,
        git::top_level(project_root).await?,
    ) else {
        return Ok(None);
    };

    let mut files = Vec::with_capacity(local.in_scope.len());
    let mut inputs = Vec::with_capacity(local.in_scope.len());
    for change in &local.in_scope {
        // Git matched the path against the scope's own pathspecs, so a
        // role is always found; the fallback only keeps an unexpected
        // disagreement from dropping a file the commit would include.
        let (role, kind) = local
            .scope
            .classify(&change.path)
            .map_or(("files", ChangedFileKind::Definition), |role| {
                (role.label(), role.kind())
            });
        let project_path = local.scope.project_relative(&change.path);
        let (old_text, new_text, change_kind) = match &change.state {
            ChangeState::Added => (
                None,
                read_work_tree(&top_level, &change.path),
                GitChangeKind::Added,
            ),
            ChangeState::Deleted => (
                git::show_at_head(project_root, &change.path).await?,
                None,
                GitChangeKind::Deleted,
            ),
            ChangeState::Modified => (
                git::show_at_head(project_root, &change.path).await?,
                read_work_tree(&top_level, &change.path),
                GitChangeKind::Modified,
            ),
            ChangeState::Renamed { from } => (
                git::show_at_head(project_root, from).await?,
                read_work_tree(&top_level, &change.path),
                GitChangeKind::Renamed,
            ),
            ChangeState::Unmerged => (
                git::show_at_head(project_root, &change.path).await?,
                read_work_tree(&top_level, &change.path),
                GitChangeKind::Unmerged,
            ),
        };
        files.push(GitChangedFile {
            path: project_path.clone(),
            role: role.to_owned(),
            change: change_kind,
        });
        inputs.push(ChangedFile {
            path: std::path::PathBuf::from(project_path),
            kind,
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
