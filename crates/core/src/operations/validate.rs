//! Validation orchestration: load the project, return structured results.
//!
//! Thin wrapper over [`crate::project::load_project`] — that function is
//! the single source of truth for loading + collecting diagnostics
//! across CLI and HTTP server. This module adds the `has_errors` flag
//! the CLI uses to set its exit code, plus the `ValidationResult` shape
//! callers already depend on.

use std::path::Path;

use crate::model::config::Config;
use crate::model::diagnostic::Diagnostic;
use crate::model::schema::Severity;
use crate::project::{load_project, LoadError};
use crate::store::Store;

// ── Public types ────────────────────────────────────────────────────

/// The structured result of a validation run.
pub struct ValidationResult {
    /// All diagnostics found.
    pub diagnostics: Vec<Diagnostic>,
    /// Whether any diagnostic is a blocking error.
    pub has_errors: bool,
    /// The loaded store (needed by callers for diagnostic-to-file mapping).
    pub store: Store,
}

/// Errors from the validation operation. Re-export of [`LoadError`] so
/// existing call sites keep their type name; the variants are the same.
pub type ValidateError = LoadError;

// ── Public API ──────────────────────────────────────────────────────

/// Run validation: load schema, store, and views, collect all diagnostics.
///
/// `config_path` is where `config.yaml` was read from — forwarded to
/// [`load_project`] so config-scope diagnostics can point at it.
pub fn validate(
    config: &Config,
    project_root: &Path,
    config_path: &Path,
) -> Result<ValidationResult, ValidateError> {
    let project = load_project(config, project_root, config_path)?;
    let has_errors = project
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error);
    Ok(ValidationResult {
        diagnostics: project.diagnostics,
        has_errors,
        store: project.store,
    })
}
