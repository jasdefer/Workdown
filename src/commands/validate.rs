//! `workdown validate` — validate all work items against the schema.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::cli::ValidateFormat;
use crate::model::config::Config;
use crate::model::diagnostic::{Diagnostic, DiagnosticKind};
use crate::model::schema::Severity;
use crate::store::Store;
use crate::{cli, parser};

// ── Public API ──────────────────────────────────────────────────────

/// Run the validate command. Returns `true` if there are errors.
///
/// Paths in `config` are relative to `project_root` (the working directory).
pub fn run_validate(config: &Config, project_root: &Path, format: ValidateFormat) -> anyhow::Result<bool> {
    let schema_path = project_root.join(&config.schema);
    let items_path = project_root.join(&config.paths.work_items);

    tracing::debug!(schema = %schema_path.display(), "loading schema");
    let schema = parser::schema::load_schema(&schema_path)
        .map_err(|e| anyhow::anyhow!("failed to load schema: {e}"))?;

    tracing::debug!(items = %items_path.display(), "loading work items");
    let store = Store::load(&items_path, &schema)
        .map_err(|e| anyhow::anyhow!("failed to read items directory: {e}"))?;

    let mut diagnostics = store.diagnostics().to_vec();
    diagnostics.extend(store.detect_cycles(&schema));
    diagnostics.extend(crate::rules::evaluate(&store, &schema));

    let has_errors = diagnostics.iter().any(|d| d.severity == Severity::Error);

    match format {
        ValidateFormat::Human => render_human(&diagnostics, &store),
        ValidateFormat::Json => render_json(&diagnostics),
    }

    Ok(has_errors)
}

// ── Human-readable output ───────────────────────────────────────────

/// Group diagnostics by source file path, sort warnings-before-errors
/// within each group, and render with styled output.
fn render_human(diagnostics: &[Diagnostic], store: &Store) {
    if diagnostics.is_empty() {
        cli::output::validation_summary(0, 0);
        return;
    }

    let (grouped, ungrouped) = group_by_file(diagnostics, store);

    let mut first = true;
    for (path, mut diags) in grouped {
        sort_by_severity(&mut diags);

        if !first {
            eprintln!();
        }
        first = false;

        cli::output::header(&path.display().to_string());
        for d in &diags {
            let line = format_diagnostic_line(d);
            match d.severity {
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

        for d in &ungrouped {
            let line = format_diagnostic_line(d);
            match d.severity {
                Severity::Warning => cli::output::warning(&line),
                Severity::Error => cli::output::error(&line),
            }
        }
    }

    let error_count = diagnostics.iter().filter(|d| d.severity == Severity::Error).count();
    let warning_count = diagnostics.iter().filter(|d| d.severity == Severity::Warning).count();
    cli::output::validation_summary(error_count, warning_count);
}

// ── JSON output ─────────────────────────────────────────────────────

fn render_json(diagnostics: &[Diagnostic]) {
    let json = serde_json::to_string_pretty(diagnostics)
        .expect("diagnostics are always serializable");
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
) -> (BTreeMap<PathBuf, Vec<&'a Diagnostic>>, Vec<&'a Diagnostic>) {
    let mut grouped: BTreeMap<PathBuf, Vec<&Diagnostic>> = BTreeMap::new();
    let mut ungrouped: Vec<&Diagnostic> = Vec::new();

    for d in diagnostics {
        match file_for_diagnostic(d, store) {
            Some(path) => grouped.entry(path).or_default().push(d),
            None => ungrouped.push(d),
        }
    }

    (grouped, ungrouped)
}

/// Try to resolve a diagnostic to the source file it belongs to.
fn file_for_diagnostic(d: &Diagnostic, store: &Store) -> Option<PathBuf> {
    match &d.kind {
        DiagnosticKind::FileError { path, .. } => Some(path.clone()),

        DiagnosticKind::InvalidFieldValue { item_id, .. }
        | DiagnosticKind::MissingRequired { item_id, .. }
        | DiagnosticKind::UnknownField { item_id, .. }
        | DiagnosticKind::BrokenLink { item_id, .. }
        | DiagnosticKind::RuleViolation { item_id, .. } => {
            store.get(item_id).map(|item| item.source_path.clone())
        }

        // These span multiple files or the whole collection.
        DiagnosticKind::DuplicateId { .. }
        | DiagnosticKind::Cycle { .. }
        | DiagnosticKind::CountViolation { .. } => None,
    }
}

/// Sort diagnostics so warnings come first, errors last.
fn sort_by_severity(diags: &mut [&Diagnostic]) {
    diags.sort_by_key(|d| match d.severity {
        Severity::Warning => 0,
        Severity::Error => 1,
    });
}

/// Format the message part of a diagnostic (without the severity icon).
fn format_diagnostic_line(d: &Diagnostic) -> String {
    match &d.kind {
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
        _ => d.to_string(),
    }
}
