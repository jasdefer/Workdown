use std::path::PathBuf;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

/// Git-based project management — work items as Markdown files.
#[derive(Debug, Parser)]
#[command(name = "workdown", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Increase logging verbosity (-v info, -vv debug, -vvv trace)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Suppress all output except errors
    #[arg(short, long, global = true, conflicts_with = "verbose")]
    pub quiet: bool,

    /// Path to project config file
    #[arg(long, global = true, default_value = ".workdown/config.yaml")]
    pub config: PathBuf,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Initialize a new workdown project in the current directory
    Init,
    /// Validate all work items against the schema
    Validate,
    /// Create a new work item
    Add {
        /// Title of the work item
        title: String,
    },
    /// Query and filter work items
    Query,
    /// Show Kanban board view
    Board,
    /// Show parent-child tree view
    Tree,
    /// Show dependency graph
    Graph,
}

/// Initialize the tracing subscriber for CLI logging.
///
/// Verbosity mapping:
/// - Default (no flags): WARN only
/// - `-v`: INFO
/// - `-vv`: DEBUG
/// - `-vvv`: TRACE (includes module paths)
/// - `-q`: ERROR only
///
/// The `WORKDOWN_LOG` environment variable overrides these defaults
/// (e.g. `WORKDOWN_LOG=debug workdown validate`).
pub fn init_logging(verbosity: u8, quiet: bool) {
    use tracing::level_filters::LevelFilter;

    let default_level = if quiet {
        LevelFilter::ERROR
    } else {
        match verbosity {
            0 => LevelFilter::WARN,
            1 => LevelFilter::INFO,
            2 => LevelFilter::DEBUG,
            _ => LevelFilter::TRACE,
        }
    };

    let env_filter = EnvFilter::builder()
        .with_default_directive(default_level.into())
        .with_env_var("WORKDOWN_LOG")
        .from_env_lossy();

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(verbosity >= 3)
        .without_time()
        .init();
}
