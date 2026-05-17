//! `workdown body` — replace the freeform Markdown body of a work item.
//!
//! Thin wrapper over [`workdown_core::operations::body::run_body_replace`].
//! Bodies don't go through schema validation, so the output line is a
//! simple line-count summary rather than the field-mutation rendering in
//! [`super::mutation_output`].

use std::path::Path;
use std::process::ExitCode;

use workdown_core::model::config::Config;
use workdown_core::model::WorkItemId;
use workdown_core::operations::body::run_body_replace;

use crate::cli::output;

pub fn run_body_command(
    config: &Config,
    project_root: &Path,
    id: &str,
    new_body: &str,
) -> anyhow::Result<ExitCode> {
    let work_item_id = WorkItemId::from(id.to_owned());

    match run_body_replace(config, project_root, &work_item_id, new_body.to_owned()) {
        Ok(outcome) => {
            let line_count = count_lines(&outcome.new_body);
            let plural = if line_count == 1 { "" } else { "s" };
            output::success(&format!("{id}: body replaced ({line_count} line{plural})"));
            for warning in &outcome.warnings {
                output::warning(&warning.to_string());
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(error) => {
            output::error(&error.to_string());
            Ok(ExitCode::FAILURE)
        }
    }
}

/// Count newline-terminated lines in a normalised body string.
///
/// The core's [`run_body_replace`] guarantees a normalised body: empty,
/// or non-empty ending in exactly one `\n`. So counting `\n` characters
/// gives the right answer in both cases — `0` for an empty body, `N` for
/// `N` lines.
fn count_lines(body: &str) -> usize {
    body.matches('\n').count()
}
