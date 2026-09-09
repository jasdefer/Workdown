//! `workdown validate`: the exit code is the product.
//!
//! The command is the one whose exit code other tooling depends on (the
//! generated pre-commit hook), and a wrong code prints nothing and looks
//! like success. Each case here would pass with the `!` dropped from
//! `exit_code(!result.has_errors)` in `main.rs` — except the ones that
//! matter.

mod common;

use common::Project;

/// The default schema plus a field that depends on the evaluation date
/// and a rule that fires when it is set. A due date in the past is an
/// error when evaluated today and clean when evaluated before it.
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
  owner:
    type: string
    required: false
  parent:
    type: link
    required: false
    allow_cycles: false
    inverse: children
rules:
  - name: overdue-needs-owner
    description: Overdue items must name an owner
    match:
      overdue: true
    require:
      owner: required
";

#[test]
fn clean_project_exits_zero() {
    Project::new()
        .run(&["validate"])
        .assert_code(0)
        .assert_stderr_contains("No issues found");
}

#[test]
fn error_in_an_item_exits_one() {
    let project = Project::new();
    project.write(
        "workdown-items/task-3.md",
        "---\nstatus: open\nparent: nowhere\n---\n",
    );
    project
        .run(&["validate"])
        .assert_code(1)
        .assert_stderr_contains("broken link to 'nowhere'");
}

#[test]
fn json_format_prints_the_diagnostics_as_json() {
    let project = Project::new();
    project.write(
        "workdown-items/task-3.md",
        "---\nstatus: open\nparent: nowhere\n---\n",
    );
    let output = project.run(&["validate", "--format", "json"]);
    output.assert_code(1);
    let diagnostics = output.stdout_json();
    let diagnostics = diagnostics.as_array().expect("a JSON array");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0]["severity"], "error");
}

#[test]
fn json_format_on_a_clean_project_is_an_empty_array() {
    let output = Project::new().run(&["validate", "--format", "json"]);
    output.assert_code(0);
    assert_eq!(output.stdout_json(), serde_json::json!([]));
}

#[test]
fn as_of_moves_the_evaluation_date() {
    let project = Project::with_schema(OVERDUE_SCHEMA);
    // `task-1` is due 2026-01-10 and names no owner.
    project
        .run(&["validate", "--as-of", "2026-01-01"])
        .assert_code(0);
    project
        .run(&["validate", "--as-of", "2026-02-01"])
        .assert_code(1)
        .assert_stderr_contains("overdue-needs-owner");
}

#[test]
fn unknown_format_is_a_malformed_invocation() {
    Project::new()
        .run(&["validate", "--format", "xml"])
        .assert_code(2);
}

#[test]
fn unparseable_as_of_is_a_malformed_invocation() {
    Project::new()
        .run(&["validate", "--as-of", "yesterday"])
        .assert_code(2);
}
