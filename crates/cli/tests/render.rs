//! `workdown render`: writes one Markdown file per view under `views/`.

mod common;

use common::Project;

#[test]
fn renders_every_view_into_the_output_directory() {
    let project = Project::new();
    project
        .run(&["render"])
        .assert_code(0)
        .assert_stderr_contains("Wrote");
    assert!(project.exists("views/status-board.md"));
    assert!(project.exists("views/item-table.md"));
}

#[test]
fn a_view_id_renders_only_that_view() {
    let project = Project::new();
    project.run(&["render", "item-table"]).assert_code(0);
    assert!(project.exists("views/item-table.md"));
    assert!(!project.exists("views/status-board.md"));
}

#[test]
fn as_of_pins_the_evaluation_date() {
    const TODAY_SCHEMA: &str = "\
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
    // A schema that reads `$today` makes render announce the date it
    // evaluated at; the flag shows up as that date.
    Project::with_schema(TODAY_SCHEMA)
        .run(&["render", "--as-of", "2026-02-01"])
        .assert_code(0)
        .assert_stderr_contains("as of 2026-02-01");
}

#[test]
fn unknown_view_id_exits_one() {
    Project::new()
        .run(&["render", "no-such-view"])
        .assert_code(1)
        .assert_stderr_contains("no view with id 'no-such-view'");
}

#[test]
fn unparseable_as_of_is_a_malformed_invocation() {
    Project::new()
        .run(&["render", "--as-of", "soon"])
        .assert_code(2);
}
