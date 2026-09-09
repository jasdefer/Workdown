//! `workdown rename`: moves the file and rewrites incoming links.

mod common;

use common::Project;

#[test]
fn rename_moves_the_file_and_rewrites_the_link() {
    let project = Project::new();
    project
        .run(&["rename", "task-1", "first-task"])
        .assert_code(0)
        .assert_stderr_contains("task-1 → first-task");
    assert!(project.exists("workdown-items/first-task.md"));
    assert!(!project.exists("workdown-items/task-1.md"));
    assert!(project.item("task-2").contains("parent: first-task"));
}

#[test]
fn dry_run_flag_leaves_the_files_alone() {
    let project = Project::new();
    project
        .run(&["rename", "task-1", "first-task", "--dry-run"])
        .assert_code(0)
        .assert_stderr_contains("[dry run]");
    assert!(project.exists("workdown-items/task-1.md"));
    assert!(!project.exists("workdown-items/first-task.md"));
    assert!(project.item("task-2").contains("parent: task-1"));
}

#[test]
fn unknown_item_exits_one() {
    Project::new()
        .run(&["rename", "nope", "other"])
        .assert_code(1)
        .assert_stderr_contains("unknown work item 'nope'");
}

#[test]
fn missing_new_id_is_a_malformed_invocation() {
    Project::new().run(&["rename", "task-1"]).assert_code(2);
}
