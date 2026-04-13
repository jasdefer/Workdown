//! User-facing output helpers: styled messages and table rendering.
//!
//! All terminal output for the end user goes through this module.
//! Logging (`tracing`) is for developer diagnostics; this is for results.

use comfy_table::presets::UTF8_FULL_CONDENSED;
use comfy_table::{ContentArrangement, Table};
use console::{style, Term};

// ── Styled messages ──────────────────────────────────────────────────

/// Print a success message (green checkmark).
pub fn success(message: &str) {
    let term = Term::stderr();
    let _ = term.write_line(&format!("{} {message}", style("✔").green().bold()));
}

/// Print a warning message (yellow exclamation).
pub fn warning(message: &str) {
    let term = Term::stderr();
    let _ = term.write_line(&format!("{} {message}", style("!").yellow().bold()));
}

/// Print an error message (red cross).
pub fn error(message: &str) {
    let term = Term::stderr();
    let _ = term.write_line(&format!("{} {message}", style("✖").red().bold()));
}

/// Print an info message (blue arrow).
pub fn info(message: &str) {
    let term = Term::stderr();
    let _ = term.write_line(&format!("{} {message}", style("→").cyan().bold()));
}

/// Print a header line (bold, underlined).
pub fn header(message: &str) {
    let term = Term::stderr();
    let _ = term.write_line(&format!("{}", style(message).bold().underlined()));
}

// ── Table rendering ──────────────────────────────────────────────────

/// Create a styled table with the given column headers.
///
/// Returns a [`Table`] ready for rows. Call `table.add_row(...)` then
/// print with `println!("{table}")`.
///
/// ```ignore
/// let mut table = output::table(&["ID", "Title", "Status"]);
/// table.add_row(&["fix-login", "Fix login bug", "open"]);
/// println!("{table}");
/// ```
pub fn table(headers: &[&str]) -> Table {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(headers);
    table
}

// ── Summary line ─────────────────────────────────────────────────────

/// Print a validation summary (e.g. "3 errors, 1 warning").
pub fn validation_summary(error_count: usize, warning_count: usize) {
    let errors = if error_count == 1 {
        format!("{} {}", style("1").red().bold(), "error")
    } else {
        format!("{} {}", style(error_count).red().bold(), "errors")
    };

    let warnings = if warning_count == 1 {
        format!("{} {}", style("1").yellow().bold(), "warning")
    } else {
        format!(
            "{} {}",
            style(warning_count).yellow().bold(),
            "warnings"
        )
    };

    let term = Term::stderr();
    if error_count > 0 && warning_count > 0 {
        let _ = term.write_line(&format!("\n{errors}, {warnings}"));
    } else if error_count > 0 {
        let _ = term.write_line(&format!("\n{errors}"));
    } else if warning_count > 0 {
        let _ = term.write_line(&format!("\n{warnings}"));
    } else {
        success("No issues found");
    }
}
