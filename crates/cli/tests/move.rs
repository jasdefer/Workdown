//! `workdown move`: `set` on the board field named in config.yaml.

mod common;

use common::Project;

#[test]
fn move_sets_the_board_field() {
    let project = Project::new();
    project
        .run(&["move", "task-1", "done"])
        .assert_code(0)
        .assert_stderr_contains("task-1: status: open → done");
    assert!(project.item("task-1").contains("status: done"));
}

#[test]
fn unknown_item_exits_one() {
    Project::new()
        .run(&["move", "nope", "done"])
        .assert_code(1)
        .assert_stderr_contains("unknown work item 'nope'");
}

#[test]
fn board_field_missing_from_the_schema_exits_one() {
    // The field name comes from config, not from the user, so the CLI
    // names the config key rather than the generic unknown-field error.
    let project = Project::new();
    project.write(
        ".workdown/config.yaml",
        &common::CONFIG.replace("board_field: status", "board_field: lane"),
    );
    project
        .run(&["move", "task-1", "done"])
        .assert_code(1)
        .assert_stderr_contains("board field 'lane'");
}

#[test]
fn missing_value_is_a_malformed_invocation() {
    Project::new().run(&["move", "task-1"]).assert_code(2);
}
