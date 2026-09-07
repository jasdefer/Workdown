//! Which paths the git controls are allowed to touch.
//!
//! The web app may stage and commit *workdown files only*: the paths
//! `config.yaml` names (`paths.work_items`, `paths.templates`,
//! `paths.resources`, `paths.views`, `schema`) plus `config.yaml`
//! itself. Nothing else in the repository is ever staged by a browser
//! button — a source change sitting next to the items is invisible to
//! it, which is what makes `serve.git_controls` safe to switch on in a
//! code repository.
//!
//! The scope is computed once per request and handed to every git call
//! that needs it (status, add, commit) as the same pathspec list, so
//! membership is decided by git's pathspec matching everywhere and
//! never by an after-the-fact filter that could drift from it. Entries
//! are relative to the *repository* root — a workdown project may live
//! in a subfolder of a larger repository — and passed to git with the
//! `:(top)` magic so they are anchored there regardless of the working
//! directory.

use std::path::Path;

use workdown_core::change_summary::ChangedFileKind;
use workdown_core::model::config::Config;

/// The role a scope entry plays, by the config key it came from. The
/// role, not the filename, is what the pill shows ("schema" whatever
/// the file is called) and what decides whether a changed file is a
/// work item or a definition file for the commit message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeRole {
    WorkItems,
    Templates,
    Resources,
    Views,
    Schema,
    Config,
}

impl ScopeRole {
    /// Definition roles in the order the pill names them.
    pub const DEFINITIONS: [ScopeRole; 5] = [
        ScopeRole::Schema,
        ScopeRole::Views,
        ScopeRole::Resources,
        ScopeRole::Templates,
        ScopeRole::Config,
    ];

    /// The short name the pill and the dialog use for this role.
    pub fn label(self) -> &'static str {
        match self {
            ScopeRole::WorkItems => "items",
            ScopeRole::Templates => "templates",
            ScopeRole::Resources => "resources",
            ScopeRole::Views => "views",
            ScopeRole::Schema => "schema",
            ScopeRole::Config => "config",
        }
    }

    /// How the commit message generator should treat a file in this role.
    pub fn kind(self) -> ChangedFileKind {
        match self {
            ScopeRole::WorkItems => ChangedFileKind::WorkItem,
            _ => ChangedFileKind::Definition,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeEntry {
    pub role: ScopeRole,
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
        let sources = [
            (ScopeRole::WorkItems, config.paths.work_items.as_path()),
            (ScopeRole::Templates, config.paths.templates.as_path()),
            (ScopeRole::Resources, config.paths.resources.as_path()),
            (ScopeRole::Views, config.paths.views.as_path()),
            (ScopeRole::Schema, config.schema.as_path()),
            (ScopeRole::Config, config_path),
        ];
        let prefix = normalize(prefix);
        let mut entries = Vec::new();
        for (role, path) in sources {
            let Some(relative) = project_relative(path, project_root) else {
                continue;
            };
            let path = join(&prefix, &relative);
            if entries.iter().any(|entry: &ScopeEntry| entry.path == path) {
                // Two keys naming the same path (a `views` file inside
                // the templates directory would be odd, but a repeated
                // path is not): the first role wins.
                continue;
            }
            entries.push(ScopeEntry { role, path });
        }
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
    pub fn classify(&self, repository_path: &str) -> Option<ScopeRole> {
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
        let paths: Vec<(&str, ScopeRole)> = scope
            .entries()
            .iter()
            .map(|entry| (entry.path.as_str(), entry.role))
            .collect();
        assert_eq!(
            paths,
            vec![
                ("workdown-items", ScopeRole::WorkItems),
                (".workdown/templates", ScopeRole::Templates),
                (".workdown/resources.yaml", ScopeRole::Resources),
                (".workdown/views.yaml", ScopeRole::Views),
                (".workdown/schema.yaml", ScopeRole::Schema),
                (".workdown/config.yaml", ScopeRole::Config),
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
            Some(ScopeRole::WorkItems)
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
            Some(ScopeRole::WorkItems)
        );
        assert_eq!(
            scope.classify("workdown-items/nested/a.md"),
            Some(ScopeRole::WorkItems)
        );
        assert_eq!(
            scope.classify(".workdown/schema.yaml"),
            Some(ScopeRole::Schema)
        );
        assert_eq!(
            scope.classify(".workdown/templates/bug.md"),
            Some(ScopeRole::Templates)
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
            Some(ScopeRole::WorkItems)
        );
    }

    #[test]
    fn work_items_at_the_project_root_covers_everything_under_it() {
        let scope = scope("tracker/", ".");
        assert_eq!(scope.entries()[0].path, "tracker");
        assert_eq!(scope.classify("tracker/a.md"), Some(ScopeRole::WorkItems));
        // Nested definition files still win over the enclosing items
        // directory — the longer match.
        assert_eq!(
            scope.classify("tracker/.workdown/schema.yaml"),
            Some(ScopeRole::Schema)
        );
        assert_eq!(scope.classify("other/a.md"), None);

        let top = scope_with_root_items();
        assert_eq!(top.pathspecs()[0], ":(top)");
        assert_eq!(top.classify("anything.md"), Some(ScopeRole::WorkItems));
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
            .all(|entry| entry.role != ScopeRole::Config));
    }
}
