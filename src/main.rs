use std::process::ExitCode;

use clap::Parser;

use workdown::cli;

fn main() -> ExitCode {
    let cli = cli::Cli::parse();

    cli::init_logging(cli.verbose, cli.quiet);

    match run(&cli) {
        Ok(code) => code,
        Err(err) => {
            tracing::error!("{err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: &cli::Cli) -> anyhow::Result<ExitCode> {
    tracing::debug!("workdown v{}", env!("CARGO_PKG_VERSION"));
    tracing::debug!(config = %cli.config.display(), "using config");

    match &cli.command {
        cli::Command::Init { name } => {
            tracing::info!("initializing workdown project");
            let root = std::env::current_dir()
                .map_err(|e| anyhow::anyhow!("cannot determine current directory: {e}"))?;
            match workdown::commands::init::run_init(&root, name.as_deref())? {
                workdown::commands::init::InitOutcome::Created => {
                    cli::output::success("Initialized workdown project");
                }
                workdown::commands::init::InitOutcome::AlreadyExists => {
                    cli::output::warning("Already initialized (.workdown/ exists, skipping)");
                }
            }
            Ok(ExitCode::SUCCESS)
        }

        // All other commands need the project config.
        cmd => {
            let config = workdown::parser::config::load_config(&cli.config)
                .map_err(|e| anyhow::anyhow!("failed to load config: {e}"))?;
            tracing::debug!(project = %config.project.name, "loaded config");

            match cmd {
                cli::Command::Init { .. } => unreachable!(),
                cli::Command::Validate { format } => {
                    tracing::info!("validating work items");
                    let project_root = std::env::current_dir()
                        .map_err(|e| anyhow::anyhow!("cannot determine current directory: {e}"))?;
                    let has_errors = workdown::commands::validate::run_validate(
                        &config,
                        &project_root,
                        *format,
                    )?;
                    if has_errors {
                        Ok(ExitCode::FAILURE)
                    } else {
                        Ok(ExitCode::SUCCESS)
                    }
                }
                cli::Command::Add { args } => {
                    tracing::info!("creating work item");
                    let project_root = std::env::current_dir()
                        .map_err(|e| anyhow::anyhow!("cannot determine current directory: {e}"))?;
                    run_add_command(&config, &project_root, args)
                }
                cli::Command::Query {
                    where_clauses,
                    sort,
                    fields,
                    format,
                    delimiter,
                    no_header,
                } => {
                    tracing::info!("querying work items");
                    let project_root = std::env::current_dir()
                        .map_err(|e| anyhow::anyhow!("cannot determine current directory: {e}"))?;
                    let output = cli::QueryOutput {
                        format: *format,
                        delimiter: *delimiter,
                        no_header: *no_header,
                    };
                    workdown::commands::query::run_query(
                        &config,
                        &project_root,
                        where_clauses,
                        sort,
                        fields.as_deref(),
                        output,
                    )?;
                    Ok(ExitCode::SUCCESS)
                }
                cli::Command::Board => {
                    tracing::info!("rendering board view");
                    anyhow::bail!("not yet implemented — coming in Phase 4");
                }
                cli::Command::Tree => {
                    tracing::info!("rendering tree view");
                    anyhow::bail!("not yet implemented — coming in Phase 4");
                }
                cli::Command::Graph => {
                    tracing::info!("rendering dependency graph");
                    anyhow::bail!("not yet implemented — coming in Phase 4");
                }
                cli::Command::Templates { action } => {
                    let project_root = std::env::current_dir()
                        .map_err(|e| anyhow::anyhow!("cannot determine current directory: {e}"))?;
                    match action {
                        cli::TemplatesAction::List { format } => {
                            tracing::info!("listing templates");
                            workdown::commands::templates::run_templates_list(
                                &config,
                                &project_root,
                                *format,
                            )?;
                            Ok(ExitCode::SUCCESS)
                        }
                        cli::TemplatesAction::Show { name } => {
                            tracing::info!("showing template");
                            match workdown::commands::templates::run_templates_show(
                                &config,
                                &project_root,
                                name,
                            ) {
                                Ok(()) => Ok(ExitCode::SUCCESS),
                                Err(err) => {
                                    cli::output::error(&err.to_string());
                                    Ok(ExitCode::FAILURE)
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Run `workdown add` with the raw args captured by the top-level clap parse.
///
/// Two-phase parsing: load the schema, build a dynamic `clap::Command`
/// with one flag per schema field, parse the raw args against it, then
/// invoke the add command with the resulting field map.
fn run_add_command(
    config: &workdown::model::config::Config,
    project_root: &std::path::Path,
    raw_args: &[String],
) -> anyhow::Result<ExitCode> {
    let schema_path = project_root.join(&config.schema);
    let schema = workdown::parser::schema::load_schema(&schema_path)
        .map_err(|e| anyhow::anyhow!("failed to load schema: {e}"))?;

    let command = workdown::cli::schema_args::build_add_command(&schema);

    let matches = match command.try_get_matches_from(raw_args.iter().cloned()) {
        Ok(matches) => matches,
        Err(error) => {
            // `--help` / `--version` paths: print and exit successfully.
            match error.kind() {
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion => {
                    error.print()?;
                    return Ok(ExitCode::SUCCESS);
                }
                _ => {
                    error.print()?;
                    return Ok(ExitCode::FAILURE);
                }
            }
        }
    };

    let field_values = workdown::cli::schema_args::matches_to_field_map(&matches, &schema);

    // Only treat --template as a template name when the schema does not
    // define a `template` field. When the schema wins the collision,
    // template support is unavailable for that project.
    let template_name = if schema.fields.contains_key("template") {
        None
    } else {
        matches.get_one::<String>("template").map(String::as_str)
    };

    match workdown::commands::add::run_add(config, project_root, field_values, template_name) {
        Ok(outcome) => {
            cli::output::success(&format!("Created {}", outcome.path.display()));
            for warning in &outcome.warnings {
                cli::output::warning(&warning.to_string());
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(workdown::commands::add::AddError::ValidationFailed { diagnostics }) => {
            for diagnostic in &diagnostics {
                cli::output::error(&diagnostic.to_string());
            }
            Ok(ExitCode::FAILURE)
        }
        Err(error @ workdown::commands::add::AddError::Template(_)) => {
            cli::output::error(&error.to_string());
            Ok(ExitCode::FAILURE)
        }
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use workdown::cli::Cli;

    #[test]
    fn verify_cli() {
        Cli::command().debug_assert();
    }
}
