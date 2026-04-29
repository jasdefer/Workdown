//! Formatting helpers for query results.
//!
//! Provides value formatting and JSON / delimited (CSV, TSV) output.
//! Table rendering is handled by the command layer using
//! `cli::output::table()` to keep this module free of CLI dependencies.

use crate::model::duration::format_duration_seconds;
use crate::model::{FieldValue, WorkItem};
use crate::query::types::QueryResult;

// ── Field value formatting ──────────────────────────────────────────

/// Format a field value as a human-readable display string.
pub fn format_field_value(value: &FieldValue) -> String {
    match value {
        FieldValue::String(string) => string.clone(),
        FieldValue::Choice(string) => string.clone(),
        FieldValue::Date(date) => date.format("%Y-%m-%d").to_string(),
        FieldValue::Duration(seconds) => format_duration_seconds(*seconds),
        FieldValue::Link(id) => id.as_str().to_owned(),
        FieldValue::Integer(number) => number.to_string(),
        FieldValue::Float(number) => number.to_string(),
        FieldValue::Boolean(flag) => flag.to_string(),
        FieldValue::Multichoice(values) => values.join(", "),
        FieldValue::List(values) => values.join(", "),
        FieldValue::Links(ids) => ids
            .iter()
            .map(|id| id.as_str())
            .collect::<Vec<_>>()
            .join(", "),
    }
}

// ── JSON output ─────────────────────────────────────────────────────

/// Render a query result as a JSON string.
///
/// Produces a JSON array of objects, one per matched item. Each object
/// has a key for every column in the result.
pub fn render_json(result: &QueryResult) -> String {
    let items: Vec<serde_json::Value> = result
        .items
        .iter()
        .map(|row| {
            let mut object = serde_json::Map::new();
            for (index, column) in result.columns.iter().enumerate() {
                let value = row.values.get(index).cloned().unwrap_or_default();
                object.insert(column.clone(), serde_json::Value::String(value));
            }
            serde_json::Value::Object(object)
        })
        .collect();

    serde_json::to_string_pretty(&items).unwrap_or_else(|_| "[]".to_owned())
}

// ── Delimited output (CSV/TSV) ──────────────────────────────────────

/// Options controlling delimited output.
#[derive(Debug, Clone, Copy)]
pub struct DelimitedOptions {
    /// Column delimiter as a raw byte (e.g. `b','` for CSV, `b'\t'` for TSV).
    pub delimiter: u8,
    /// Emit a header row with column names.
    pub header: bool,
    /// In-cell separator for list / multichoice / links values.
    pub list_separator: char,
}

/// Errors produced while rendering delimited output.
#[derive(Debug)]
pub enum DelimitedError {
    /// A list/multichoice/links element contains the configured list separator,
    /// which would make the cell ambiguous.
    EmbeddedSeparator {
        item_id: String,
        field: String,
        separator: char,
    },
    /// The column delimiter equals the list separator — the cell would be
    /// indistinguishable from two columns.
    DelimiterConflict {
        delimiter: char,
        list_separator: char,
    },
    /// Low-level write failure from the `csv` writer.
    Io(std::io::Error),
}

impl std::fmt::Display for DelimitedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmbeddedSeparator {
                item_id,
                field,
                separator,
            } => write!(
                f,
                "item '{item_id}' field '{field}' contains list separator '{separator}' in a value; pick a different --delimiter or remove the character",
            ),
            Self::DelimiterConflict {
                delimiter,
                list_separator,
            } => write!(
                f,
                "column delimiter '{delimiter}' matches the list-cell separator '{list_separator}'; pick a different --delimiter",
            ),
            Self::Io(error) => write!(f, "failed writing delimited output: {error}"),
        }
    }
}

impl std::error::Error for DelimitedError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for DelimitedError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Render matched items as CSV/TSV using `options.delimiter` between columns
/// and `options.list_separator` inside list/multichoice/links cells.
///
/// Uses the `csv` crate for correct quoting and escaping. Produces LF line
/// terminators for Unix-friendly pipelines; Excel and pandas both accept this.
pub fn render_delimited(
    items: &[&WorkItem],
    columns: &[String],
    options: &DelimitedOptions,
) -> Result<String, DelimitedError> {
    // Guard against a column delimiter colliding with the list-cell
    // separator: the `csv` crate would quote such cells, but the output
    // would still be misleading to a human. Fail loudly instead.
    if options.list_separator.is_ascii() && options.list_separator as u32 as u8 == options.delimiter
    {
        return Err(DelimitedError::DelimiterConflict {
            delimiter: options.delimiter as char,
            list_separator: options.list_separator,
        });
    }

    let mut writer = csv::WriterBuilder::new()
        .delimiter(options.delimiter)
        .terminator(csv::Terminator::Any(b'\n'))
        .from_writer(Vec::<u8>::new());

    if options.header {
        writer.write_record(columns).map_err(csv_error_to_io)?;
    }

    for item in items {
        let mut row: Vec<String> = Vec::with_capacity(columns.len());
        for column in columns {
            let cell = if column == "id" {
                item.id.as_str().to_owned()
            } else {
                match item.fields.get(column) {
                    Some(value) => format_value_delimited(
                        value,
                        options.list_separator,
                        item.id.as_str(),
                        column,
                    )?,
                    None => String::new(),
                }
            };
            row.push(cell);
        }
        writer.write_record(&row).map_err(csv_error_to_io)?;
    }

    let buffer = writer
        .into_inner()
        .map_err(|error| DelimitedError::Io(error.into_error()))?;
    Ok(String::from_utf8(buffer).expect("csv writer emits valid UTF-8"))
}

/// Format a field value for delimited output, joining multi-valued fields
/// with `list_separator` and erroring if any element itself contains it.
fn format_value_delimited(
    value: &FieldValue,
    list_separator: char,
    item_id: &str,
    field: &str,
) -> Result<String, DelimitedError> {
    match value {
        FieldValue::String(string) => Ok(string.clone()),
        FieldValue::Choice(string) => Ok(string.clone()),
        FieldValue::Date(date) => Ok(date.format("%Y-%m-%d").to_string()),
        FieldValue::Duration(seconds) => Ok(format_duration_seconds(*seconds)),
        FieldValue::Link(id) => Ok(id.as_str().to_owned()),
        FieldValue::Integer(number) => Ok(number.to_string()),
        FieldValue::Float(number) => Ok(number.to_string()),
        FieldValue::Boolean(flag) => Ok(flag.to_string()),
        FieldValue::Multichoice(values) | FieldValue::List(values) => join_with_separator(
            values.iter().map(String::as_str),
            list_separator,
            item_id,
            field,
        ),
        FieldValue::Links(ids) => join_with_separator(
            ids.iter().map(|id| id.as_str()),
            list_separator,
            item_id,
            field,
        ),
    }
}

fn join_with_separator<'a, I: Iterator<Item = &'a str>>(
    iter: I,
    separator: char,
    item_id: &str,
    field: &str,
) -> Result<String, DelimitedError> {
    let mut out = String::new();
    let mut first = true;
    for element in iter {
        if element.contains(separator) {
            return Err(DelimitedError::EmbeddedSeparator {
                item_id: item_id.to_owned(),
                field: field.to_owned(),
                separator,
            });
        }
        if !first {
            out.push(separator);
        }
        out.push_str(element);
        first = false;
    }
    Ok(out)
}

fn csv_error_to_io(error: csv::Error) -> DelimitedError {
    match error.into_kind() {
        csv::ErrorKind::Io(io) => DelimitedError::Io(io),
        other => DelimitedError::Io(std::io::Error::other(format!("{other:?}"))),
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::WorkItemId;
    use crate::query::types::{QueryResult, QueryRow};

    #[test]
    fn format_string_value() {
        assert_eq!(
            format_field_value(&FieldValue::String("hello".into())),
            "hello"
        );
    }

    #[test]
    fn format_integer_value() {
        assert_eq!(format_field_value(&FieldValue::Integer(42)), "42");
    }

    #[test]
    fn format_list_value() {
        assert_eq!(
            format_field_value(&FieldValue::List(vec!["a".into(), "b".into()])),
            "a, b"
        );
    }

    #[test]
    fn format_links_value() {
        assert_eq!(
            format_field_value(&FieldValue::Links(vec![
                WorkItemId::from("x".to_owned()),
                WorkItemId::from("y".to_owned()),
            ])),
            "x, y"
        );
    }

    #[test]
    fn render_json_produces_valid_json() {
        let result = QueryResult {
            columns: vec!["id".into(), "title".into()],
            items: vec![
                QueryRow {
                    id: "task-a".into(),
                    values: vec!["task-a".into(), "Fix Login".into()],
                },
                QueryRow {
                    id: "task-b".into(),
                    values: vec!["task-b".into(), "Add Dashboard".into()],
                },
            ],
        };

        let json = render_json(&result);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed.as_array().unwrap().len(), 2);
        assert_eq!(parsed[0]["id"], "task-a");
        assert_eq!(parsed[0]["title"], "Fix Login");
    }

    #[test]
    fn render_json_empty_result() {
        let result = QueryResult {
            columns: vec!["id".into()],
            items: vec![],
        };
        let json = render_json(&result);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 0);
    }

    // ── Delimited output tests ──────────────────────────────────────

    use std::collections::HashMap;
    use std::path::PathBuf;

    fn make_item(id: &str, fields: Vec<(&str, FieldValue)>) -> WorkItem {
        let mut map = HashMap::new();
        for (name, value) in fields {
            map.insert(name.to_owned(), value);
        }
        WorkItem {
            id: WorkItemId::from(id.to_owned()),
            fields: map,
            body: String::new(),
            source_path: PathBuf::from(format!("{id}.md")),
        }
    }

    fn tsv_options() -> DelimitedOptions {
        DelimitedOptions {
            delimiter: b'\t',
            header: true,
            list_separator: ';',
        }
    }

    #[test]
    fn delimited_renders_header_and_basic_row() {
        let item = make_item(
            "task-a",
            vec![
                ("title", FieldValue::String("Hello".into())),
                ("points", FieldValue::Integer(3)),
            ],
        );
        let columns = vec!["id".to_owned(), "title".to_owned(), "points".to_owned()];
        let output = render_delimited(&[&item], &columns, &tsv_options()).unwrap();
        assert_eq!(output, "id\ttitle\tpoints\ntask-a\tHello\t3\n");
    }

    #[test]
    fn delimited_omits_header_when_disabled() {
        let item = make_item("task-a", vec![("title", FieldValue::String("Hi".into()))]);
        let columns = vec!["id".to_owned(), "title".to_owned()];
        let options = DelimitedOptions {
            header: false,
            ..tsv_options()
        };
        let output = render_delimited(&[&item], &columns, &options).unwrap();
        assert_eq!(output, "task-a\tHi\n");
    }

    #[test]
    fn delimited_joins_lists_with_list_separator() {
        let item = make_item(
            "task-a",
            vec![(
                "tags",
                FieldValue::List(vec!["auth".into(), "backend".into()]),
            )],
        );
        let columns = vec!["id".to_owned(), "tags".to_owned()];
        let output = render_delimited(&[&item], &columns, &tsv_options()).unwrap();
        assert_eq!(output, "id\ttags\ntask-a\tauth;backend\n");
    }

    #[test]
    fn delimited_missing_field_is_empty_cell() {
        let item = make_item("task-a", vec![]);
        let columns = vec!["id".to_owned(), "title".to_owned()];
        let output = render_delimited(&[&item], &columns, &tsv_options()).unwrap();
        assert_eq!(output, "id\ttitle\ntask-a\t\n");
    }

    #[test]
    fn delimited_csv_quotes_cells_containing_delimiter() {
        // CSV mode: title "Hello, world" must be quoted so the comma is not
        // read as a column break.
        let item = make_item(
            "task-a",
            vec![("title", FieldValue::String("Hello, world".into()))],
        );
        let columns = vec!["id".to_owned(), "title".to_owned()];
        let options = DelimitedOptions {
            delimiter: b',',
            header: true,
            list_separator: ';',
        };
        let output = render_delimited(&[&item], &columns, &options).unwrap();
        assert_eq!(output, "id,title\ntask-a,\"Hello, world\"\n");
    }

    #[test]
    fn delimited_errors_on_embedded_separator_in_list_element() {
        let item = make_item(
            "task-a",
            vec![(
                "tags",
                FieldValue::List(vec!["auth;sensitive".into(), "backend".into()]),
            )],
        );
        let columns = vec!["id".to_owned(), "tags".to_owned()];
        let result = render_delimited(&[&item], &columns, &tsv_options());
        match result {
            Err(DelimitedError::EmbeddedSeparator {
                item_id,
                field,
                separator,
            }) => {
                assert_eq!(item_id, "task-a");
                assert_eq!(field, "tags");
                assert_eq!(separator, ';');
            }
            other => panic!("expected EmbeddedSeparator, got {other:?}"),
        }
    }

    #[test]
    fn delimited_errors_on_delimiter_list_separator_collision() {
        let item = make_item("task-a", vec![]);
        let columns = vec!["id".to_owned()];
        let options = DelimitedOptions {
            delimiter: b';',
            header: true,
            list_separator: ';',
        };
        let result = render_delimited(&[&item], &columns, &options);
        assert!(matches!(
            result,
            Err(DelimitedError::DelimiterConflict { .. })
        ));
    }
}
