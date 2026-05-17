//! `workdown move` — shortcut for setting the project's board field.
//!
//! Reads `config.defaults.board_field`, type-shapes the value through
//! the schema, and delegates to
//! `workdown_core::operations::set::run_set` with
//! `SetOperation::Replace`. No new core operation: `move` is purely a
//! CLI affordance over `set`.

use std::path::Path;
use std::process::ExitCode;

use workdown_core::model::config::Config;
use workdown_core::model::WorkItemId;
use workdown_core::operations::frontmatter_io;
use workdown_core::operations::set::{run_set, SetOperation};

use crate::cli::output;
use crate::commands::mutation_output::{render_outcome, MutationMode};

pub fn run_move_command(
    config: &Config,
    project_root: &Path,
    id: &str,
    value: &str,
) -> anyhow::Result<ExitCode> {
    let field = config.defaults.board_field.as_str();

    // Load the schema to type-shape the value. The board field name
    // comes from config, not from the user, so a miss here points at a
    // misconfigured config key — call that out explicitly rather than
    // letting `run_set` surface the generic `UnknownField` message.
    let schema_path = project_root.join(&config.schema);
    let schema = workdown_core::parser::schema::load_schema(&schema_path)
        .map_err(|e| anyhow::anyhow!("failed to load schema: {e}"))?;

    let field_definition = match schema.fields.get(field) {
        Some(field_def) => field_def,
        None => {
            output::error(&format!(
                "board field '{field}' (from `defaults.board_field` in config.yaml) \
                 is not defined in schema"
            ));
            return Ok(ExitCode::FAILURE);
        }
    };

    let parsed_value = frontmatter_io::parse_value_for_field(value, field_definition);
    let work_item_id = WorkItemId::from(id.to_owned());

    match run_set(
        config,
        project_root,
        &work_item_id,
        field,
        SetOperation::Replace(parsed_value),
    ) {
        Ok(outcome) => Ok(render_outcome(id, field, MutationMode::Replace, &outcome)),
        Err(error) => {
            output::error(&error.to_string());
            Ok(ExitCode::FAILURE)
        }
    }
}
