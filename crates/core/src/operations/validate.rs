//! Validation orchestration: load, check, and return structured results.

use std::path::Path;

use crate::model::config::Config;
use crate::model::diagnostic::Diagnostic;
use crate::model::schema::Severity;
use crate::parser;
use crate::parser::schema::SchemaLoadError;
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

/// Errors from the validation operation.
#[derive(Debug, thiserror::Error)]
pub enum ValidateError {
    #[error("failed to load schema: {0}")]
    SchemaLoad(#[from] SchemaLoadError),

    #[error("failed to read items directory: {0}")]
    StoreLoad(#[from] std::io::Error),
}

// ── Public API ──────────────────────────────────────────────────────

/// Run validation: load schema and store, collect all diagnostics.
///
/// Returns a [`ValidationResult`] with the full diagnostic list,
/// whether any errors were found, and the loaded store.
pub fn validate(config: &Config, project_root: &Path) -> Result<ValidationResult, ValidateError> {
    let schema_path = project_root.join(&config.schema);
    let items_path = project_root.join(&config.paths.work_items);

    tracing::debug!(schema = %schema_path.display(), "loading schema");
    let schema = parser::schema::load_schema(&schema_path)?;

    tracing::debug!(items = %items_path.display(), "loading work items");
    let store = Store::load(&items_path, &schema)?;

    let mut diagnostics = store.diagnostics().to_vec();
    diagnostics.extend(store.detect_cycles(&schema));
    diagnostics.extend(crate::rules::evaluate(&store, &schema));

    let has_errors = diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error);

    Ok(ValidationResult {
        diagnostics,
        has_errors,
        store,
    })
}
