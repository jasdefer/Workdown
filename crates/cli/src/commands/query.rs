//! Implementation of the `workdown query` command.

use std::path::Path;

use crate::cli::{self, QueryFormat, QueryOutput};
use workdown_core::model::config::Config;
use workdown_core::model::schema::Schema;
use workdown_core::project::load_project;
use workdown_core::query;
use workdown_core::query::format::DelimitedOptions;
use workdown_core::query::parse::parse_sort;
use workdown_core::query::types::{Predicate, QueryRequest, SortSpec};
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

    // Parse --where clauses into a single predicate (ANDed together). A
    // clause the grammar rejects or a field the schema lacks is an error,
    // exactly as it is for a view's `where:` slot.
    let predicate = parse_where_clauses(where_clauses, schema)?;

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

    // Parse --sort arguments into sort specs. A key the grammar rejects
    // or a field the schema lacks is an error, exactly as it is for a
    // view's `sort:` slot — the two share the grammar and the rules.
    let sort = parse_sort_arguments(sort_arguments, schema)?;

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

/// Parse --where clauses into a single predicate, refusing a clause the
/// grammar rejects and one naming a field that does not exist — the same
/// two rules `views_check` applies to a view's `where:`, asked of the
/// same core functions, so the flag and the slot cannot drift. Each
/// clause is checked on its own so the message can name the one at
/// fault.
fn parse_where_clauses(clauses: &[String], schema: &Schema) -> anyhow::Result<Option<Predicate>> {
    let mut predicates = Vec::new();
    for clause in clauses {
        let predicate = query::parse::parse_where(clause)
            .map_err(|error| anyhow::anyhow!("--where '{clause}': {error}"))?;
        if let Some(field_name) = where_check::unknown_fields(&predicate, schema).first() {
            anyhow::bail!("--where '{clause}': {}", unknown_field_detail(field_name));
        }
        predicates.push(predicate);
    }
    Ok(match predicates.len() {
        0 => None,
        1 => Some(predicates.remove(0)),
        _ => Some(Predicate::And(predicates)),
    })
}

/// Parse --sort arguments into sort specifications, refusing a key the
/// shared grammar rejects and a field items do not carry — the same two
/// rules `views_check` applies to a view's `sort:`.
fn parse_sort_arguments(arguments: &[String], schema: &Schema) -> anyhow::Result<Vec<SortSpec>> {
    arguments
        .iter()
        .map(|argument| {
            let spec = parse_sort(argument)
                .map_err(|error| anyhow::anyhow!("--sort '{argument}': {error}"))?;
            if !schema.is_item_field(&spec.field) {
                anyhow::bail!("--sort '{argument}': {}", unknown_field_detail(&spec.field));
            }
            Ok(spec)
        })
        .collect()
}

/// The one wording for a flag naming a field that does not exist.
fn unknown_field_detail(field_name: &str) -> String {
    format!("unknown field '{field_name}' (not defined in schema.yaml)")
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
