mod cli;
mod commands;
mod render;

use std::path::Path;
use std::process::ExitCode;

use clap::Parser;
use workdown_core::model::config::Config;

fn main() -> ExitCode {
    let cli = cli::Cli::parse();

    cli::init_logging(cli.verbose, cli.quiet);

    match run(&cli) {
        Ok(code) => code,
        // Every failure the user sees leaves through one channel and
        // wears one style. Routing a startup failure to `tracing`
        // instead would both look different from an operation failure
        // and be subject to the log filter, which can drop it entirely.
        Err(err) => {
            cli::output::error(&format!("{err:#}"));
            ExitCode::FAILURE
        }
    }
}

fn run(cli: &cli::Cli) -> anyhow::Result<ExitCode> {
    tracing::debug!("workdown v{}", env!("CARGO_PKG_VERSION"));
    tracing::debug!(config = %cli.config.display(), "using config");

    // Every command works relative to the directory it was invoked
    // from, so this is read once here rather than per command.
    let project_root = std::env::current_dir()
        .map_err(|e| anyhow::anyhow!("cannot determine current directory: {e}"))?;
    let root = project_root.as_path();

    // Each arm names its own needs: `init` runs without a project
    // config — creating one is its job — and every other command opens
    // by loading it.
    match &cli.command {
        cli::Command::Init {
            name,
            install_hooks,
        } => run_init(cli, root, name.as_deref(), *install_hooks),

        cli::Command::Validate { format, as_of } => {
            let config = load_project_config(cli)?;
            tracing::info!("validating work items");
            let result =
                workdown_core::operations::validate::validate(&config, root, &cli.config, *as_of)
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
            commands::validate::render(&result.diagnostics, *format);
            Ok(exit_code(!result.has_errors))
        }

        cli::Command::Add { args } => {
            let config = load_project_config(cli)?;
            tracing::info!("creating work item");
            run_add_command(&config, root, args)
        }

        cli::Command::Query {
            where_clauses,
            sort,
            fields,
            format,
            delimiter,
            no_header,
            as_of,
        } => {
            let config = load_project_config(cli)?;
            tracing::info!("querying work items");
            let output = cli::QueryOutput {
                format: *format,
                delimiter: *delimiter,
                no_header: *no_header,
            };
            commands::query::run_query(
                &config,
                root,
                &cli.config,
                where_clauses,
                sort,
                fields.as_deref(),
                output,
                *as_of,
            )?;
            Ok(ExitCode::SUCCESS)
        }

        cli::Command::Render { view_id, as_of } => {
            let config = load_project_config(cli)?;
            tracing::info!("rendering views");
            commands::render::run_render(&config, root, &cli.config, view_id.as_deref(), *as_of)
        }

        cli::Command::Templates { action } => {
            let config = load_project_config(cli)?;
            run_templates_command(&config, root, action)
        }

        cli::Command::Set {
            id,
            field,
            value,
            append,
            remove,
            delta,
            toggle,
        } => {
            let config = load_project_config(cli)?;
            tracing::info!("mutating field on work item");
            let mode = set_mode(value, append, remove, delta, *toggle);
            commands::set::run_set_command(&config, root, id, field, mode)
        }

        cli::Command::Unset { id, field } => {
            let config = load_project_config(cli)?;
            tracing::info!("clearing field on work item");
            commands::unset::run_unset_command(&config, root, id, field)
        }

        cli::Command::Move { id, value } => {
            let config = load_project_config(cli)?;
            tracing::info!("moving work item on board field");
            commands::r#move::run_move_command(&config, root, id, value)
        }

        cli::Command::Body { id, body } => {
            let config = load_project_config(cli)?;
            tracing::info!("replacing body of work item");
            commands::body::run_body_command(&config, root, id, body)
        }

        cli::Command::InstallHooks { check } => {
            let config = load_project_config(cli)?;
            tracing::info!("installing pre-commit hook");
            commands::install_hooks::run_install_hooks_command(&config, root, &cli.config, *check)
        }

        cli::Command::Serve { port, open, as_of } => {
            let config = load_project_config(cli)?;
            tracing::info!("starting workdown serve");
            commands::serve::run_serve_command(&config, root, &cli.config, *port, *open, *as_of)
        }

        cli::Command::Rename {
            old_id,
            new_id,
            dry_run,
        } => {
            let config = load_project_config(cli)?;
            tracing::info!("renaming work item");
            commands::rename::run_rename_command(&config, root, old_id, new_id, *dry_run)
        }
    }
}

/// Load the project config every command but `init` opens with.
fn load_project_config(cli: &cli::Cli) -> anyhow::Result<Config> {
    let config = workdown_core::parser::config::load_config(&cli.config)
        .map_err(|e| anyhow::anyhow!("failed to load config: {e}"))?;
    tracing::debug!(project = %config.project.name, "loaded config");
    Ok(config)
}

/// The `0` / `1` axis of the exit-code contract: did the work succeed.
///
/// A malformed invocation is `2` and never comes through here — clap
/// returns it directly for every command but `add`, which parses its
/// schema-derived flags itself. See `docs/architecture.md`.
fn exit_code(ok: bool) -> ExitCode {
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Scaffold a project, then optionally install the git hooks into it.
///
/// The one command that runs without a project config, because it is
/// what produces one. `--install-hooks` needs a config to template the
/// hook's paths from, so it loads the scaffold this run just wrote (or
/// the config a pre-existing project already had).
fn run_init(
    cli: &cli::Cli,
    project_root: &Path,
    name: Option<&str>,
    install_hooks: bool,
) -> anyhow::Result<ExitCode> {
    use workdown_core::operations::init::{run_init as scaffold, InitOutcome};

    tracing::info!("initializing workdown project");
    match scaffold(project_root, name)? {
        InitOutcome::Created => cli::output::success("Initialized workdown project"),
        InitOutcome::AlreadyExists => {
            cli::output::warning("Already initialized (.workdown/ exists, skipping)")
        }
    }

    if !install_hooks {
        return Ok(ExitCode::SUCCESS);
    }
    let config = load_project_config(cli)?;
    commands::install_hooks::run_install_hooks_command(&config, project_root, &cli.config, false)
}

/// Dispatch the `templates` subcommands.
fn run_templates_command(
    config: &Config,
    project_root: &Path,
    action: &cli::TemplatesAction,
) -> anyhow::Result<ExitCode> {
    match action {
        cli::TemplatesAction::List { format } => {
            tracing::info!("listing templates");
            commands::templates::run_templates_list(config, project_root, *format)?;
            Ok(ExitCode::SUCCESS)
        }
        cli::TemplatesAction::Show { name } => {
            tracing::info!("showing template");
            commands::templates::run_templates_show(config, project_root, name)?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// Pick the mutation mode `set` was invoked with.
///
/// Clap's `ArgGroup` guarantees exactly one of the five is present; the
/// order below matches that contract.
fn set_mode(
    value: &Option<String>,
    append: &Option<String>,
    remove: &Option<String>,
    delta: &Option<String>,
    toggle: bool,
) -> commands::set::CliSetMode {
    use commands::set::CliSetMode;

    if let Some(value) = value {
        CliSetMode::Replace(value.clone())
    } else if let Some(value) = append {
        CliSetMode::Append(value.clone())
    } else if let Some(value) = remove {
        CliSetMode::Remove(value.clone())
    } else if let Some(value) = delta {
        CliSetMode::Delta(value.clone())
    } else if toggle {
        CliSetMode::Toggle
    } else {
        unreachable!("clap ArgGroup ensures one of value/append/remove/delta/toggle is set");
    }
}

/// Run `workdown add` with the raw args captured by the top-level clap parse.
///
/// Two-phase parsing: load the schema, build a dynamic `clap::Command`
/// with one flag per schema field, parse the raw args against it, then
/// invoke the add command with the resulting field map.
fn run_add_command(
    config: &Config,
    project_root: &Path,
    raw_args: &[String],
) -> anyhow::Result<ExitCode> {
    let schema_path = project_root.join(&config.schema);
    let schema = workdown_core::parser::schema::load_schema(&schema_path)
        .map_err(|e| anyhow::anyhow!("failed to load schema: {e}"))?;

    let command = cli::schema_args::build_add_command(&schema);

    let matches = match command.try_get_matches_from(raw_args.iter().cloned()) {
        Ok(matches) => matches,
        Err(error) => {
            // The one command clap does not exit for us: its flags come
            // from the schema, so the parse happens here and the exit
            // code is ours to return. `--help` / `--version` are a
            // successful invocation; anything else is a malformed one,
            // and gets the same `2` clap returns for every other command.
            error.print()?;
            return Ok(match error.kind() {
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion => {
                    ExitCode::SUCCESS
                }
                _ => ExitCode::from(2),
            });
        }
    };

    let field_values = cli::schema_args::matches_to_field_map(&matches, &schema);

    // Only treat --template as a template name when the schema does not
    // define a `template` field. When the schema wins the collision,
    // template support is unavailable for that project.
    let template_name = if schema.fields.contains_key("template") {
        None
    } else {
        matches.get_one::<String>("template").map(String::as_str)
    };

    match workdown_core::operations::add::run_add(config, project_root, field_values, template_name)
    {
        Ok(outcome) => {
            cli::output::success(&format!("Created {}", outcome.path.display()));
            for warning in &outcome.warnings {
                cli::output::warning(&warning.to_string());
            }
            Ok(exit_code(!outcome.mutation_caused_warning))
        }
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use crate::cli::Cli;

    #[test]
    fn verify_cli() {
        Cli::command().debug_assert();
    }
}
