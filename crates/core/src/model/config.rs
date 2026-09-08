//! Project configuration types, deserialized from `config.yaml`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::calendar::WorkingCalendar;
use super::views::DisplayConfig;
use super::weekday::Weekday;

/// A parsed project configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Project metadata (name, description).
    pub project: ProjectMeta,
    /// File paths for work items, templates, and resources.
    pub paths: Paths,
    /// Path to the schema file (relative to project root).
    pub schema: PathBuf,
    /// Project-wide field roles and display defaults — which field
    /// plays which role for the surfaces that need one.
    pub defaults: ViewDefaults,
    /// Project-wide working calendar — the days of the week that count
    /// as work days for views like workload. `None` means inherit the
    /// built-in Monday–Friday default; consume via [`Self::working_calendar`].
    #[serde(default)]
    pub working_days: Option<Vec<Weekday>>,
    /// Settings for `workdown serve`. Absent in older project configs;
    /// the CLI applies its own defaults when fields are missing.
    #[serde(default)]
    pub serve: Option<ServeConfig>,
}

impl Config {
    /// Every file and directory that belongs to workdown, with the role
    /// each plays — the one definition of "the workdown paths".
    /// `config_path` is where this config was read from, as the CLI was
    /// given it (relative to the project root, or absolute); it is the
    /// `Config` entry.
    ///
    /// The paths come back as written in `config.yaml`, in a fixed order
    /// (work items, templates, resources, views, schema, config), with a
    /// repeated path listed once under its first role. Consumers that
    /// care about some roles only filter this list rather than naming
    /// the config keys themselves: the git controls commit everything
    /// here, the pre-commit hook re-renders on everything that can
    /// change a rendered view.
    pub fn workdown_paths(&self, config_path: &Path) -> Vec<WorkdownPath> {
        let candidates = [
            (PathRole::WorkItems, self.paths.work_items.as_path()),
            (PathRole::Templates, self.paths.templates.as_path()),
            (PathRole::Resources, self.paths.resources.as_path()),
            (PathRole::Views, self.paths.views.as_path()),
            (PathRole::Schema, self.schema.as_path()),
            (PathRole::Config, config_path),
        ];
        let mut paths: Vec<WorkdownPath> = Vec::with_capacity(candidates.len());
        for (role, path) in candidates {
            if paths.iter().any(|known| known.path == path) {
                continue;
            }
            paths.push(WorkdownPath {
                role,
                path: path.to_path_buf(),
            });
        }
        paths
    }

    /// Build the [`WorkingCalendar`] this project's views should use.
    ///
    /// Falls back to [`WorkingCalendar::default_business_week`] when
    /// `working_days` is omitted from `config.yaml`. Per-view overrides
    /// on `Workload` are applied later, by the extractor.
    pub fn working_calendar(&self) -> WorkingCalendar {
        match &self.working_days {
            Some(days) => WorkingCalendar::from_days(days.iter().copied()),
            None => WorkingCalendar::default_business_week(),
        }
    }
}

/// The config key a workdown path came from. The role, not the
/// filename, is what a surface names ("schema" whatever the file is
/// called) and what tells a work item from a definition file.
///
/// On the wire (the git status and commit preview) it serializes as
/// [`PathRole::label`] — the same word the pill prints — so the
/// generated TypeScript type is the vocabulary the browser compares
/// against, and a test pins the two spellings together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum PathRole {
    #[serde(rename = "items")]
    WorkItems,
    Templates,
    Resources,
    Views,
    Schema,
    Config,
}

impl PathRole {
    /// The roles that are definition files rather than work items, in
    /// the order surfaces name them.
    pub const DEFINITIONS: [PathRole; 5] = [
        PathRole::Schema,
        PathRole::Views,
        PathRole::Resources,
        PathRole::Templates,
        PathRole::Config,
    ];

    /// The short name surfaces use for this role — the git pill's
    /// `3 items · schema`, the commit dialog's grouping, the role
    /// column of `workdown changes --files`.
    pub fn label(self) -> &'static str {
        match self {
            PathRole::WorkItems => "items",
            PathRole::Templates => "templates",
            PathRole::Resources => "resources",
            PathRole::Views => "views",
            PathRole::Schema => "schema",
            PathRole::Config => "config",
        }
    }

    /// Whether files in this role are work items (parsed frontmatter,
    /// named by title) rather than definition files (named by path).
    pub fn is_work_item(self) -> bool {
        self == PathRole::WorkItems
    }
}

/// One entry of [`Config::workdown_paths`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkdownPath {
    pub role: PathRole,
    /// As written in `config.yaml` (or passed on the command line for
    /// the config itself): relative to the project root, or absolute.
    pub path: PathBuf,
}

/// Project-level metadata.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectMeta {
    /// Project name.
    pub name: String,
    /// Optional project description.
    #[serde(default)]
    pub description: String,
}

/// Paths to key directories and files, relative to the project root.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Paths {
    /// Directory containing work item `.md` files.
    pub work_items: PathBuf,
    /// Directory containing work item templates.
    pub templates: PathBuf,
    /// Path to the resources file.
    pub resources: PathBuf,
    /// Path to the views file.
    pub views: PathBuf,
}

/// Settings for `workdown serve`. Optional fields let projects pin
/// just what they care about; the CLI applies built-in defaults for
/// anything omitted.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServeConfig {
    /// Port to bind. `None` means use the CLI's built-in default (3141)
    /// and scan upward on conflict.
    #[serde(default)]
    pub port: Option<u16>,
    /// Show the git controls in the web UI: commit & push behind a
    /// confirmation dialog, pull, push. Off unless explicitly enabled:
    /// they act on whatever git repository contains the project, which
    /// may be the user's whole code repo — nobody should get
    /// network-touching buttons they didn't ask for. A commit from the
    /// browser is limited to the paths this config names. Plain `bool`,
    /// not `Option`: absent and `false` mean the same thing, unlike
    /// `port`, whose `None` defers to the CLI default.
    #[serde(default)]
    pub git_controls: bool,
}

/// Project-wide field roles: the project's answer to "which field
/// plays this role", for surfaces that have nowhere else to ask.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewDefaults {
    /// Field used for board columns (must be a `choice` field).
    pub board_field: String,
    /// Field used for tree hierarchy (must be a `link` field).
    pub tree_field: String,
    /// Field used for dependency graph (must be a `links` field).
    pub graph_field: String,
    /// Field that carries measured effort (must be a `duration`
    /// field). Optional, and unset is a normal state: it means the
    /// project records no effort, and surfaces that would write it —
    /// the web app's timer — are simply absent. Never inferred, even
    /// for a project with exactly one duration field: an existing
    /// duration field is most likely a *calendar* duration, and
    /// aiming a stopwatch at it would commit measured work into a
    /// field meaning "this item spans three weeks".
    #[serde(default)]
    pub effort_field: Option<String>,
    /// Project-wide display roles, inherited by every view. A view's
    /// own `display:` block overrides these per role. Omitted section
    /// means no project defaults — views fall through to the per-kind
    /// hardcoded fallbacks.
    #[serde(default)]
    pub display: DisplayConfig,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::config::parse_config;

    fn config(work_items: &str, views: &str) -> Config {
        parse_config(&format!(
            "\
project:
  name: Test
  description: ''
paths:
  work_items: {work_items}
  templates: .workdown/templates
  resources: .workdown/resources.yaml
  views: {views}
schema: .workdown/schema.yaml
defaults:
  board_field: status
  tree_field: parent
  graph_field: parent
"
        ))
        .expect("config parses")
    }

    fn roles_and_paths(paths: &[WorkdownPath]) -> Vec<(PathRole, &str)> {
        paths
            .iter()
            .map(|entry| (entry.role, entry.path.to_str().expect("utf-8 path")))
            .collect()
    }

    #[test]
    fn workdown_paths_lists_every_role_in_a_fixed_order() {
        let paths = config("workdown-items", ".workdown/views.yaml")
            .workdown_paths(Path::new(".workdown/config.yaml"));
        assert_eq!(
            roles_and_paths(&paths),
            vec![
                (PathRole::WorkItems, "workdown-items"),
                (PathRole::Templates, ".workdown/templates"),
                (PathRole::Resources, ".workdown/resources.yaml"),
                (PathRole::Views, ".workdown/views.yaml"),
                (PathRole::Schema, ".workdown/schema.yaml"),
                (PathRole::Config, ".workdown/config.yaml"),
            ]
        );
    }

    #[test]
    fn workdown_paths_keeps_a_repeated_path_once_under_its_first_role() {
        // Views and schema pointing at the same file is unusual but legal;
        // the git scope and the hook installer must not list it twice.
        let paths = config("workdown-items", ".workdown/schema.yaml")
            .workdown_paths(Path::new(".workdown/config.yaml"));
        let roles: Vec<PathRole> = paths.iter().map(|entry| entry.role).collect();
        assert!(roles.contains(&PathRole::Views));
        assert!(!roles.contains(&PathRole::Schema));
        assert_eq!(paths.len(), 5);
    }

    #[test]
    fn workdown_paths_carries_the_config_path_as_given() {
        let paths = config("workdown-items", ".workdown/views.yaml")
            .workdown_paths(Path::new("/elsewhere/config.yaml"));
        let config_entry = paths
            .iter()
            .find(|entry| entry.role == PathRole::Config)
            .expect("config entry");
        assert_eq!(config_entry.path, PathBuf::from("/elsewhere/config.yaml"));
    }

    #[test]
    fn a_role_serializes_as_its_label() {
        // The wire spelling and the printed word are one vocabulary; a
        // rename on either side must fail here, not in the browser.
        for role in std::iter::once(PathRole::WorkItems).chain(PathRole::DEFINITIONS) {
            let serialized = serde_json::to_string(&role).expect("serializes");
            assert_eq!(serialized, format!("\"{}\"", role.label()), "{role:?}");
        }
    }

    #[test]
    fn definitions_are_every_role_but_work_items() {
        for role in PathRole::DEFINITIONS {
            assert!(!role.is_work_item(), "{role:?} is a definition role");
        }
        assert!(PathRole::WorkItems.is_work_item());
        assert!(!PathRole::DEFINITIONS.contains(&PathRole::WorkItems));
    }
}
