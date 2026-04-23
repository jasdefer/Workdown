//! `workdown validate` — rendering and output formatting.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use workdown_core::model::config::Config;
use workdown_core::model::diagnostic::{Diagnostic, DiagnosticKind};
use workdown_core::model::schema::Severity;
use workdown_core::store::Store;

use crate::cli;
use crate::cli::ValidateFormat;

// ── Public API ──────────────────────────────────────────────────────

/// Render validation results in the requested format.
///
/// `config` and `project_root` are threaded through to route view-level
/// diagnostics (which don't carry a path) to `config.paths.views`.
pub fn render(
    diagnostics: &[Diagnostic],
    store: &Store,
    format: ValidateFormat,
    config: &Config,
    project_root: &Path,
) {
    match format {
        ValidateFormat::Human => render_human(diagnostics, store, config, project_root),
        ValidateFormat::Json => render_json(diagnostics),
    }
}

// ── Human-readable output ───────────────────────────────────────────

/// Group diagnostics by source file path, sort warnings-before-errors
/// within each group, and render with styled output.
fn render_human(diagnostics: &[Diagnostic], store: &Store, config: &Config, project_root: &Path) {
    if diagnostics.is_empty() {
        cli::output::validation_summary(0, 0);
        return;
    }

    let (grouped, ungrouped) = group_by_file(diagnostics, store, config, project_root);

    let mut first = true;
    for (path, mut file_diagnostics) in grouped {
        sort_by_severity(&mut file_diagnostics);

        if !first {
            eprintln!();
        }
        first = false;

        cli::output::header(&path.display().to_string());
        for diagnostic in &file_diagnostics {
            let line = format_diagnostic_line(diagnostic);
            match diagnostic.severity {
                Severity::Warning => cli::output::warning(&line),
                Severity::Error => cli::output::error(&line),
            }
        }
    }

    if !ungrouped.is_empty() {
        let mut ungrouped = ungrouped;
        sort_by_severity(&mut ungrouped);

        if !first {
            eprintln!();
        }

        for diagnostic in &ungrouped {
            let line = format_diagnostic_line(diagnostic);
            match diagnostic.severity {
                Severity::Warning => cli::output::warning(&line),
                Severity::Error => cli::output::error(&line),
            }
        }
    }

    let error_count = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .count();
    let warning_count = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Warning)
        .count();
    cli::output::validation_summary(error_count, warning_count);
}

// ── JSON output ─────────────────────────────────────────────────────

fn render_json(diagnostics: &[Diagnostic]) {
    let json =
        serde_json::to_string_pretty(diagnostics).expect("diagnostics are always serializable");
    println!("{json}");
}

// ── Grouping helpers ────────────────────────────────────────────────

/// Group diagnostics by file path. Returns (grouped, ungrouped).
///
/// Uses the item's `source_path` to resolve `item_id` → file.
/// Diagnostics that can't be mapped to a single file (e.g. `DuplicateId`,
/// `Cycle`, `CountViolation`) go into the ungrouped bucket.
fn group_by_file<'a>(
    diagnostics: &'a [Diagnostic],
    store: &Store,
    config: &Config,
    project_root: &Path,
) -> (BTreeMap<PathBuf, Vec<&'a Diagnostic>>, Vec<&'a Diagnostic>) {
    let mut grouped: BTreeMap<PathBuf, Vec<&Diagnostic>> = BTreeMap::new();
    let mut ungrouped: Vec<&Diagnostic> = Vec::new();

    for diagnostic in diagnostics {
        match file_for_diagnostic(diagnostic, store, config, project_root) {
            Some(path) => grouped.entry(path).or_default().push(diagnostic),
            None => ungrouped.push(diagnostic),
        }
    }

    (grouped, ungrouped)
}

/// Try to resolve a diagnostic to the source file it belongs to.
fn file_for_diagnostic(
    diagnostic: &Diagnostic,
    store: &Store,
    config: &Config,
    project_root: &Path,
) -> Option<PathBuf> {
    match &diagnostic.kind {
        DiagnosticKind::FileError { path, .. } => Some(path.clone()),

        DiagnosticKind::InvalidFieldValue { item_id, .. }
        | DiagnosticKind::MissingRequired { item_id, .. }
        | DiagnosticKind::UnknownField { item_id, .. }
        | DiagnosticKind::BrokenLink { item_id, .. }
        | DiagnosticKind::RuleViolation { item_id, .. } => store
            .get(item_id.as_str())
            .map(|item| item.source_path.clone()),

        // These span multiple files or the whole collection.
        DiagnosticKind::DuplicateId { .. }
        | DiagnosticKind::Cycle { .. }
        | DiagnosticKind::CountViolation { .. } => None,

        // View diagnostics live in `config.paths.views`. (Parse-time read/YAML
        // failures surface as `FileError` above, already carrying the path.)
        DiagnosticKind::ViewDuplicateId { .. }
        | DiagnosticKind::ViewMissingSlot { .. }
        | DiagnosticKind::ViewUnknownField { .. }
        | DiagnosticKind::ViewFieldTypeMismatch { .. }
        | DiagnosticKind::ViewWhereParseError { .. }
        | DiagnosticKind::ViewBucketWithoutDateAxis { .. }
        | DiagnosticKind::ViewCountAggregateWithValue { .. }
        | DiagnosticKind::ViewAggregateTypeMismatch { .. } => {
            Some(project_root.join(&config.paths.views))
        }
    }
}

/// Sort diagnostics so warnings come first, errors last.
fn sort_by_severity(diagnostics: &mut [&Diagnostic]) {
    diagnostics.sort_by_key(|diagnostic| match diagnostic.severity {
        Severity::Warning => 0,
        Severity::Error => 1,
    });
}

/// Format the message part of a diagnostic (without the severity icon).
fn format_diagnostic_line(diagnostic: &Diagnostic) -> String {
    match &diagnostic.kind {
        // For item-level diagnostics under a file header, omit the item_id
        // (the file header already provides context) and show field + detail.
        DiagnosticKind::InvalidFieldValue { field, detail, .. } => {
            format!("field '{field}': {detail}")
        }
        DiagnosticKind::MissingRequired { field, .. } => {
            format!("required field '{field}' is missing")
        }
        DiagnosticKind::UnknownField { field, .. } => {
            format!("unknown field '{field}'")
        }
        DiagnosticKind::BrokenLink {
            field, target_id, ..
        } => {
            format!("field '{field}': broken link to '{target_id}'")
        }
        DiagnosticKind::RuleViolation { rule, detail, .. } => {
            format!("rule '{rule}': {detail}")
        }

        // File-level and ungrouped — use the full Display impl.
        _ => diagnostic.to_string(),
    }
}
