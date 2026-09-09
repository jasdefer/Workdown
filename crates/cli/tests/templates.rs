//! `workdown templates list` and `workdown templates show`.

mod common;

use common::Project;

#[test]
fn list_prints_the_template_names() {
    Project::new()
        .run(&["templates", "list"])
        .assert_code(0)
        .assert_stdout_contains("bug");
}

#[test]
fn list_json_format_prints_name_and_path() {
    let output = Project::new().run(&["templates", "list", "--format", "json"]);
    output.assert_code(0);
    let entries = output.stdout_json();
    assert_eq!(entries[0]["name"], "bug");
    assert!(entries[0]["path"]
        .as_str()
        .expect("path is a string")
        .ends_with("bug.md"));
}

#[test]
fn show_prints_the_raw_template() {
    Project::new()
        .run(&["templates", "show", "bug"])
        .assert_code(0)
        .assert_stdout_contains("Body from the template.");
}

#[test]
fn show_of_an_unknown_template_exits_one() {
    Project::new()
        .run(&["templates", "show", "nope"])
        .assert_code(1)
        .assert_stderr_contains("template 'nope' not found");
}

#[test]
fn show_without_a_name_is_a_malformed_invocation() {
    Project::new().run(&["templates", "show"]).assert_code(2);
}

#[test]
fn templates_without_an_action_is_a_malformed_invocation() {
    Project::new().run(&["templates"]).assert_code(2);
}
