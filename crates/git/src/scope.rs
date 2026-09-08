//! Which paths the git controls are allowed to touch.
//!
//! The web app may stage and commit *workdown files only*: the paths
//! `Config::workdown_paths` lists — what `config.yaml` names
//! (`paths.work_items`, `paths.templates`, `paths.resources`,
//! `paths.views`, `schema`) plus `config.yaml` itself. Nothing else in
//! the repository is ever staged by a browser button — a source change
//! sitting next to the items is invisible to it, which is what makes
//! `serve.git_controls` safe to switch on in a code repository.
//!
//! The scope is computed once per process (the project cannot move
//! inside its repository while the server runs) and handed to every git
//! call that needs it (status, add, commit) as the same pathspec list,
//! so membership is decided by git's pathspec matching everywhere and
//! never by an after-the-fact filter that could drift from it. Entries
//! are relative to the *repository* root — a workdown project may live
//! in a subfolder of a larger repository — and passed to git with the
//! `:(top)` magic so they are anchored there regardless of the working
//! directory.

use std::path::Path;

use workdown_core::model::config::{Config, PathRole};

use crate::{repository_prefix, GitError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeEntry {
    pub role: PathRole,
    /// Repository-relative, forward slashes, no leading `./`, no
    /// trailing slash. Empty means the repository root itself (a
    /// project at the top whose `work_items` is `.`).
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitScope {
    /// The project root inside the repository, normalized: empty at
    /// the top, `sub/dir` below.
    prefix: String,
    entries: Vec<ScopeEntry>,
}

impl GitScope {
    /// The scope for the project at `project_root`: asks git where the
    /// project sits inside its repository, then builds the entries from
    /// the config. `None` when the project is not inside a git work
    /// tree. `config_path` is where `config.yaml` was read from, as the
    /// CLI was given it.
    pub fn for_project(
        project_root: &Path,
        config: &Config,
        config_path: &Path,
    ) -> Result<Option<GitScope>, GitError> {
        let Some(prefix) = repository_prefix(project_root)? else {
            return Ok(None);
        };
        Ok(Some(GitScope::from_config(
            &prefix,
            config,
            config_path,
            project_root,
        )))
    }

    /// Build the scope from the project config. `prefix` is where the
    /// project root sits inside the repository, as `git rev-parse
    /// --show-prefix` reports it: empty at the top, `sub/dir/` below.
    /// `config_path` is where `config.yaml` was read from, relative to
    /// the project root or absolute — the same value the CLI passed to
    /// the server state.
    ///
    /// A config path that is absolute and outside the project root
    /// cannot be expressed relative to the repository and is left out
    /// of the scope: the button then never commits it, which is the
    /// safe direction to fail in.
    pub fn from_config(
        prefix: &str,
        config: &Config,
        config_path: &Path,
        project_root: &Path,
    ) -> GitScope {
        let prefix = normalize(prefix);
        let entries = config
            .workdown_paths(config_path)
            .into_iter()
            .filter_map(|workdown_path| {
                let relative = project_relative(&workdown_path.path, project_root)?;
                Some(ScopeEntry {
                    role: workdown_path.role,
                    path: join(&prefix, &relative),
                })
            })
            .collect();
        GitScope { prefix, entries }
    }

    /// A repository-relative path as the project sees it: the project
    /// prefix stripped. A path outside the prefix comes back unchanged.
    pub fn project_relative(&self, repository_path: &str) -> String {
        let path = normalize(repository_path);
        if self.prefix.is_empty() {
            return path;
        }
        path.strip_prefix(self.prefix.as_str())
            .and_then(|rest| rest.strip_prefix('/'))
            .map_or(path.clone(), str::to_owned)
    }

    pub fn entries(&self) -> &[ScopeEntry] {
        &self.entries
    }

    /// The pathspecs to hand git: one per entry, anchored at the
    /// repository root so they mean the same thing from any working
    /// directory. Directories match everything beneath them.
    pub fn pathspecs(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|entry| {
                if entry.path.is_empty() {
                    ":(top)".to_owned()
                } else {
                    format!(":(top){}", entry.path)
                }
            })
            .collect()
    }

    /// The role of a repository-relative path, or `None` when it is
    /// outside the scope. The longest matching entry wins, so a file
    /// under a nested directory is attributed to the nearer key.
    pub fn classify(&self, repository_path: &str) -> Option<PathRole> {
        let path = normalize(repository_path);
        self.entries
            .iter()
            .filter(|entry| {
                entry.path.is_empty()
                    || path == entry.path
                    || path
                        .strip_prefix(entry.path.as_str())
                        .is_some_and(|rest| rest.starts_with('/'))
            })
            .max_by_key(|entry| entry.path.len())
            .map(|entry| entry.role)
    }
}

/// A config path relative to the project root, or `None` when it is
/// absolute and points elsewhere.
fn project_relative(path: &Path, project_root: &Path) -> Option<String> {
    let relative = if path.is_absolute() {
        path.strip_prefix(project_root).ok()?
    } else {
        path
    };
    Some(normalize(&relative.to_string_lossy()))
}

/// Forward slashes, no `./` segments at the front, no trailing slash.
fn normalize(path: &str) -> String {
    let mut text = path.replace('\\', "/");
    while let Some(rest) = text.strip_prefix("./") {
        text = rest.to_owned();
    }
    if text == "." {
        return String::new();
    }
    text.trim_end_matches('/').to_owned()
}

fn join(prefix: &str, relative: &str) -> String {
    match (prefix.is_empty(), relative.is_empty()) {
        (true, _) => relative.to_owned(),
        (false, true) => prefix.to_owned(),
        (false, false) => format!("{prefix}/{relative}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use workdown_core::parser::config::parse_config;

    fn config(work_items: &str) -> Config {
        parse_config(&format!(
            "\
project:
  name: Test
  description: ''
paths:
  work_items: {work_items}
  templates: .workdown/templates
  resources: .workdown/resources.yaml
  views: .workdown/views.yaml
schema: .workdown/schema.yaml
defaults:
  board_field: status
  tree_field: parent
  graph_field: parent
"
        ))
        .expect("config parses")
    }

    fn scope(prefix: &str, work_items: &str) -> GitScope {
        GitScope::from_config(
            prefix,
            &config(work_items),
            Path::new(".workdown/config.yaml"),
            Path::new("/project"),
        )
    }

    #[test]
    fn entries_follow_the_config_keys_at_the_repository_top() {
        let scope = scope("", "workdown-items");
        let paths: Vec<(&str, PathRole)> = scope
            .entries()
            .iter()
            .map(|entry| (entry.path.as_str(), entry.role))
            .collect();
        assert_eq!(
            paths,
            vec![
                ("workdown-items", PathRole::WorkItems),
                (".workdown/templates", PathRole::Templates),
                (".workdown/resources.yaml", PathRole::Resources),
                (".workdown/views.yaml", PathRole::Views),
                (".workdown/schema.yaml", PathRole::Schema),
                (".workdown/config.yaml", PathRole::Config),
            ]
        );
        assert_eq!(scope.pathspecs()[0], ":(top)workdown-items");
    }

    #[test]
    fn a_project_in_a_subfolder_is_prefixed() {
        let nested = scope("tracker/", "./items/");
        assert_eq!(nested.entries()[0].path, "tracker/items");
        assert_eq!(nested.entries()[5].path, "tracker/.workdown/config.yaml");
        assert_eq!(
            nested.classify("tracker/items/fix-login.md"),
            Some(PathRole::WorkItems)
        );
        assert_eq!(nested.classify("items/fix-login.md"), None);
        assert_eq!(nested.classify("tracker/src/main.rs"), None);
        assert_eq!(
            nested.project_relative("tracker/items/fix-login.md"),
            "items/fix-login.md"
        );
        assert_eq!(nested.project_relative("README.md"), "README.md");
        assert_eq!(
            scope("", "workdown-items").project_relative("workdown-items/a.md"),
            "workdown-items/a.md"
        );
    }

    #[test]
    fn classify_matches_files_and_directories_but_not_name_prefixes() {
        let scope = scope("", "workdown-items");
        assert_eq!(
            scope.classify("workdown-items/a.md"),
            Some(PathRole::WorkItems)
        );
        assert_eq!(
            scope.classify("workdown-items/nested/a.md"),
            Some(PathRole::WorkItems)
        );
        assert_eq!(
            scope.classify(".workdown/schema.yaml"),
            Some(PathRole::Schema)
        );
        assert_eq!(
            scope.classify(".workdown/templates/bug.md"),
            Some(PathRole::Templates)
        );
        // `workdown-items-old/` shares a name prefix, not a directory.
        assert_eq!(scope.classify("workdown-items-old/a.md"), None);
        assert_eq!(scope.classify(".workdown/other.yaml"), None);
        assert_eq!(scope.classify("src/main.rs"), None);
        assert_eq!(scope.classify("views/board.md"), None);
    }

    #[test]
    fn backslashes_in_reported_paths_are_tolerated() {
        let scope = scope("", "workdown-items");
        assert_eq!(
            scope.classify("workdown-items\\a.md"),
            Some(PathRole::WorkItems)
        );
    }

    #[test]
    fn work_items_at_the_project_root_covers_everything_under_it() {
        let scope = scope("tracker/", ".");
        assert_eq!(scope.entries()[0].path, "tracker");
        assert_eq!(scope.classify("tracker/a.md"), Some(PathRole::WorkItems));
        // Nested definition files still win over the enclosing items
        // directory — the longer match.
        assert_eq!(
            scope.classify("tracker/.workdown/schema.yaml"),
            Some(PathRole::Schema)
        );
        assert_eq!(scope.classify("other/a.md"), None);

        let top = scope_with_root_items();
        assert_eq!(top.pathspecs()[0], ":(top)");
        assert_eq!(top.classify("anything.md"), Some(PathRole::WorkItems));
    }

    fn scope_with_root_items() -> GitScope {
        scope("", ".")
    }

    #[test]
    fn absolute_config_path_inside_the_project_is_made_relative() {
        let scope = GitScope::from_config(
            "",
            &config("workdown-items"),
            &PathBuf::from("/project/conf/config.yaml"),
            Path::new("/project"),
        );
        assert_eq!(scope.entries()[5].path, "conf/config.yaml");
    }

    #[test]
    fn absolute_config_path_outside_the_project_is_left_out() {
        let scope = GitScope::from_config(
            "",
            &config("workdown-items"),
            &PathBuf::from("/elsewhere/config.yaml"),
            Path::new("/project"),
        );
        assert!(scope
            .entries()
            .iter()
            .all(|entry| entry.role != PathRole::Config));
    }
}
