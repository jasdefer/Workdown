pub mod output;
pub mod schema_args;

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
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Suppress all output except errors
    #[arg(short, long, conflicts_with = "verbose")]
    pub quiet: bool,

    /// Path to project config file
    #[arg(long, default_value = ".workdown/config.yaml", env = "WORKDOWN_CONFIG")]
    pub config: PathBuf,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Initialize a new workdown project in the current directory
    Init {
        /// Project name (defaults to current directory name)
        #[arg(long)]
        name: Option<String>,
    },
    /// Validate all work items against the schema
    Validate {
        /// Output format: human-readable or JSON
        #[arg(long, value_enum, default_value_t = ValidateFormat::Human)]
        format: ValidateFormat,
    },
    /// Create a new work item
    ///
    /// Field flags are built dynamically from the project's schema. Run
    /// `workdown add --help` inside a workdown project to see the fields
    /// available in this project.
    #[command(disable_help_flag = true)]
    Add {
        /// Field args (e.g. `--title "Foo" --type epic --tags a --tags b`).
        /// Parsed against the project schema at runtime.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Query and filter work items
    Query {
        /// Filter expression (repeatable, combined with AND).
        /// Examples: status=open, "points>3", "title~login", assignee?
        #[arg(long = "where", value_name = "EXPR")]
        where_clauses: Vec<String>,

        /// Sort by field (repeatable for multi-sort). Format: field or field:desc
        #[arg(long = "sort", value_name = "FIELD[:dir]")]
        sort: Vec<String>,

        /// Columns to display (comma-separated). Default: id + required fields.
        #[arg(long = "fields", value_name = "FIELD,...")]
        fields: Option<String>,

        /// Output format
        #[arg(long = "format", value_enum, default_value_t = QueryFormat::Table)]
        format: QueryFormat,

        /// Column delimiter for tsv/csv output. Defaults: tab for tsv, comma for csv.
        /// Must be a single ASCII character. Ignored for table/json.
        #[arg(long = "delimiter", value_name = "CHAR")]
        delimiter: Option<char>,

        /// Omit the header row in tsv/csv output. Ignored for table/json.
        #[arg(long = "no-header")]
        no_header: bool,
    },
    /// Show Kanban board view
    Board,
    /// Show parent-child tree view
    Tree,
    /// Show dependency graph
    Graph,
    /// List or show work item templates
    Templates {
        #[command(subcommand)]
        action: TemplatesAction,
    },
}

#[derive(Debug, Subcommand)]
pub enum TemplatesAction {
    /// List available templates
    List {
        /// Output format
        #[arg(long, value_enum, default_value_t = QueryFormat::Table)]
        format: QueryFormat,
    },
    /// Print a template's raw contents
    Show {
        /// Template name (without .md)
        name: String,
    },
}

/// Output format for the `validate` command.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ValidateFormat {
    /// Styled, human-readable output (default).
    Human,
    /// Machine-readable JSON output.
    Json,
}

/// Output format for the `query` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum QueryFormat {
    /// Styled table output (default).
    Table,
    /// Machine-readable JSON output.
    Json,
    /// Tab-separated values (Excel clipboard-friendly).
    Tsv,
    /// Comma-separated values (RFC 4180).
    Csv,
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
