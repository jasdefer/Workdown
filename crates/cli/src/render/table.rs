//! Table renderer — turns [`TableData`] into a GFM table.
//!
//! Output shape: a top-level `# Table` heading, then a tight GFM table
//! with one column per `data.columns` entry. The virtual `id` column
//! renders as a Markdown link to the row's own item file. `Link` and
//! `Links` cells render the same way (id-only — `TableRow` doesn't carry
//! target titles). Empty cells render blank; `|` and newlines in text
//! cells are escaped to `\|` and `<br>`. A table with zero columns
//! emits the heading only; zero rows emits header + separator and stops.

use workdown_core::model::duration::format_duration_seconds;
use workdown_core::model::FieldValue;
use workdown_core::view_data::{TableData, TableRow};

use crate::render::markdown::{emit_description, escape_cell, id_link};

/// Render a `TableData` as a Markdown string.
///
/// `item_link_base` is the relative path from the rendered view file to
/// the work items directory — see `render::board::render_board` for the
/// same parameter. `description` is the one-line caption below the
/// heading; tables typically receive an empty string since column
/// headers already convey field names.
pub fn render_table(data: &TableData, item_link_base: &str, description: &str) -> String {
    let mut out = String::new();
    out.push_str("# Table\n\n");
    emit_description(description, &mut out);

    if data.columns.is_empty() {
        return out;
    }

    out.push_str("| ");
    out.push_str(&data.columns.join(" | "));
    out.push_str(" |\n");

    out.push('|');
    for _ in &data.columns {
        out.push_str(" --- |");
    }
    out.push('\n');

    for row in &data.rows {
        render_row(row, &data.columns, item_link_base, &mut out);
    }

    out
}

fn render_row(row: &TableRow, columns: &[String], item_link_base: &str, out: &mut String) {
    out.push_str("| ");
    for (idx, cell) in row.cells.iter().enumerate() {
        if idx > 0 {
            out.push_str(" | ");
        }
        out.push_str(&format_cell(cell, &columns[idx], item_link_base));
    }
    out.push_str(" |\n");
}

fn format_cell(cell: &Option<FieldValue>, column: &str, item_link_base: &str) -> String {
    let Some(value) = cell else {
        return String::new();
    };

    // The virtual `id` column is emitted as `String` by the extractor;
    // rewrite it as a link to the row's own item file.
    if column == "id" {
        if let FieldValue::String(s) = value {
            return id_link(s, item_link_base);
        }
    }

    match value {
        FieldValue::String(s) | FieldValue::Choice(s) => escape_cell(s),
        FieldValue::Multichoice(values) | FieldValue::List(values) => values
            .iter()
            .map(|v| escape_cell(v))
            .collect::<Vec<_>>()
            .join(", "),
        FieldValue::Integer(n) => n.to_string(),
        FieldValue::Float(f) => f.to_string(),
        FieldValue::Date(d) => d.format("%Y-%m-%d").to_string(),
        FieldValue::Duration(seconds) => format_duration_seconds(*seconds),
        FieldValue::Boolean(true) => "✓".to_owned(),
        FieldValue::Boolean(false) => "✗".to_owned(),
        FieldValue::Link(id) => id_link(id.as_str(), item_link_base),
        FieldValue::Links(ids) => ids
            .iter()
            .map(|id| id_link(id.as_str(), item_link_base))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use workdown_core::model::{FieldValue, WorkItemId};
    use workdown_core::view_data::{TableData, TableRow};

    fn row(id: &str, cells: Vec<Option<FieldValue>>) -> TableRow {
        TableRow {
            id: WorkItemId::from(id.to_owned()),
            cells,
        }
    }

    fn table(columns: Vec<&str>, rows: Vec<TableRow>) -> TableData {
        TableData {
            columns: columns.into_iter().map(str::to_owned).collect(),
            rows,
        }
    }

    fn id_cell(id: &str) -> Option<FieldValue> {
        Some(FieldValue::String(id.to_owned()))
    }

    #[test]
    fn renders_top_heading() {
        let data = table(vec!["id"], vec![]);
        let output = render_table(&data, "../workdown-items", "");
        assert!(output.starts_with("# Table\n\n"));
    }

    #[test]
    fn zero_columns_renders_heading_only() {
        let data = table(vec![], vec![]);
        let output = render_table(&data, "../workdown-items", "");
        assert_eq!(output, "# Table\n\n");
    }

    #[test]
    fn zero_rows_emits_header_and_separator_only() {
        let data = table(vec!["id", "status"], vec![]);
        let output = render_table(&data, "../workdown-items", "");
        assert_eq!(output, "# Table\n\n| id | status |\n| --- | --- |\n");
    }

    #[test]
    fn id_column_renders_as_link() {
        let data = table(vec!["id"], vec![row("task-a", vec![id_cell("task-a")])]);
        let output = render_table(&data, "../workdown-items", "");
        assert!(output.contains("| [task-a](../workdown-items/task-a.md) |\n"));
    }

    #[test]
    fn missing_cell_renders_blank() {
        let data = table(
            vec!["id", "points"],
            vec![row("a", vec![id_cell("a"), None])],
        );
        let output = render_table(&data, "../workdown-items", "");
        assert!(output.contains("| [a](../workdown-items/a.md) |  |\n"));
    }

    #[test]
    fn string_cell_escapes_pipe() {
        let data = table(
            vec!["id", "title"],
            vec![row(
                "a",
                vec![id_cell("a"), Some(FieldValue::String("a | b".into()))],
            )],
        );
        let output = render_table(&data, "../workdown-items", "");
        assert!(output.contains(r"| a \| b |"));
    }

    #[test]
    fn newline_in_cell_becomes_br() {
        let data = table(
            vec!["id", "title"],
            vec![row(
                "a",
                vec![
                    id_cell("a"),
                    Some(FieldValue::String("line one\nline two".into())),
                ],
            )],
        );
        let output = render_table(&data, "../workdown-items", "");
        assert!(output.contains("| line one<br>line two |"));
    }

    #[test]
    fn crlf_in_cell_collapses_to_single_br() {
        let data = table(
            vec!["id", "title"],
            vec![row(
                "a",
                vec![id_cell("a"), Some(FieldValue::String("one\r\ntwo".into()))],
            )],
        );
        let output = render_table(&data, "../workdown-items", "");
        assert!(output.contains("| one<br>two |"));
    }

    #[test]
    fn integer_cell_renders_number() {
        let data = table(
            vec!["id", "points"],
            vec![row("a", vec![id_cell("a"), Some(FieldValue::Integer(42))])],
        );
        let output = render_table(&data, "../workdown-items", "");
        assert!(output.contains("| 42 |\n"));
    }

    #[test]
    fn float_cell_preserves_precision() {
        let data = table(
            vec!["id", "ratio"],
            vec![row(
                "a",
                vec![id_cell("a"), Some(FieldValue::Float(3.14159))],
            )],
        );
        let output = render_table(&data, "../workdown-items", "");
        assert!(output.contains("| 3.14159 |\n"));
    }

    #[test]
    fn date_cell_renders_iso() {
        let date = NaiveDate::from_ymd_opt(2026, 4, 30).unwrap();
        let data = table(
            vec!["id", "due"],
            vec![row("a", vec![id_cell("a"), Some(FieldValue::Date(date))])],
        );
        let output = render_table(&data, "../workdown-items", "");
        assert!(output.contains("| 2026-04-30 |\n"));
    }

    #[test]
    fn boolean_true_renders_check() {
        let data = table(
            vec!["id", "blocked"],
            vec![row(
                "a",
                vec![id_cell("a"), Some(FieldValue::Boolean(true))],
            )],
        );
        let output = render_table(&data, "../workdown-items", "");
        assert!(output.contains("| ✓ |\n"));
    }

    #[test]
    fn boolean_false_renders_cross() {
        let data = table(
            vec!["id", "blocked"],
            vec![row(
                "a",
                vec![id_cell("a"), Some(FieldValue::Boolean(false))],
            )],
        );
        let output = render_table(&data, "../workdown-items", "");
        assert!(output.contains("| ✗ |\n"));
    }

    #[test]
    fn choice_cell_renders_value() {
        let data = table(
            vec!["id", "status"],
            vec![row(
                "a",
                vec![id_cell("a"), Some(FieldValue::Choice("open".into()))],
            )],
        );
        let output = render_table(&data, "../workdown-items", "");
        assert!(output.contains("| open |\n"));
    }

    #[test]
    fn multichoice_renders_comma_separated() {
        let data = table(
            vec!["id", "labels"],
            vec![row(
                "a",
                vec![
                    id_cell("a"),
                    Some(FieldValue::Multichoice(vec!["red".into(), "blue".into()])),
                ],
            )],
        );
        let output = render_table(&data, "../workdown-items", "");
        assert!(output.contains("| red, blue |\n"));
    }

    #[test]
    fn list_renders_comma_separated() {
        let data = table(
            vec!["id", "tags"],
            vec![row(
                "a",
                vec![
                    id_cell("a"),
                    Some(FieldValue::List(vec!["alpha".into(), "beta".into()])),
                ],
            )],
        );
        let output = render_table(&data, "../workdown-items", "");
        assert!(output.contains("| alpha, beta |\n"));
    }

    #[test]
    fn link_cell_renders_link_to_target() {
        let data = table(
            vec!["id", "parent"],
            vec![row(
                "a",
                vec![
                    id_cell("a"),
                    Some(FieldValue::Link(WorkItemId::from("epic-x".to_owned()))),
                ],
            )],
        );
        let output = render_table(&data, "../workdown-items", "");
        assert!(output.contains("| [epic-x](../workdown-items/epic-x.md) |\n"));
    }

    #[test]
    fn links_cell_renders_comma_separated_links() {
        let data = table(
            vec!["id", "depends_on"],
            vec![row(
                "a",
                vec![
                    id_cell("a"),
                    Some(FieldValue::Links(vec![
                        WorkItemId::from("foo".to_owned()),
                        WorkItemId::from("bar".to_owned()),
                    ])),
                ],
            )],
        );
        let output = render_table(&data, "../workdown-items", "");
        assert!(output
            .contains("| [foo](../workdown-items/foo.md), [bar](../workdown-items/bar.md) |\n"));
    }

    #[test]
    fn uses_configured_item_link_base() {
        let data = table(vec!["id"], vec![row("a", vec![id_cell("a")])]);
        let output = render_table(&data, "../nested/items", "");
        assert!(output.contains("| [a](../nested/items/a.md) |\n"));
    }

    #[test]
    fn full_output_snapshot() {
        let data = table(
            vec!["id", "status", "points"],
            vec![
                row(
                    "task-a",
                    vec![
                        id_cell("task-a"),
                        Some(FieldValue::Choice("open".into())),
                        Some(FieldValue::Integer(3)),
                    ],
                ),
                row(
                    "task-b",
                    vec![
                        id_cell("task-b"),
                        Some(FieldValue::Choice("done".into())),
                        None,
                    ],
                ),
            ],
        );
        let output = render_table(&data, "../workdown-items", "");
        let expected = "# Table\n\n\
            | id | status | points |\n\
            | --- | --- | --- |\n\
            | [task-a](../workdown-items/task-a.md) | open | 3 |\n\
            | [task-b](../workdown-items/task-b.md) | done |  |\n";
        assert_eq!(output, expected);
    }
}
