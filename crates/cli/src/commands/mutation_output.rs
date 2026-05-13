//! Shared output rendering for `workdown` field-mutation commands.
//!
//! Each mutation command (`set`, `unset`, and the future `--append`,
//! `--remove`, `--delta` modes) picks the format that matches what the
//! user asked for. Pre-existing diagnostics surfaced after the write
//! flow through the same per-mode entry point so the call sites stay
//! uniform.

use std::process::ExitCode;

use workdown_core::operations::set::SetOutcome;

use crate::cli::output;

/// Which kind of mutation produced this outcome.
///
/// Drives the headline format only — the warning list and exit-code
/// derivation are identical across modes.
#[derive(Debug, Clone, Copy)]
pub enum MutationMode {
    Replace,
    Unset,
}

/// Print the per-mutation headline plus every warning, and return the
/// exit code derived from `outcome.mutation_caused_warning`.
pub fn render_outcome(
    id: &str,
    field: &str,
    mode: MutationMode,
    outcome: &SetOutcome,
) -> ExitCode {
    let headline = match mode {
        MutationMode::Replace => format_replace(field, outcome),
        MutationMode::Unset => format_unset(field, outcome),
    };
    output::success(&format!("{id}: {headline}"));

    for warning in &outcome.warnings {
        output::warning(&warning.to_string());
    }

    if outcome.mutation_caused_warning {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

// ── Per-mode formatters ──────────────────────────────────────────────

fn format_replace(field: &str, outcome: &SetOutcome) -> String {
    let previous = render_optional(outcome.previous_value.as_ref());
    let new = render_optional(outcome.new_value.as_ref());
    format!("{field}: {previous} → {new}")
}

fn format_unset(field: &str, outcome: &SetOutcome) -> String {
    match &outcome.previous_value {
        Some(value) => format!("{field}: {} → (cleared)", format_yaml_value(value)),
        None => format!("{field}: (already absent)"),
    }
}

// ── Value rendering ──────────────────────────────────────────────────

fn render_optional(value: Option<&serde_yaml::Value>) -> String {
    match value {
        None => "(unset)".to_owned(),
        Some(value) => format_yaml_value(value),
    }
}

fn format_yaml_value(value: &serde_yaml::Value) -> String {
    match value {
        serde_yaml::Value::Null => "(null)".to_owned(),
        serde_yaml::Value::String(string) => string.clone(),
        serde_yaml::Value::Bool(boolean) => boolean.to_string(),
        serde_yaml::Value::Number(number) => number.to_string(),
        serde_yaml::Value::Sequence(items) => {
            let inner: Vec<String> = items.iter().map(format_yaml_value).collect();
            format!("[{}]", inner.join(", "))
        }
        serde_yaml::Value::Mapping(_) | serde_yaml::Value::Tagged(_) => {
            serde_yaml::to_string(value)
                .unwrap_or_default()
                .trim()
                .to_owned()
        }
    }
}
