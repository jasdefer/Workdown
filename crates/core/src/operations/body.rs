//! `workdown body` — replace the freeform Markdown body of a work item.
//!
//! Body content is freeform by design — no schema validation applies, so
//! this command's only job is to splice a new body onto the existing
//! frontmatter bytes. The frontmatter is left byte-identical: we slice
//! the file at the body offset rather than re-emitting the YAML.
//!
//! Interactive editing is out of scope — the user opens the `.md` file
//! directly. This command exists for non-interactive callers (the web
//! UI, scripts).

use std::path::{Path, PathBuf};

use crate::model::config::Config;
use crate::model::diagnostic::Diagnostic;
use crate::model::WorkItemId;
use crate::operations::frontmatter_io::write_file_atomically;
use crate::parser;
use crate::parser::schema::SchemaLoadError;

// ── Public types ─────────────────────────────────────────────────────

/// The outcome of a successful `workdown body`.
#[derive(Debug)]
pub struct BodyOutcome {
    /// Path to the file that was written.
    pub path: PathBuf,
    /// The body string before the write — exactly as it appeared on disk.
    pub previous_body: String,
    /// The body string after the write — normalised (trailing whitespace-only
    /// lines collapsed to a single `\n`, or empty if the body is empty).
    pub new_body: String,
    /// All non-blocking diagnostics from the post-write store reload plus
    /// rule evaluation. Body content cannot itself produce warnings, but
    /// pre-existing warnings elsewhere in the project are surfaced here per
    /// the milestone's "always show all" convention.
    pub warnings: Vec<Diagnostic>,
}

/// Errors returned by [`run_body_replace`].
///
/// All variants are hard-fails: nothing is written to disk.
#[derive(Debug, thiserror::Error)]
pub enum BodyError {
    #[error("failed to load schema: {0}")]
    SchemaLoad(#[from] SchemaLoadError),

    #[error("failed to load work items: {0}")]
    StoreLoad(#[from] std::io::Error),

    #[error("unknown work item '{id}'")]
    UnknownItem { id: String },

    #[error("failed to read '{path}': {source}")]
    ReadTarget {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse '{path}': {source}")]
    ParseTarget {
        path: PathBuf,
        source: parser::ParseError,
    },

    #[error("failed to write '{path}': {source}")]
    WriteFile {
        path: PathBuf,
        source: std::io::Error,
    },
}

// ── Public API ───────────────────────────────────────────────────────

/// Replace the freeform Markdown body of `id`.
///
/// The frontmatter bytes are preserved verbatim — we slice the on-disk
/// file at the body offset and write `frontmatter_bytes + normalised_body`.
/// This is the only mutation in the CLI that does not round-trip the
/// frontmatter through serde_yaml.
///
/// Trailing newline rule: exactly one `\n` for a non-empty body, none for
/// an empty body. Trailing `\r` is stripped alongside `\n` so CRLF input
/// doesn't leave a dangling carriage return.
pub fn run_body_replace(
    config: &Config,
    project_root: &Path,
    id: &WorkItemId,
    new_body: String,
) -> Result<BodyOutcome, BodyError> {
    let schema_path = project_root.join(&config.schema);
    let schema = parser::schema::load_schema(&schema_path)?;

    let items_path = project_root.join(&config.paths.work_items);
    // A missing or malformed resources.yaml degrades to empty resources
    // here — `workdown validate` owns reporting it.
    let (resources, _) =
        crate::resources_check::load_and_check(&project_root.join(&config.paths.resources));
    let store = crate::store::Store::load_with_resources(&items_path, &schema, &resources)?;

    let work_item = store
        .get(id.as_str())
        .ok_or_else(|| BodyError::UnknownItem { id: id.to_string() })?;
    let file_path = work_item.source_path.clone();

    let file_content =
        std::fs::read_to_string(&file_path).map_err(|source| BodyError::ReadTarget {
            path: file_path.clone(),
            source,
        })?;

    let (_frontmatter, body_offset) =
        parser::split_frontmatter_with_body_offset(&file_content, &file_path).map_err(
            |source| BodyError::ParseTarget {
                path: file_path.clone(),
                source,
            },
        )?;

    let previous_body = file_content.get(body_offset..).unwrap_or("").to_owned();
    let normalised_new_body = normalise_body(&new_body);

    let frontmatter_bytes = file_content.get(..body_offset).unwrap_or("");
    let new_file_content = format!("{frontmatter_bytes}{normalised_new_body}");

    write_file_atomically(&file_path, &new_file_content).map_err(|source| {
        BodyError::WriteFile {
            path: file_path.clone(),
            source,
        }
    })?;

    // Reload and surface every diagnostic. Body content cannot itself
    // introduce a new warning (rules and schema only look at frontmatter),
    // so there is no pre/post diff — the post-write snapshot is what the
    // user sees.
    let reloaded = crate::store::Store::load_with_resources(&items_path, &schema, &resources)?;
    let mut post_diagnostics: Vec<Diagnostic> = reloaded.diagnostics().to_vec();
    post_diagnostics.extend(crate::rules::evaluate(&reloaded, &schema));

    Ok(BodyOutcome {
        path: file_path,
        previous_body,
        new_body: normalised_new_body,
        warnings: post_diagnostics,
    })
}

// ── Internals ────────────────────────────────────────────────────────

/// Normalise the body's trailing whitespace into the canonical form:
/// exactly one `\n` for a non-empty body, none for an empty one.
///
/// Strips both `\n` and `\r` from the tail so CRLF input from Windows
/// editors doesn't leave a dangling carriage return.
fn normalise_body(body: &str) -> String {
    let trimmed = body.trim_end_matches(['\n', '\r']);
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}\n")
    }
}
