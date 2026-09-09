//! `workdown body`: replaces the Markdown below the frontmatter.

mod common;

use common::Project;

#[test]
fn body_reaches_the_operation() {
    let project = Project::new();
    project
        .run(&["body", "task-1", "Line one\nLine two"])
        .assert_code(0)
        .assert_stderr_contains("task-1: body replaced (2 lines)");
    let item = project.item("task-1");
    assert!(item.contains("Line two"));
    assert!(!item.contains("First task."));
}

#[test]
fn empty_string_clears_the_body() {
    let project = Project::new();
    project
        .run(&["body", "task-1", ""])
        .assert_code(0)
        .assert_stderr_contains("body replaced (0 lines)");
    assert!(!project.item("task-1").contains("First task."));
}

#[test]
fn unknown_item_exits_one() {
    Project::new()
        .run(&["body", "nope", "text"])
        .assert_code(1)
        .assert_stderr_contains("unknown work item 'nope'");
}

#[test]
fn missing_body_argument_is_a_malformed_invocation() {
    Project::new().run(&["body", "task-1"]).assert_code(2);
}
