//! `workdown unset`: clears one field.

mod common;

use common::Project;

#[test]
fn unset_reaches_the_operation() {
    let project = Project::new();
    project
        .run(&["unset", "task-1", "points"])
        .assert_code(0)
        .assert_stderr_contains("task-1: points: 3 → (cleared)");
    assert!(!project.item("task-1").contains("points:"));
}

#[test]
fn unknown_field_exits_one() {
    Project::new()
        .run(&["unset", "task-1", "nope"])
        .assert_code(1)
        .assert_stderr_contains("unknown field 'nope'");
}

#[test]
fn missing_field_argument_is_a_malformed_invocation() {
    Project::new().run(&["unset", "task-1"]).assert_code(2);
}
