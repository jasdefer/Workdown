use std::process::ExitCode;

use clap::Parser;

use workdown::cli;

fn main() -> ExitCode {
    let cli = cli::Cli::parse();

    cli::init_logging(cli.verbose, cli.quiet);

    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!("{err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: &cli::Cli) -> anyhow::Result<()> {
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
            Ok(())
        }

        // All other commands need the project config.
        cmd => {
            let config = workdown::parser::config::load_config(&cli.config)
                .map_err(|e| anyhow::anyhow!("failed to load config: {e}"))?;
            tracing::debug!(project = %config.project.name, "loaded config");

            match cmd {
                cli::Command::Init { .. } => unreachable!(),
                cli::Command::Validate => {
                    tracing::info!("validating work items");
                    anyhow::bail!("not yet implemented — coming in Phase 3");
                }
                cli::Command::Add { title } => {
                    tracing::info!(title, "creating work item");
                    anyhow::bail!("not yet implemented — coming in Phase 3");
                }
                cli::Command::Query => {
                    tracing::info!("querying work items");
                    anyhow::bail!("not yet implemented — coming in Phase 3");
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
            }
        }
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
