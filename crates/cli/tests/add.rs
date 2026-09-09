//! `workdown add`: the one command that parses its own flags.
//!
//! Its flags come from the project's schema, so clap cannot reject a bad
//! one before the config is loaded; the `2` for a malformed invocation
//! is returned by `main.rs` itself. That makes this the command where
//! "wrong command line" and "the operation refused" are easiest to mix
//! up, and both are pinned here.

mod common;

use common::Project;

#[test]
fn title_flag_reaches_the_operation_and_names_the_file() {
    let project = Project::new();
    project
        .run(&["add", "--title", "Write the docs"])
        .assert_code(0)
        .assert_stderr_contains("Created");
    assert!(project
        .item("write-the-docs")
        .contains("title: Write the docs"));
}

#[test]
fn a_schema_field_flag_lands_in_the_frontmatter() {
    let project = Project::new();
    project
        .run(&["add", "--title", "Urgent one", "--status", "in_progress"])
        .assert_code(0);
    assert!(project.item("urgent-one").contains("status: in_progress"));
}

#[test]
fn template_flag_applies_the_template() {
    let project = Project::new();
    project
        .run(&["add", "--title", "From template", "--template", "bug"])
        .assert_code(0);
    let item = project.item("from-template");
    assert!(
        item.contains("templated"),
        "template field missing:\n{item}"
    );
    assert!(item.contains("Body from the template."));
}

#[test]
fn help_for_the_dynamic_flags_exits_zero() {
    Project::new()
        .run(&["add", "--help"])
        .assert_code(0)
        .assert_stdout_contains("--status");
}

#[test]
fn a_value_outside_the_choices_is_a_malformed_invocation() {
    // Unlike `set`, the schema-built flags are typed: a choice flag only
    // accepts its values, so the parse rejects this before any file is
    // written and the exit code is clap's `2`, not save-with-warning's `1`.
    let project = Project::new();
    project
        .run(&["add", "--title", "Odd status", "--status", "bogus"])
        .assert_code(2);
    assert!(!project.exists("workdown-items/odd-status.md"));
}

#[test]
fn missing_title_is_the_operation_refusing() {
    Project::new()
        .run(&["add"])
        .assert_code(1)
        .assert_stderr_contains("provide --id or --title");
}

#[test]
fn existing_item_is_the_operation_refusing() {
    Project::new()
        .run(&["add", "--title", "Task 1"])
        .assert_code(1)
        .assert_stderr_contains("already exists");
}

#[test]
fn unknown_flag_is_a_malformed_invocation() {
    Project::new()
        .run(&["add", "--title", "X", "--no-such-field", "y"])
        .assert_code(2);
}
