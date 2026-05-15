//! `workdown set` — mutate a single field on a work item.
//!
//! Thin wrapper over `workdown_core::operations::set::run_set`. Parses
//! the user's input per mode (replace / append / remove), builds the
//! matching `SetOperation`, and delegates rendering to
//! [`super::mutation_output`].

use std::path::Path;
use std::process::ExitCode;

use workdown_core::model::config::Config;
use workdown_core::model::schema::{FieldDefinition, FieldTypeConfig};
use workdown_core::model::WorkItemId;
use workdown_core::operations::frontmatter_io;
use workdown_core::operations::set::{
    run_set, BooleanMode, CollectionMode, DateMode, DurationMode, NumericMode, SetOperation,
};

use crate::cli::output;
use crate::commands::mutation_output::{render_outcome, MutationMode};

/// Which CLI-side mode the user invoked. Clap's `ArgGroup` enforces
/// exactly one of these, so we know one is `Some` by the time we get
/// here.
pub enum CliSetMode {
    Replace(String),
    Append(String),
    Remove(String),
    Delta(String),
    Toggle,
}

pub fn run_set_command(
    config: &Config,
    project_root: &Path,
    id: &str,
    field: &str,
    mode: CliSetMode,
) -> anyhow::Result<ExitCode> {
    // Load the schema so we can type-shape the user's string value for
    // `Replace`, and so an unknown field errors here (friendlier than
    // the core's `UnknownField` message that surfaces after the store
    // load).
    let schema_path = project_root.join(&config.schema);
    let schema = workdown_core::parser::schema::load_schema(&schema_path)
        .map_err(|e| anyhow::anyhow!("failed to load schema: {e}"))?;

    let field_definition = match schema.fields.get(field) {
        Some(field_def) => field_def,
        None => {
            output::error(&format!("unknown field '{field}' (not defined in schema)"));
            return Ok(ExitCode::FAILURE);
        }
    };

    let (operation, mutation_mode) = match mode {
        CliSetMode::Replace(value_str) => {
            let value = frontmatter_io::parse_value_for_field(&value_str, field_definition);
            (SetOperation::Replace(value), MutationMode::Replace)
        }
        CliSetMode::Append(value_str) => {
            let values = frontmatter_io::parse_collection_values(&value_str);
            (
                SetOperation::Collection(CollectionMode::Append(values.clone())),
                MutationMode::Append(values),
            )
        }
        CliSetMode::Remove(value_str) => {
            let values = frontmatter_io::parse_collection_values(&value_str);
            (
                SetOperation::Collection(CollectionMode::Remove(values.clone())),
                MutationMode::Remove(values),
            )
        }
        CliSetMode::Delta(value_str) => match build_delta(&value_str, field, field_definition) {
            Ok(parts) => parts,
            Err(message) => {
                output::error(&message);
                return Ok(ExitCode::FAILURE);
            }
        },
        CliSetMode::Toggle => (
            SetOperation::Boolean(BooleanMode::Toggle),
            MutationMode::Toggle,
        ),
    };

    let work_item_id = WorkItemId::from(id.to_owned());

    match run_set(config, project_root, &work_item_id, field, operation) {
        Ok(outcome) => Ok(render_outcome(id, field, mutation_mode, &outcome)),
        Err(error) => {
            output::error(&error.to_string());
            Ok(ExitCode::FAILURE)
        }
    }
}

/// Parse a `--delta` value string per the field's type and pair the
/// resulting [`SetOperation`] with the [`MutationMode`] for rendering.
///
/// Pre-rejects field types that don't support `--delta` (with a more
/// helpful message than the core's generic `ModeNotValidForFieldType`).
/// On parse failure, returns the human-readable message for the CLI
/// error renderer.
fn build_delta(
    value_str: &str,
    field: &str,
    field_definition: &FieldDefinition,
) -> Result<(SetOperation, MutationMode), String> {
    use workdown_core::model::duration::{format_duration_seconds, parse_duration};

    match &field_definition.type_config {
        FieldTypeConfig::Integer { .. } => {
            let parsed: i64 = value_str.parse().map_err(|_| {
                format!("invalid integer for --delta on '{field}': '{value_str}'")
            })?;
            let operation =
                SetOperation::Numeric(NumericMode::Delta(serde_yaml::Number::from(parsed)));
            let mutation_mode = MutationMode::Delta {
                operand: parsed.unsigned_abs().to_string(),
                is_negative: parsed < 0,
            };
            Ok((operation, mutation_mode))
        }
        FieldTypeConfig::Float { .. } => {
            let parsed: f64 = value_str
                .parse()
                .map_err(|_| format!("invalid float for --delta on '{field}': '{value_str}'"))?;
            let operation =
                SetOperation::Numeric(NumericMode::Delta(serde_yaml::Number::from(parsed)));
            let mutation_mode = MutationMode::Delta {
                operand: parsed.abs().to_string(),
                is_negative: parsed < 0.0,
            };
            Ok((operation, mutation_mode))
        }
        FieldTypeConfig::Duration { .. } => {
            let seconds = parse_duration(value_str).map_err(|error| {
                format!("invalid duration for --delta on '{field}': {error}")
            })?;
            let operation = SetOperation::Duration(DurationMode::Delta(seconds));
            let mutation_mode = MutationMode::Delta {
                operand: format_duration_seconds(seconds.saturating_abs()),
                is_negative: seconds < 0,
            };
            Ok((operation, mutation_mode))
        }
        FieldTypeConfig::Date => {
            let seconds = parse_duration(value_str).map_err(|error| {
                format!("invalid duration for --delta on '{field}': {error}")
            })?;
            let operation = SetOperation::Date(DateMode::Delta(seconds));
            let mutation_mode = MutationMode::Delta {
                operand: format_duration_seconds(seconds.saturating_abs()),
                is_negative: seconds < 0,
            };
            Ok((operation, mutation_mode))
        }
        _ => Err(format!(
            "--delta is not valid for field '{field}' (type: {}); use it on integer, float, duration, or date fields",
            field_definition.field_type()
        )),
    }
}
