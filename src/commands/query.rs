//! Implementation of the `workdown query` command.

use std::path::Path;

use crate::cli::{self, QueryFormat};
use crate::model::config::Config;
use crate::parser;
use crate::query;
use crate::query::types::{Predicate, QueryRequest, SortDirection, SortSpec};
use crate::store::Store;

/// Run the query command: filter, sort, and display work items.
pub fn run_query(
    config: &Config,
    project_root: &Path,
    where_clauses: &[String],
    sort_arguments: &[String],
    fields_argument: Option<&str>,
    format: QueryFormat,
) -> anyhow::Result<()> {
    let schema_path = project_root.join(&config.schema);
    let items_path = project_root.join(&config.paths.work_items);

    let schema = parser::schema::load_schema(&schema_path)?;
    let store = Store::load(&items_path, &schema)?;

    // Parse --where clauses into a single predicate (ANDed together).
    let predicate = parse_where_clauses(where_clauses)?;

    // Parse --sort arguments into sort specs.
    let sort = parse_sort_arguments(sort_arguments);

    // Parse --fields argument into column names.
    let fields = parse_fields_argument(fields_argument);

    let request = QueryRequest {
        predicate,
        sort,
        fields,
    };

    let result = query::engine::execute(&request, &store, &schema)?;

    match format {
        QueryFormat::Table => {
            if result.items.is_empty() {
                cli::output::info("No matching items");
            } else {
                let headers: Vec<&str> = result.columns.iter().map(|column| column.as_str()).collect();
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
            println!("{}", query::format::render_json(&result));
        }
    }

    Ok(())
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
