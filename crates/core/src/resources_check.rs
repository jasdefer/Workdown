//! Load `resources.yaml` and route any load failure into the diagnostic
//! stream — the resources counterpart to [`crate::views_check`].
//!
//! There are no semantic checks here yet: the only failure mode is a file
//! that exists but cannot be read or parsed. Validating that an item's
//! stored value matches a known resource id is a separate concern (the
//! `resource-option-lists` issue) and will grow its own `evaluate` here
//! when it lands.

use std::path::Path;

use crate::model::diagnostic::{Diagnostic, FileDiagnosticKind};
use crate::model::resources::Resources;
use crate::model::schema::Severity;
use crate::parser::resources::{load_resources, ResourcesLoadError};

/// Load `resources.yaml` from disk and return the parsed resources along
/// with any diagnostics produced.
///
/// Returns `(Resources::default(), [])` when the file is absent —
/// `resources.yaml` is optional. On an I/O or YAML-parse failure returns
/// `(Resources::default(), [diagnostic])` so the project still loads and
/// the failure surfaces through the same channel as every other finding.
pub fn load_and_check(resources_path: &Path) -> (Resources, Vec<Diagnostic>) {
    if !resources_path.exists() {
        return (Resources::default(), Vec::new());
    }
    match load_resources(resources_path) {
        Ok(resources) => (resources, Vec::new()),
        Err(error) => (
            Resources::default(),
            parse_errors_to_diagnostics(error, resources_path),
        ),
    }
}

/// Convert a [`ResourcesLoadError`] into a single file-scope diagnostic
/// pointed at `resources_path`. The detail carries the underlying I/O or
/// serde message.
pub fn parse_errors_to_diagnostics(
    error: ResourcesLoadError,
    resources_path: &Path,
) -> Vec<Diagnostic> {
    let detail = match error {
        ResourcesLoadError::ReadFailed(io) => io.to_string(),
        ResourcesLoadError::InvalidYaml(yaml) => yaml.to_string(),
    };
    vec![Diagnostic::file(
        Severity::Error,
        resources_path.to_path_buf(),
        FileDiagnosticKind::ReadError { detail },
    )]
}
