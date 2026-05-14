//! `workdown unset` — clear a single field on a work item.
//!
//! Thin wrapper over `workdown_core::operations::set::run_set` with
//! `SetOperation::Unset`. No value parsing needed (and so no schema
//! preload here, unlike [`super::set`]).

use std::path::Path;
use std::process::ExitCode;

use workdown_core::model::config::Config;
use workdown_core::model::WorkItemId;
use workdown_core::operations::set::{run_set, SetOperation};

use crate::cli::output;
use crate::commands::mutation_output::{render_outcome, MutationMode};

pub fn run_unset_command(
    config: &Config,
    project_root: &Path,
    id: &str,
    field: &str,
) -> anyhow::Result<ExitCode> {
    let work_item_id = WorkItemId::from(id.to_owned());

    match run_set(
        config,
        project_root,
        &work_item_id,
        field,
        SetOperation::Unset,
    ) {
        Ok(outcome) => Ok(render_outcome(id, field, MutationMode::Unset, &outcome)),
        Err(error) => {
            output::error(&error.to_string());
            Ok(ExitCode::FAILURE)
        }
    }
}
