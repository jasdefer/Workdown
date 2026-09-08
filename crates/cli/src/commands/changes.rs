//! `workdown changes` — print the commit message the web app's "Commit &
//! push" button would generate for the uncommitted workdown changes.
//!
//! The same builder the dialog's preview uses
//! (`workdown_git::preview::build_preview`), so a terminal and the board
//! describe a change with the same words. The message goes to stdout
//! and nothing else does — a `prepare-commit-msg` hook can pipe it
//! straight into git's message file; the note about files outside the
//! workdown paths goes to stderr.

use std::path::Path;
use std::process::ExitCode;

use anyhow::{anyhow, Result};
use workdown_core::model::config::Config;
use workdown_git::preview::build_preview;
use workdown_git::scope::GitScope;

pub fn run_changes_command(
    config: &Config,
    project_root: &Path,
    config_path: &Path,
    list_files: bool,
) -> Result<ExitCode> {
    let schema_path = project_root.join(&config.schema);
    let schema = workdown_core::parser::schema::load_schema(&schema_path)
        .map_err(|error| anyhow!("failed to load schema: {error}"))?;

    let not_a_repository = || anyhow!("the project is not inside a git repository");
    let scope =
        GitScope::for_project(project_root, config, config_path)?.ok_or_else(not_a_repository)?;
    let preview =
        build_preview(project_root, &scope, config, &schema)?.ok_or_else(not_a_repository)?;

    if list_files && !preview.files.is_empty() {
        for file in &preview.files {
            println!("{:<10} {:<10} {}", file.label, file.role, file.path);
        }
        println!();
    }
    println!("{}", preview.message);

    if !preview.outside.is_empty() {
        let count = preview.outside.len();
        eprintln!(
            "note: {count} uncommitted {} outside the workdown paths {} not described",
            if count == 1 { "file" } else { "files" },
            if count == 1 { "is" } else { "are" },
        );
    }
    Ok(ExitCode::SUCCESS)
}
