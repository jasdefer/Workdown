//! Shared project loader: reads everything `workdown render`, `workdown
//! validate`, and the HTTP server need from disk into one in-memory
//! struct.
//!
//! Hard failures (schema missing, views.yaml unparseable, items dir
//! unreadable) become [`LoadError`]. Per-item, cross-item, and
//! views_check diagnostics ride along inside the successful
//! [`Project`] — callers iterate `project.diagnostics` to surface them.
//!
//! The server calls this per HTTP request (cold-load — no caching).
//! Parsing is in the millisecond range for projects with a few hundred
//! items, well below human-perceptible latency. A future watcher
//! (`live-updates`) handles SSE push, not cache invalidation, because
//! there is no cache to invalidate.

use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::model::calendar::WorkingCalendar;
use crate::model::config::Config;
use crate::model::diagnostic::{Diagnostic, FileDiagnosticKind};
use crate::model::resources::Resources;
use crate::model::schema::{Schema, Severity};
use crate::model::views::Views;
use crate::parser;
use crate::store::Store;
use crate::{resources_check, views_check};

/// A fully loaded workdown project.
///
/// `views` is `None` when the project has no `views.yaml` file — that's
/// a valid configuration meaning "this project has no persisted views
/// yet," not an error.
pub struct Project {
    pub store: Store,
    pub schema: Schema,
    pub views: Option<Views>,
    /// Resource lists from `resources.yaml`. Empty when the project has no
    /// `resources.yaml` (a valid "no resources" configuration) or when the
    /// file failed to load — in the latter case a diagnostic explains why.
    pub resources: Resources,
    pub calendar: WorkingCalendar,
    /// Every diagnostic collected during loading: per-item and
    /// cross-item findings from [`Store::load`], plus `views_check`
    /// findings when `views.yaml` exists. May be empty for a healthy
    /// project.
    pub diagnostics: Vec<Diagnostic>,
}

/// Failures that prevent the loader from returning a [`Project`].
///
/// Only schema and items-directory failures are hard — without them
/// nothing useful can be served. Views.yaml problems (missing,
/// unparseable, semantically invalid) become diagnostics inside an
/// otherwise-successful `Project` so the server can still answer
/// `GET /api/views` with an empty list and a diagnostic explaining why.
#[derive(Debug, Error)]
pub enum LoadError {
    #[error("failed to load schema from {path}: {detail}")]
    Schema { path: PathBuf, detail: String },

    #[error("failed to read items directory {path}: {detail}")]
    Items { path: PathBuf, detail: String },
}

impl LoadError {
    /// Translate this hard load failure into the equivalent core
    /// [`Diagnostic`]: a file-scoped read error pinned to the path that
    /// failed. Lets any front-end (the HTTP server today, others later)
    /// surface a load failure through the same diagnostic channel as the
    /// soft per-item findings, without reaching into the diagnostic
    /// internals itself.
    pub fn to_diagnostic(&self) -> Diagnostic {
        let (path, detail) = match self {
            LoadError::Schema { path, detail } | LoadError::Items { path, detail } => {
                (path.clone(), detail.clone())
            }
        };
        Diagnostic::file(
            Severity::Error,
            path,
            FileDiagnosticKind::ReadError { detail },
        )
    }
}

/// Load every part of the project from disk into memory.
///
/// `config_path` is where `config.yaml` was read from — the loader
/// receives the already-parsed [`Config`] and would otherwise not know
/// the file's location, which `config_check` needs to pin its
/// diagnostics. Passed relative to `project_root` or absolute; joining
/// resolves both.
pub fn load_project(
    config: &Config,
    project_root: &Path,
    config_path: &Path,
) -> Result<Project, LoadError> {
    let schema_path = project_root.join(&config.schema);
    let items_path = project_root.join(&config.paths.work_items);
    let views_path = project_root.join(&config.paths.views);
    let resources_path = project_root.join(&config.paths.resources);
    let config_path = project_root.join(config_path);

    let schema = parser::schema::load_schema(&schema_path).map_err(|e| LoadError::Schema {
        path: schema_path.clone(),
        detail: e.to_string(),
    })?;

    let store = Store::load(&items_path, &schema).map_err(|e| LoadError::Items {
        path: items_path.clone(),
        detail: e.to_string(),
    })?;

    let mut diagnostics: Vec<Diagnostic> = store.diagnostics().to_vec();
    diagnostics.extend(store.detect_cycles(&schema));
    diagnostics.extend(crate::rules::evaluate(&store, &schema));

    // Parse views.yaml exactly once: load_and_check returns the parsed
    // views (None when absent or unparseable) together with its check
    // diagnostics, so we never re-read the file to populate `views`.
    let (views, views_diagnostics) = views_check::load_and_check(&views_path, &schema);
    diagnostics.extend(views_diagnostics);

    // Load resources.yaml the same way: absent is fine, a malformed file
    // becomes a diagnostic rather than a hard load failure.
    let (resources, resources_diagnostics) = resources_check::load_and_check(&resources_path);
    diagnostics.extend(resources_diagnostics);

    // Validate config.yaml's project-wide display-role defaults against
    // the schema. No file read here — the parsed config is already in
    // hand; only its path is needed to pin the diagnostics.
    diagnostics.extend(crate::config_check::evaluate(config, &schema, &config_path));

    let calendar = config.working_calendar();

    Ok(Project {
        store,
        schema,
        views,
        resources,
        calendar,
        diagnostics,
    })
}
