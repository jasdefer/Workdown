//! Implementation of the `workdown query` command.

use std::path::Path;

use crate::cli::{self, QueryFormat, QueryOutput};
use workdown_core::model::config::Config;
use workdown_core::project::load_project;
use workdown_core::query;
use workdown_core::query::format::DelimitedOptions;
use workdown_core::query::types::{Predicate, QueryRequest, SortDirection, SortSpec};
use workdown_core::where_check;

/// In-cell separator for list/multichoice/links values in delimited output.
const LIST_SEPARATOR: char = ';';

/// Run the query command: filter, sort, and display work items.
/// `as_of` pins what `$today` resolves to in computed fields; `None`
/// means the current local date.
#[allow(clippy::too_many_arguments)]
pub fn run_query(
    config: &Config,
    project_root: &Path,
    config_path: &Path,
    where_clauses: &[String],
    sort_arguments: &[String],
    fields_argument: Option<&str>,
    output: QueryOutput,
    as_of: Option<chrono::NaiveDate>,
) -> anyhow::Result<()> {
    let project = load_project(config, project_root, config_path, as_of)
        .map_err(|error| anyhow::anyhow!("{error}"))?;

    // Query results run over whatever loaded — say so when parts of the
    // project are broken instead of returning silently distorted rows.
    // Warnings go to stderr, so piped table/JSON/CSV output stays clean.
    cli::output::surface_diagnostics(&project.diagnostics);

    let store = &project.store;
    let schema = &project.schema;

    // Parse --where clauses into a single predicate (ANDed together).
    let predicate = parse_where_clauses(where_clauses)?;

    // An operand that can never match makes an ad-hoc query look like a
    // project with no matching items. Say which clause is responsible —
    // on stderr, so piped table/JSON/CSV output stays clean.
    if let Some(predicate) = predicate.as_ref() {
        for violation in where_check::check_predicate(predicate, schema, &project.resources, store)
        {
            cli::output::warning(&format!(
                "filter on '{}': {}",
                violation.field,
                violation.detail()
            ));
        }
    }

    // Parse --sort arguments into sort specs.
    let sort = parse_sort_arguments(sort_arguments);

    // Parse --fields argument into column names.
    let fields = parse_fields_argument(fields_argument);

    let request = QueryRequest {
        predicate,
        sort,
        fields,
    };

    match output.format {
        QueryFormat::Table => {
            let result = query::engine::execute(&request, store, schema)?;
            if result.items.is_empty() {
                cli::output::info("No matching items");
            } else {
                let headers: Vec<&str> = result
                    .columns
                    .iter()
                    .map(|column| column.as_str())
                    .collect();
                let mut table = cli::output::table(&headers);
                for row in &result.items {
                    let cells: Vec<&str> = row.values.iter().map(|value| value.as_str()).collect();
                    table.add_row(cells);
                }
                println!("{table}");
                cli::output::info(&format!("{} item(s)", result.items.len()));
            }
        }
        QueryFormat::Json => {
            let result = query::engine::execute(&request, store, schema)?;
            println!("{}", query::format::render_json(&result));
        }
        QueryFormat::Tsv | QueryFormat::Csv => {
            let options = build_delimited_options(output)?;
            let (columns, items) = query::engine::filter_and_sort(&request, store, schema)?;
            let rendered = query::format::render_delimited(&items, &columns, &options)?;
            print!("{rendered}");
        }
    }

    Ok(())
}

/// Build [`DelimitedOptions`] for CSV/TSV rendering, honouring `--delimiter`
/// and `--no-header` overrides.
fn build_delimited_options(output: QueryOutput) -> anyhow::Result<DelimitedOptions> {
    let default_delimiter: u8 = match output.format {
        QueryFormat::Tsv => b'\t',
        QueryFormat::Csv => b',',
        _ => unreachable!("build_delimited_options called for non-delimited format"),
    };

    let resolved_delimiter = match output.delimiter {
        Some(character) => {
            if !character.is_ascii() {
                anyhow::bail!("--delimiter must be a single ASCII character (got '{character}')");
            }
            character as u8
        }
        None => default_delimiter,
    };

    Ok(DelimitedOptions {
        delimiter: resolved_delimiter,
        header: !output.no_header,
        list_separator: LIST_SEPARATOR,
    })
}

/// Parse --where clauses into a single predicate.
fn parse_where_clauses(clauses: &[String]) -> anyhow::Result<Option<Predicate>> {
    let mut predicates = Vec::new();
    for clause in clauses {
        predicates.push(query::parse::parse_where(clause)?);
    }
    Ok(match predicates.len() {
        0 => None,
        1 => Some(predicates.remove(0)),
        _ => Some(Predicate::And(predicates)),
    })
}

/// Parse --sort arguments into sort specifications.
fn parse_sort_arguments(arguments: &[String]) -> Vec<SortSpec> {
    arguments
        .iter()
        .map(|argument| {
            if let Some((field, direction_string)) = argument.split_once(':') {
                let direction = match direction_string {
                    "desc" => SortDirection::Descending,
                    _ => SortDirection::Ascending,
                };
                SortSpec {
                    field: field.to_owned(),
                    direction,
                }
            } else {
                SortSpec {
                    field: argument.to_owned(),
                    direction: SortDirection::Ascending,
                }
            }
        })
        .collect()
}

/// Parse --fields argument into column names.
fn parse_fields_argument(argument: Option<&str>) -> Vec<String> {
    argument
        .map(|value| {
            value
                .split(',')
                .map(|field| field.trim().to_owned())
                .collect()
        })
        .unwrap_or_default()
}
