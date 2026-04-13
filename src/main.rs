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
        cli::Command::Init => {
            tracing::info!("initializing workdown project");
            anyhow::bail!("not yet implemented — coming in Phase 3");
        }
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

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use workdown::cli::Cli;

    #[test]
    fn verify_cli() {
        Cli::command().debug_assert();
    }
}
