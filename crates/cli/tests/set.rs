//! `workdown set`: five modes behind one clap `ArgGroup`, each mapped to
//! a different core operation.
//!
//! One case per mode proves the mode arrived; the headline the CLI
//! prints is asserted because it is the CLI's own output, not core's.
//! What each mode does to the value is core's contract and lives in
//! `crates/core/tests/`.

mod common;

use common::Project;

#[test]
fn replace_reaches_the_operation() {
    let project = Project::new();
    project
        .run(&["set", "task-1", "status", "done"])
        .assert_code(0)
        .assert_stderr_contains("task-1: status: open → done");
    assert!(project.item("task-1").contains("status: done"));
}

#[test]
fn append_flag_selects_the_append_mode() {
    Project::new()
        .run(&["set", "task-1", "tags", "--append", "beta"])
        .assert_code(0)
        .assert_stderr_contains("tags: [alpha] + [beta] = [alpha, beta]");
}

#[test]
fn remove_flag_selects_the_remove_mode() {
    Project::new()
        .run(&["set", "task-1", "tags", "--remove", "alpha"])
        .assert_code(0)
        .assert_stderr_contains("tags: [alpha] - [alpha] = []");
}

#[test]
fn delta_flag_selects_the_delta_mode() {
    Project::new()
        .run(&["set", "task-1", "points", "--delta", "2"])
        .assert_code(0)
        .assert_stderr_contains("points: 3 + 2 = 5");
}

#[test]
fn negative_delta_is_a_value_not_a_flag() {
    Project::new()
        .run(&["set", "task-1", "points", "--delta", "-1"])
        .assert_code(0)
        .assert_stderr_contains("points: 3 - 1 = 2");
}

#[test]
fn toggle_flag_selects_the_toggle_mode() {
    Project::new()
        .run(&["set", "task-1", "urgent", "--toggle"])
        .assert_code(0)
        .assert_stderr_contains("urgent: false → true");
}

#[test]
fn a_mutation_that_causes_a_warning_saves_and_exits_one() {
    // The save-with-warning rule: the file changes, the command fails.
    let project = Project::new();
    project
        .run(&["set", "task-1", "status", "bogus"])
        .assert_code(1);
    assert!(project.item("task-1").contains("status: bogus"));
}

#[test]
fn unknown_item_exits_one() {
    Project::new()
        .run(&["set", "nope", "status", "done"])
        .assert_code(1)
        .assert_stderr_contains("unknown work item 'nope'");
}

#[test]
fn unknown_field_exits_one() {
    Project::new()
        .run(&["set", "task-1", "nope", "x"])
        .assert_code(1)
        .assert_stderr_contains("unknown field 'nope'");
}

#[test]
fn two_modes_at_once_is_a_malformed_invocation() {
    Project::new()
        .run(&["set", "task-1", "status", "done", "--toggle"])
        .assert_code(2);
}

#[test]
fn no_mode_at_all_is_a_malformed_invocation() {
    Project::new()
        .run(&["set", "task-1", "status"])
        .assert_code(2);
}
