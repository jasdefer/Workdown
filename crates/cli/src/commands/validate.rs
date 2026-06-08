//! `workdown validate` — rendering and output formatting.

use std::collections::BTreeMap;
use std::path::PathBuf;

use workdown_core::model::diagnostic::Diagnostic;
use workdown_core::model::schema::Severity;

use crate::cli;
use crate::cli::ValidateFormat;

// ── Public API ──────────────────────────────────────────────────────

/// Render validation results in the requested format.
pub fn render(diagnostics: &[Diagnostic], format: ValidateFormat) {
    match format {
        ValidateFormat::Human => render_human(diagnostics),
        ValidateFormat::Json => render_json(diagnostics),
    }
}

// ── Human-readable output ───────────────────────────────────────────

/// Group diagnostics by source file path, sort warnings-before-errors
/// within each group, and render with styled output.
fn render_human(diagnostics: &[Diagnostic]) {
    if diagnostics.is_empty() {
        cli::output::validation_summary(0, 0);
        return;
    }

    let (grouped, ungrouped) = group_by_file(diagnostics);

    let mut first = true;
    for (path, mut file_diagnostics) in grouped {
        sort_by_severity(&mut file_diagnostics);

        if !first {
            eprintln!();
        }
        first = false;

        cli::output::header(&path.display().to_string());
        for diagnostic in &file_diagnostics {
            match diagnostic.severity {
                Severity::Warning => cli::output::warning(&diagnostic.message),
                Severity::Error => cli::output::error(&diagnostic.message),
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
            match diagnostic.severity {
                Severity::Warning => cli::output::warning(&diagnostic.message),
                Severity::Error => cli::output::error(&diagnostic.message),
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
/// Uses [`Diagnostic::source_path`] — diagnostics that don't carry a single
/// source path (`Files`, `Collection`) go into the ungrouped bucket.
fn group_by_file(
    diagnostics: &[Diagnostic],
) -> (BTreeMap<PathBuf, Vec<&Diagnostic>>, Vec<&Diagnostic>) {
    let mut grouped: BTreeMap<PathBuf, Vec<&Diagnostic>> = BTreeMap::new();
    let mut ungrouped: Vec<&Diagnostic> = Vec::new();

    for diagnostic in diagnostics {
        match diagnostic.source_path() {
            Some(path) => grouped
                .entry(path.to_path_buf())
                .or_default()
                .push(diagnostic),
            None => ungrouped.push(diagnostic),
        }
    }

    (grouped, ungrouped)
}

/// Sort diagnostics so warnings come first, errors last.
fn sort_by_severity(diagnostics: &mut [&Diagnostic]) {
    diagnostics.sort_by_key(|diagnostic| match diagnostic.severity {
        Severity::Warning => 0,
        Severity::Error => 1,
    });
}
