//! `workdown set` — replace a single field on a work item.
//!
//! Thin wrapper over `workdown_core::operations::set::run_set` that
//! parses the user's string value against the schema field type and
//! delegates rendering to [`super::mutation_output`].

use std::path::Path;
use std::process::ExitCode;

use workdown_core::model::config::Config;
use workdown_core::model::WorkItemId;
use workdown_core::operations::frontmatter_io;
use workdown_core::operations::set::{run_set, SetOperation};

use crate::cli::output;
use crate::commands::mutation_output::{render_outcome, MutationMode};

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
        Ok(outcome) => Ok(render_outcome(id, field, MutationMode::Replace, &outcome)),
        Err(error) => {
            output::error(&error.to_string());
            Ok(ExitCode::FAILURE)
        }
    }
}
