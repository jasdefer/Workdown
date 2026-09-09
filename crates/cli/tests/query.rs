//! `workdown query`: the command with the most flags, every one of which
//! changes the rows or the shape of the output.

mod common;

use common::Project;

#[test]
fn table_output_lists_every_item() {
    Project::new()
        .run(&["query"])
        .assert_code(0)
        .assert_stdout_contains("task-1")
        .assert_stdout_contains("task-2")
        .assert_stderr_contains("2 item(s)");
}

#[test]
fn where_flag_filters() {
    let output = Project::new().run(&["query", "--where", "status=done", "--format", "json"]);
    output.assert_code(0);
    let rows = output.stdout_json();
    let rows = rows.as_array().expect("a JSON array");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], "task-2");
}

#[test]
fn repeated_where_flags_are_anded() {
    let output = Project::new().run(&[
        "query",
        "--where",
        "status=done",
        "--where",
        "title~1",
        "--format",
        "json",
    ]);
    output.assert_code(0);
    assert_eq!(output.stdout_json(), serde_json::json!([]));
}

#[test]
fn sort_flag_orders_and_desc_reverses() {
    let ascending = Project::new().run(&["query", "--sort", "title", "--format", "json"]);
    let ids = |output: &common::Output| -> Vec<String> {
        output
            .stdout_json()
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["id"].as_str().unwrap().to_owned())
            .collect()
    };
    ascending.assert_code(0);
    assert_eq!(ids(&ascending), ["task-1", "task-2"]);

    let descending = Project::new().run(&["query", "--sort", "title:desc", "--format", "json"]);
    descending.assert_code(0);
    assert_eq!(ids(&descending), ["task-2", "task-1"]);
}

#[test]
fn fields_flag_picks_the_columns() {
    let output = Project::new().run(&["query", "--fields", "id,points", "--format", "json"]);
    output.assert_code(0);
    let rows = output.stdout_json();
    let first = rows[0].as_object().expect("a JSON object");
    let mut columns: Vec<&String> = first.keys().collect();
    columns.sort();
    assert_eq!(columns, [&"id".to_owned(), &"points".to_owned()]);
}

#[test]
fn csv_format_prints_a_header_and_comma_separated_rows() {
    Project::new()
        .run(&["query", "--fields", "id,status", "--format", "csv"])
        .assert_code(0)
        .assert_stdout_contains("id,status\n")
        .assert_stdout_contains("task-2,done\n");
}

#[test]
fn tsv_format_separates_with_tabs() {
    Project::new()
        .run(&["query", "--fields", "id,status", "--format", "tsv"])
        .assert_code(0)
        .assert_stdout_contains("id\tstatus\n")
        .assert_stdout_contains("task-2\tdone\n");
}

#[test]
fn delimiter_flag_replaces_the_separator() {
    Project::new()
        .run(&[
            "query",
            "--fields",
            "id,status",
            "--format",
            "csv",
            "--delimiter",
            "|",
        ])
        .assert_code(0)
        .assert_stdout_contains("id|status\n");
}

#[test]
fn no_header_flag_drops_the_header_row() {
    let output = Project::new().run(&[
        "query",
        "--fields",
        "id,status",
        "--format",
        "csv",
        "--no-header",
    ]);
    output.assert_code(0);
    assert!(
        !output.stdout.contains("id,status"),
        "header still present:\n{}",
        output.stdout
    );
    output.assert_stdout_contains("task-1,open\n");
}

#[test]
fn as_of_moves_the_evaluation_date() {
    const OVERDUE_SCHEMA: &str = "\
fields:
  title:
    type: string
    required: false
    default: $filename_pretty
  status:
    type: choice
    values: [open, in_progress, done]
    required: true
    default: open
  due:
    type: date
    required: false
  overdue:
    type: boolean
    compute: due < $today
  parent:
    type: link
    required: false
    allow_cycles: false
    inverse: children
";
    let project = Project::with_schema(OVERDUE_SCHEMA);
    // `task-1` is due 2026-01-10.
    let before = project.run(&[
        "query",
        "--where",
        "overdue=true",
        "--as-of",
        "2026-01-01",
        "--format",
        "json",
    ]);
    before.assert_code(0);
    assert_eq!(before.stdout_json(), serde_json::json!([]));

    let after = project.run(&[
        "query",
        "--where",
        "overdue=true",
        "--as-of",
        "2026-02-01",
        "--format",
        "json",
    ]);
    after.assert_code(0);
    assert_eq!(after.stdout_json()[0]["id"], "task-1");
}

#[test]
fn unparseable_where_clause_exits_one() {
    Project::new()
        .run(&["query", "--where", "==="])
        .assert_code(1)
        .assert_stderr_contains("filter expression");
}

#[test]
fn unknown_format_is_a_malformed_invocation() {
    Project::new()
        .run(&["query", "--format", "xml"])
        .assert_code(2);
}
