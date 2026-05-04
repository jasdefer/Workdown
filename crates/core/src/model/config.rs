//! Project configuration types, deserialized from `config.yaml`.

use std::path::PathBuf;

use serde::Deserialize;

use super::calendar::WorkingCalendar;
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
    /// CLI default settings (which fields to use for views).
    pub defaults: ViewDefaults,
    /// Project-wide working calendar — the days of the week that count
    /// as work days for views like workload. `None` means inherit the
    /// built-in Monday–Friday default; consume via [`Self::working_calendar`].
    #[serde(default)]
    pub working_days: Option<Vec<Weekday>>,
}

impl Config {
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

/// Default field selections for CLI views.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewDefaults {
    /// Field used for board columns (must be a `choice` field).
    pub board_field: String,
    /// Field used for tree hierarchy (must be a `link` field).
    pub tree_field: String,
    /// Field used for dependency graph (must be a `links` field).
    pub graph_field: String,
}
