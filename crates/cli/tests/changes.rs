//! `workdown changes`: prints the commit message the web UI would use.
//!
//! Each test creates its own git repository inside the temp directory
//! and only ever reads from it.

mod common;

use common::Project;

#[test]
fn describes_the_uncommitted_change_by_item_title() {
    let project = Project::new();
    project.init_git_repository();
    project
        .run(&["set", "task-1", "status", "done"])
        .assert_code(0);
    project
        .run(&["changes"])
        .assert_code(0)
        .assert_stdout_contains("Task 1");
}

#[test]
fn clean_tree_prints_no_changes() {
    let project = Project::new();
    project.init_git_repository();
    project
        .run(&["changes"])
        .assert_code(0)
        .assert_stdout_contains("No changes");
}

#[test]
fn files_flag_lists_the_paths_first() {
    let project = Project::new();
    project.init_git_repository();
    project
        .run(&["set", "task-1", "status", "done"])
        .assert_code(0);
    project
        .run(&["changes", "--files"])
        .assert_code(0)
        .assert_stdout_contains("workdown-items/task-1.md");
}

#[test]
fn outside_a_repository_exits_one() {
    Project::new()
        .run(&["changes"])
        .assert_code(1)
        .assert_stderr_contains("not inside a git repository");
}

#[test]
fn unknown_flag_is_a_malformed_invocation() {
    Project::new().run(&["changes", "--bogus"]).assert_code(2);
}
