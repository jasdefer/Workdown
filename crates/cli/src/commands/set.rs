//! `workdown set` — replace a single field on a work item.
//!
//! Thin wrapper over `workdown_core::operations::set::run_set` that
//! parses the user's string value against the schema field type and
//! renders the change as a human-readable line plus any post-write
//! warnings.

use std::path::Path;
use std::process::ExitCode;

use workdown_core::model::config::Config;
use workdown_core::model::WorkItemId;
use workdown_core::operations::frontmatter_io;
use workdown_core::operations::set::{run_set, SetOperation, SetOutcome};

use crate::cli::output;

pub fn run_set_command(
    config: &Config,
    project_root: &Path,
    id: &str,
    field: &str,
    value_str: &str,
) -> anyhow::Result<ExitCode> {
    // Load the schema so we can type-shape the user's string value.
    // Core also performs an UnknownField check; doing it here too gives
    // a friendlier error before the rest of the load happens.
    let schema_path = project_root.join(&config.schema);
    let schema = workdown_core::parser::schema::load_schema(&schema_path)
        .map_err(|e| anyhow::anyhow!("failed to load schema: {e}"))?;

    let value = match schema.fields.get(field) {
        Some(field_def) => frontmatter_io::parse_value_for_field(value_str, field_def),
        None => {
            output::error(&format!("unknown field '{field}' (not defined in schema)"));
            return Ok(ExitCode::FAILURE);
        }
    };

    let work_item_id = WorkItemId::from(id.to_owned());
    let operation = SetOperation::Replace(value);

    match run_set(config, project_root, &work_item_id, field, operation) {
        Ok(outcome) => {
            render_change_line(id, field, &outcome);
            for warning in &outcome.warnings {
                output::warning(&warning.to_string());
            }
            if outcome.mutation_caused_warning {
                Ok(ExitCode::FAILURE)
            } else {
                Ok(ExitCode::SUCCESS)
            }
        }
        Err(error) => {
            output::error(&error.to_string());
            Ok(ExitCode::FAILURE)
        }
    }
}

/// Render the milestone-table replace-mode output:
/// `task-1: status: open → in_progress`.
fn render_change_line(id: &str, field: &str, outcome: &SetOutcome) {
    let previous = render_value(outcome.previous_value.as_ref());
    let new = render_value(outcome.new_value.as_ref());
    output::success(&format!("{id}: {field}: {previous} → {new}"));
}

/// Render a YAML value for display. `None` becomes `(unset)`; everything
/// else uses serde_yaml's compact scalar form (sequences render as
/// `[a, b]`, scalars unquoted).
fn render_value(value: Option<&serde_yaml::Value>) -> String {
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
