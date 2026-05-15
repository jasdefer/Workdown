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
/// derivation are identical across modes. Variants that carry the
/// operand (e.g. `Append`/`Remove`) need it for the operator line:
/// the outcome's previous/new values don't contain the user's input
/// directly.
#[derive(Debug, Clone)]
pub enum MutationMode {
    Replace,
    Unset,
    Append(Vec<serde_yaml::Value>),
    Remove(Vec<serde_yaml::Value>),
    /// Numeric / duration / date delta. `operand` is the **magnitude**
    /// (sign already stripped); `is_negative` drives the displayed
    /// operator. Carrying the operand here keeps the renderer
    /// independent of how the core represents the delta internally
    /// (signed `i64`, `Number`, etc.).
    Delta {
        operand: String,
        is_negative: bool,
    },
    Toggle,
}

/// Print the per-mutation headline plus every warning, and return the
/// exit code derived from `outcome.mutation_caused_warning`.
///
/// Order: headline → operation-level info messages → diagnostic
/// warnings. Info messages describe what the operation did (e.g. a
/// duplicate append) and never affect the exit code; warnings come from
/// the post-write store reload and do.
pub fn render_outcome(id: &str, field: &str, mode: MutationMode, outcome: &SetOutcome) -> ExitCode {
    let headline = match &mode {
        MutationMode::Replace => format_replace(field, outcome),
        MutationMode::Unset => format_unset(field, outcome),
        MutationMode::Append(operand) => format_append(field, operand, outcome),
        MutationMode::Remove(operand) => format_remove(field, operand, outcome),
        MutationMode::Delta {
            operand,
            is_negative,
        } => format_delta(field, operand, *is_negative, outcome),
        MutationMode::Toggle => format_toggle(field, outcome),
    };
    output::success(&format!("{id}: {headline}"));

    for info_message in &outcome.info_messages {
        output::info(info_message);
    }

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

fn format_append(field: &str, operand: &[serde_yaml::Value], outcome: &SetOutcome) -> String {
    let previous = format_sequence_value(outcome.previous_value.as_ref());
    let operand_rendered = format_sequence_literal(operand);
    let new = format_sequence_value(outcome.new_value.as_ref());
    format!("{field}: {previous} + {operand_rendered} = {new}")
}

fn format_remove(field: &str, operand: &[serde_yaml::Value], outcome: &SetOutcome) -> String {
    let previous = format_sequence_value(outcome.previous_value.as_ref());
    let operand_rendered = format_sequence_literal(operand);
    let new = format_sequence_value(outcome.new_value.as_ref());
    format!("{field}: {previous} - {operand_rendered} = {new}")
}

fn format_delta(field: &str, operand: &str, is_negative: bool, outcome: &SetOutcome) -> String {
    // Preconditions guarantee both values are present for delta — keep
    // defensive fallbacks just in case future flows produce a missing
    // side, but the typical path never hits them.
    let previous = render_optional(outcome.previous_value.as_ref());
    let new = render_optional(outcome.new_value.as_ref());
    let operator = if is_negative { "-" } else { "+" };
    format!("{field}: {previous} {operator} {operand} = {new}")
}

fn format_toggle(field: &str, outcome: &SetOutcome) -> String {
    let previous = render_optional(outcome.previous_value.as_ref());
    let new = render_optional(outcome.new_value.as_ref());
    format!("{field}: {previous} → {new}")
}

/// Render a possibly-absent field value as a sequence literal. Used by
/// collection-mode formatters where the displayed shape is always a
/// list — an absent field renders as `[]`, a scalar (defensive: file
/// hand-edited to a non-sequence) gets wrapped in brackets.
fn format_sequence_value(value: Option<&serde_yaml::Value>) -> String {
    match value {
        Some(serde_yaml::Value::Sequence(items)) => format_sequence_literal(items),
        Some(other) => format!("[{}]", format_yaml_value(other)),
        None => "[]".to_owned(),
    }
}

fn format_sequence_literal(items: &[serde_yaml::Value]) -> String {
    let rendered: Vec<String> = items.iter().map(format_yaml_value).collect();
    format!("[{}]", rendered.join(", "))
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
