//! `workdown rename` — change a work item's id.
//!
//! Thin wrapper over [`workdown_core::operations::rename::run_rename`].
//! Parses CLI arguments into `RenameOptions`, hands off to core, and
//! delegates output to [`mutation_output::render_rename_outcome`].

use std::path::Path;
use std::process::ExitCode;

use workdown_core::model::config::Config;
use workdown_core::model::WorkItemId;
use workdown_core::operations::rename::{run_rename, RenameOptions};

use crate::cli::output;
use crate::commands::mutation_output::render_rename_outcome;

pub fn run_rename_command(
    config: &Config,
    project_root: &Path,
    old_id: &str,
    new_id: &str,
    dry_run: bool,
) -> anyhow::Result<ExitCode> {
    let old_work_item_id = WorkItemId::from(old_id.to_owned());
    let new_work_item_id = WorkItemId::from(new_id.to_owned());
    let options = RenameOptions { dry_run };

    match run_rename(
        config,
        project_root,
        &old_work_item_id,
        &new_work_item_id,
        options,
    ) {
        Ok(outcome) => Ok(render_rename_outcome(&outcome)),
        Err(error) => {
            output::error(&error.to_string());
            Ok(ExitCode::FAILURE)
        }
    }
}
